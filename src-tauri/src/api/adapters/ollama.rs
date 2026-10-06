//! Ollama adapter.
//!
//! Ollama's native API lives under `/api`; `/api/ps` lists the running models
//! and is the only endpoint that says whether the engine is up at all. Ollama
//! exposes no live throughput, so the speed cards stay empty by design.

use serde_json::Value;

use crate::api::adapters::{Adapter, ParsedSnapshot};
use crate::api::probes::{Probe, ProbeReport};
use crate::api::types::{ContextMetric, DiagCode, ModelMetric};

/// Probe id. The native base already includes `/api`, so the URL is `<base>/ps`.
pub const PS: &str = "ps";

pub struct Ollama;

impl Adapter for Ollama {
    fn probes(&self, native: &str, _openai: &str) -> Vec<Probe> {
        // `/api/ps` is REQUIRED. Treating it as best-effort — as the previous
        // implementation did by swallowing the error — made a dead Ollama server
        // indistinguishable from a live one with missing metrics, so the widget
        // never showed "server unavailable" for Ollama.
        vec![Probe::required(PS, format!("{native}/ps"))]
    }

    fn parse(&self, report: &ProbeReport) -> ParsedSnapshot {
        let mut snap = ParsedSnapshot::default();

        if let Some(body) = report.body(PS) {
            match serde_json::from_str::<Value>(body) {
                Ok(json) => {
                    if detect_kind_from_ps(&json) {
                        let (model, ctx) = parse_ollama_ps(&json);
                        if let Some(model) = model {
                            snap.model = model;
                        }
                        if let Some(ctx) = ctx {
                            snap.context = ctx;
                        }
                    } else {
                        // Reachable, but nothing is loaded: a real state the user
                        // needs to see, not an error.
                        snap.diag(DiagCode::OllamaNoModels);
                    }
                }
                Err(e) => log::debug!("ollama /api/ps parse failed: {e}"),
            }
        }

        snap
    }
}

/// Detect Ollama from the `/api/ps` payload: an array whose entries are running
/// models carrying a `name`.
pub fn detect_kind_from_ps(value: &Value) -> bool {
    value
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("name"))
        .and_then(|v| v.as_str())
        .is_some()
}

/// Extract the model name and context window from an Ollama `/api/ps` entry.
///
/// Ollama reports each running model with a `name` and an `info` object that
/// may contain `context_length` (the model's configured window).
pub fn extract_ollama_model(entry: &Value) -> ModelMetric {
    let info = entry.get("info").and_then(|v| v.as_object());
    let context_size = info
        .and_then(|i| i.get("context_length"))
        .and_then(|v| v.as_u64())
        .or_else(|| entry.get("context_length").and_then(|v| v.as_u64()));
    let name = entry
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    ModelMetric {
        name,
        context_size,
        loaded: true,
        quantization: info
            .and_then(|i| i.get("quantize_level"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        path: None,
        version: None,
    }
}

/// Parse an Ollama `/api/ps` array: the running model plus context usage.
///
/// Ollama reports how many context tokens each model currently has loaded via
/// the `context` field (used) — when present. `total` comes from
/// `context_length` if the entry carries it.
///
/// The model and the context are returned independently: an entry that names a
/// model but reports no usage must still yield the model name (the previous
/// implementation required both to be present and dropped the name otherwise).
pub fn parse_ollama_ps(value: &Value) -> (Option<ModelMetric>, Option<ContextMetric>) {
    let Some(first) = value.as_array().and_then(|arr| arr.first()) else {
        return (None, None);
    };
    let model = extract_ollama_model(first);

    // Ollama's `context` field (in the entry) is the number of context tokens
    // currently loaded for that model.
    let used = first.get("context").and_then(|v| v.as_u64());
    let total = model.context_size;
    let ctx = match (used, total) {
        (Some(u), Some(t)) if t > 0 => {
            let remaining = t.saturating_sub(u);
            let percent = (u as f64 / t as f64).clamp(0.0, 1.0);
            Some(ContextMetric {
                total: Some(t),
                used: Some(u),
                remaining: Some(remaining),
                percent: Some(percent),
                available: true,
            })
        }
        (None, Some(t)) if t > 0 => Some(ContextMetric {
            total: Some(t),
            used: None,
            remaining: None,
            percent: None,
            available: true,
        }),
        _ => None,
    };
    (Some(model), ctx)
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

    #[test]
    fn native_base_gets_the_ps_suffix() {
        let probes = Ollama.probes("http://127.0.0.1:11434/api", "http://127.0.0.1:11434/v1");
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].url, "http://127.0.0.1:11434/api/ps");
        assert!(probes[0].required, "a dead Ollama must not look healthy");
    }

    #[test]
    fn parses_running_model_and_context() {
        let body = r#"[{
            "name": "llama3:8b",
            "context": 2048,
            "info": { "context_length": 8192, "quantize_level": "Q4_K_M" }
        }]"#;
        let snap = Ollama.parse(&report(&[(PS, ok(body))]));
        assert_eq!(snap.model.name.as_deref(), Some("llama3:8b"));
        assert_eq!(snap.model.context_size, Some(8192));
        assert_eq!(snap.model.quantization.as_deref(), Some("Q4_K_M"));
        assert!(snap.context.available);
        assert_eq!(snap.context.used, Some(2048));
        assert_eq!(snap.context.remaining, Some(6144));
        assert!(snap.diagnostics.is_empty());
    }

    /// A model without a `context` field must still be named.
    #[test]
    fn model_is_kept_even_without_usage() {
        let body = r#"[{ "name": "phi3", "info": { "context_length": 4096 } }]"#;
        let snap = Ollama.parse(&report(&[(PS, ok(body))]));
        assert_eq!(snap.model.name.as_deref(), Some("phi3"));
        // Total known, used unknown -> available, but no percent.
        assert!(snap.context.available);
        assert_eq!(snap.context.used, None);
        assert_eq!(snap.context.percent, None);
    }

    /// An empty list means "Ollama is up, nothing is loaded".
    #[test]
    fn empty_list_reports_no_running_model() {
        let snap = Ollama.parse(&report(&[(PS, ok("[]"))]));
        assert!(snap.model.name.is_none());
        assert!(snap.diagnostics.contains(&DiagCode::OllamaNoModels));
    }

    #[test]
    fn non_ollama_payload_is_reported() {
        let snap = Ollama.parse(&report(&[(PS, ok(r#"{"error":"not found"}"#))]));
        assert!(snap.diagnostics.contains(&DiagCode::OllamaNoModels));
    }

    #[test]
    fn detect_kind_requires_a_named_entry() {
        assert!(detect_kind_from_ps(&serde_json::json!([{ "name": "x" }])));
        assert!(!detect_kind_from_ps(&serde_json::json!([])));
        assert!(!detect_kind_from_ps(&serde_json::json!({})));
    }
}
