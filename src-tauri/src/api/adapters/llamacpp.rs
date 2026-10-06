//! llama.cpp adapter (local or remote).
//!
//! Polls the native root endpoints plus the OpenAI-compatible model list:
//! `/health`, `/v1/models`, `/props`, `/metrics`, `/slots`.

use crate::api::adapters::{Adapter, ParsedSnapshot};
use crate::api::parse::metrics::{collect_other_metrics, extract_context, extract_speeds};
use crate::api::parse::model::{
    extract_model, extract_model_from_props, extract_server_label, merge_model, ModelsResponse,
    PropsResponse,
};
use crate::api::parse::prometheus::parse_metrics_body;
use crate::api::parse::slots::parse_slots;
use crate::api::probes::{Probe, ProbeReport};
use crate::api::types::{DiagCode, SpeedMetric};

/// Probe ids.
pub const HEALTH: &str = "health";
pub const MODELS: &str = "models";
pub const PROPS: &str = "props";
pub const METRICS: &str = "metrics";
pub const SLOTS: &str = "slots";

pub struct LlamaCpp;

impl Adapter for LlamaCpp {
    fn probes(&self, native: &str, openai: &str) -> Vec<Probe> {
        vec![
            // The health check is the only required probe: it is what separates
            // "server down" from "server up but this field is missing".
            Probe::required(HEALTH, format!("{native}/health")),
            Probe::optional(MODELS, format!("{openai}/models")),
            Probe::optional(PROPS, format!("{native}/props")),
            Probe::optional(METRICS, format!("{native}/metrics")),
            Probe::optional(SLOTS, format!("{native}/slots")),
        ]
    }

    fn parse(&self, report: &ProbeReport) -> ParsedSnapshot {
        let mut snap = ParsedSnapshot::default();

        // 1. Models (OpenAI-compatible route, lives under /v1) — model name.
        if let Some(body) = report.body(MODELS) {
            match serde_json::from_str::<ModelsResponse>(body) {
                Ok(json) => snap.model = extract_model(&json),
                Err(e) => log::debug!("models parse failed: {e}"),
            }
        }

        // 2. Props (native endpoint, server root) — model name, context size,
        //    quantization. Props is the authoritative source for the *loaded*
        //    model, so it wins on any field it actually provides; /v1/models
        //    only fills the gaps.
        if let Some(body) = report.body(PROPS) {
            match serde_json::from_str::<PropsResponse>(body) {
                Ok(json) => {
                    merge_model(&mut snap.model, extract_model_from_props(&json));
                    snap.server_label = extract_server_label(&json).or(snap.server_label);
                }
                Err(e) => log::debug!("props parse failed: {e}"),
            }
        }

        // 3. Metrics (speeds; context only on builds that publish it).
        //
        //    Many builds run WITHOUT --metrics (HTTP 501); in that case we fall
        //    back to /slots below, which is on by default. Only a *definitive*
        //    501/404 produces a diagnostic — transient failures (connection
        //    refused, 502 during model reload, timeouts) stay silent, otherwise
        //    every restart would flash a misleading warning.
        let got_metrics = report.is_ok(METRICS);
        if let Some(body) = report.body(METRICS) {
            let map = parse_metrics_body(body);
            let ctx = extract_context(&map);
            if ctx.available {
                snap.context = ctx;
            }
            let (prefill, generation) = extract_speeds(&map);
            snap.prefill_speed = prefill;
            snap.generation_speed = generation;
            // Raw counters for the expanded view (informational only).
            snap.other_metrics = collect_other_metrics(&map);
        } else if report.is_missing(METRICS) {
            snap.diag(DiagCode::MetricsDisabled);
        }

        // 4. Slots (native endpoint, server root). Queried even when metrics are
        //    available, because /slots is the only source of the live per-slot
        //    context usage.
        if let Some(body) = report.body(SLOTS) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
                let slots = parse_slots(&json);

                // Prefer Prometheus/JSON metrics when they supplied real context
                // numbers; otherwise take what /slots reports.
                if !snap.context.available {
                    if let Some(ctx) = slots.context_opt() {
                        snap.context = ctx;
                    }
                }
                // /slots timings give the instantaneous rate, which we label as
                // "current". The avg value is filled in by the calculator.
                if !snap.prefill_speed.available {
                    if let Some(v) = slots.prefill_speed {
                        snap.prefill_speed = SpeedMetric {
                            current: Some(v),
                            available: true,
                            ..Default::default()
                        };
                    }
                }
                if !snap.generation_speed.available {
                    if let Some(v) = slots.generation_speed {
                        snap.generation_speed = SpeedMetric {
                            current: Some(v),
                            available: true,
                            ..Default::default()
                        };
                    }
                }
                if snap.model.context_size.is_none() {
                    snap.model.context_size = slots.context_size;
                }
                if !slots.active && !got_metrics {
                    snap.diag(DiagCode::ServerIdle);
                }
            }
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

