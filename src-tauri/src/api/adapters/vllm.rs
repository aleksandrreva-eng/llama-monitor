//! vLLM adapter.
//!
//! vLLM exposes `/health` at the root, an OpenAI-compatible `/v1/models`, and a
//! Prometheus `/metrics` carrying `vllm:`-namespaced series. It gives live
//! context (KV-cache occupancy) and per-token latencies, so context percent and
//! both speeds come from `/metrics`.

use crate::api::adapters::{detect_kind_from_metrics, Adapter, ParsedSnapshot};
use crate::api::parse::metrics::{extract_vllm_context, extract_vllm_speeds};
use crate::api::parse::model::{model_from_entry, ModelsResponse};
use crate::api::probes::{Probe, ProbeReport};
use crate::api::types::{ContextMetric, DiagCode};
use crate::config::ServerKind;

pub const HEALTH: &str = "health";
pub const MODELS: &str = "models";
pub const METRICS: &str = "metrics";

pub struct Vllm;

impl Adapter for Vllm {
    fn probes(&self, native: &str, openai: &str) -> Vec<Probe> {
        vec![
            Probe::required(HEALTH, format!("{native}/health")),
            Probe::optional(MODELS, format!("{openai}/models")),
            Probe::optional(METRICS, format!("{native}/metrics")),
        ]
    }

    fn parse(&self, report: &ProbeReport) -> ParsedSnapshot {
        let mut snap = ParsedSnapshot::default();

        // Model name from /v1/models.
        if let Some(body) = report.body(MODELS) {
            match serde_json::from_str::<ModelsResponse>(body) {
                Ok(json) => {
                    if let Some(entry) = json.models.as_deref().and_then(|v| v.first()) {
                        snap.model = model_from_entry(entry);
                    }
                }
                Err(e) => log::debug!("vLLM models parse failed: {e}"),
            }
        }

        // Metrics (Prometheus, vllm: prefix) — context (KV occupancy) + speeds.
        if let Some(body) = report.body(METRICS) {
            if let Some(ServerKind::Vllm) = detect_kind_from_metrics(body) {
                let ctx = extract_vllm_context(body);
                if let Some(percent) = ctx.percent {
                    snap.context = ContextMetric {
                        total: ctx.total,
                        used: ctx.used,
                        remaining: None,
                        percent: Some(percent),
                        available: true,
                    };
                }
                let (prefill, generation) = extract_vllm_speeds(body);
                snap.prefill_speed = prefill;
                snap.generation_speed = generation;
            } else {
                snap.diag(DiagCode::VllmMetricsMissing);
            }
        } else if report.is_missing(METRICS) {
            snap.diag(DiagCode::MetricsDisabled);
        }

        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::probes::ProbeOutcome;
    use std::collections::HashMap;

    fn report(pairs: &[(&'static str, ProbeOutcome)]) -> ProbeReport {
        let mut outcomes = HashMap::new();
        for (id, outcome) in pairs {
            outcomes.insert(*id, outcome.clone());
        }
        ProbeReport::from_outcomes(outcomes)
    }

    fn ok(body: &str) -> ProbeOutcome {
        ProbeOutcome::Ok(body.to_string())
    }

    const VLLM_METRICS: &str = "\
vllm:kv_cache_usage_sys 512
vllm:kv_cache_usage_max 2048
vllm:time_to_first_token_seconds 0.25
vllm:inter_token_latency_seconds 0.05
";

    #[test]
    fn parses_models_and_metrics() {
        let models = r#"{"models":[{"id":"meta-llama/Llama-3-8B","context_length":8192}]}"#;
        let snap = Vllm.parse(&report(&[
            (HEALTH, ok("")),
            (MODELS, ok(models)),
            (METRICS, ok(VLLM_METRICS)),
        ]));
        assert_eq!(snap.model.name.as_deref(), Some("meta-llama/Llama-3-8B"));
        assert_eq!(snap.model.context_size, Some(8192));
        assert!(snap.context.available);
        assert_eq!(snap.context.used, Some(512));
        assert_eq!(snap.context.percent, Some(0.25));
        assert!((snap.prefill_speed.current.unwrap() - 4.0).abs() < 1e-9);
        assert!(snap.diagnostics.is_empty());
    }

    /// A `/metrics` endpoint without any `vllm:` series means we are pointed at
    /// something else — say so instead of showing empty cards silently.
    #[test]
    fn metrics_without_vllm_prefix_is_reported() {
        let snap = Vllm.parse(&report(&[
            (HEALTH, ok("")),
            (METRICS, ok("go_gc_duration 1\n")),
        ]));
        assert!(snap.diagnostics.contains(&DiagCode::VllmMetricsMissing));
    }

    /// Metrics switched off entirely is a different condition from "wrong
    /// engine", and the UI phrases them differently.
    #[test]
    fn metrics_501_is_reported_as_disabled() {
        let snap = Vllm.parse(&report(&[
            (HEALTH, ok("")),
            (METRICS, ProbeOutcome::Status(501)),
        ]));
        assert!(snap.diagnostics.contains(&DiagCode::MetricsDisabled));
        assert!(!snap.diagnostics.contains(&DiagCode::VllmMetricsMissing));
    }

    /// With no KV-cache counters there is no occupancy reading, and the context
    /// metric must stay unavailable rather than reporting a fake zero.
    #[test]
    fn missing_kv_counters_leave_context_unavailable() {
        let snap = Vllm.parse(&report(&[
            (HEALTH, ok("")),
            (METRICS, ok("vllm:num_requests_running 3\n")),
        ]));
        assert!(!snap.context.available);
        assert_eq!(snap.context.percent, None);
    }
}