    const PROPS_BODY: &str = r#"{
        "model_alias": "Ornith-1.5-35B-A3B",
        "model_ftype": "Q4_K_M",
        "model_path": "/models/Ornith-1.5-35B-A3B-Q4_K_M.gguf",
        "default_generation_settings": { "n_ctx": 32768 },
        "build_info": "b10929"
    }"#;

    /// An *active* slot: the happy path has the server mid-request, so no
    /// `ServerIdle` diagnostic is expected.
    const SLOTS_BODY: &str = r#"[{
        "id": 0, "n_ctx": 32768, "is_processing": true,
        "n_prompt_tokens": 1200, "n_decoded": 300
    }]"#;

    #[test]
    fn probes_hit_native_and_openai_roots() {
        let probes = LlamaCpp.probes("http://127.0.0.1:8080", "http://127.0.0.1:8080/v1");
        let urls: Vec<&str> = probes.iter().map(|p| p.url.as_str()).collect();
        assert!(urls.contains(&"http://127.0.0.1:8080/health"));
        assert!(urls.contains(&"http://127.0.0.1:8080/props"));
        assert!(urls.contains(&"http://127.0.0.1:8080/metrics"));
        assert!(urls.contains(&"http://127.0.0.1:8080/slots"));
        assert!(urls.contains(&"http://127.0.0.1:8080/v1/models"));
        // Only health is fatal.
        let required: Vec<&str> = probes.iter().filter(|p| p.required).map(|p| p.id).collect();
        assert_eq!(required, vec![HEALTH]);
    }

    /// The full happy path: props gives the model, slots gives the live context.
    #[test]
    fn parses_props_and_slots() {
        let snap = LlamaCpp.parse(&report(&[
            (HEALTH, ok(r#"{"status":"ok"}"#)),
            (PROPS, ok(PROPS_BODY)),
            (SLOTS, ok(SLOTS_BODY)),
        ]));
        assert_eq!(snap.model.name.as_deref(), Some("Ornith-1.5-35B-A3B"));
        assert_eq!(snap.model.context_size, Some(32768));
        assert_eq!(snap.model.quantization.as_deref(), Some("Q4_K_M"));
        assert!(snap.context.available);
        assert_eq!(snap.context.used, Some(1500));
        assert!(snap.diagnostics.is_empty());
    }

    /// Without `--metrics` the server answers 501: that is a definitive state
    /// worth telling the user about, and /slots must still supply context.
    #[test]
    fn metrics_501_reports_disabled_and_still_uses_slots() {
        let snap = LlamaCpp.parse(&report(&[
            (HEALTH, ok(r#"{"status":"ok"}"#)),
            (PROPS, ok(PROPS_BODY)),
            (METRICS, ProbeOutcome::Status(501)),
            (SLOTS, ok(SLOTS_BODY)),
        ]));
        assert!(snap.diagnostics.contains(&DiagCode::MetricsDisabled));
        assert!(snap.context.available, "context must come from /slots");
        assert!(!snap.prefill_speed.available);
    }

    /// A transient metrics failure (502 during a reload, timeout) must stay
    /// silent — otherwise every restart flashes a misleading warning.
    #[test]
    fn transient_metrics_failure_is_silent() {
        let snap = LlamaCpp.parse(&report(&[
            (HEALTH, ok(r#"{"status":"ok"}"#)),
            (METRICS, ProbeOutcome::Status(502)),
        ]));
        assert!(snap.diagnostics.is_empty(), "got {:?}", snap.diagnostics);
    }

    /// Metrics win over /slots for the context numbers when they exist.
    #[test]
    fn metrics_context_takes_precedence_over_slots() {
        let metrics = "ctx_total 1000\nctx_used 250\n";
        let snap = LlamaCpp.parse(&report(&[
            (HEALTH, ok("{}")),
            (METRICS, ok(metrics)),
            (SLOTS, ok(SLOTS_BODY)),
        ]));
        assert_eq!(snap.context.total, Some(1000));
        assert_eq!(snap.context.used, Some(250));
        assert!(snap.prefill_speed.split_available || snap.prefill_speed.current.is_none());
    }

    /// An idle server without metrics must say so, so the speed cards can
    /// explain the missing number instead of showing a bare dash.
    #[test]
    fn idle_server_without_metrics_is_reported() {
        let idle = r#"[{ "id": 0, "n_ctx": 4096, "is_processing": false }]"#;
        let snap = LlamaCpp.parse(&report(&[(HEALTH, ok("{}")), (SLOTS, ok(idle))]));
        assert!(snap.diagnostics.contains(&DiagCode::ServerIdle));
    }

    /// A body that is not JSON at all must degrade quietly, not panic.
    #[test]
    fn malformed_bodies_do_not_panic() {
        let snap = LlamaCpp.parse(&report(&[
            (HEALTH, ok("not json")),
            (MODELS, ok("nope")),
            (PROPS, ok("<html>")),
            (SLOTS, ok("[]")),
        ]));
        assert!(snap.model.name.is_none());
        assert!(!snap.context.available);
    }
}
