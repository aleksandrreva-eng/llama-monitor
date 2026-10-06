//! Generic OpenAI-compatible cloud adapter (OpenAI, OpenRouter, Groq, Together).
//!
//! These providers expose `/v1/models` and nothing else useful for monitoring:
//! there is no health endpoint, no live context, and no throughput reading. The
//! model name plus its advertised context window is all we can honestly show.

use crate::api::adapters::{Adapter, ParsedSnapshot};
use crate::api::parse::model::{model_from_entry, ModelsResponse};
use crate::api::probes::{Probe, ProbeReport};

pub const MODELS: &str = "models";

pub struct Cloud;

impl Adapter for Cloud {
    fn probes(&self, _native: &str, openai: &str) -> Vec<Probe> {
        // `/v1/models` doubles as the reachability check: it is the only
        // endpoint we call, so a failure means the provider is unusable.
        vec![Probe::required(MODELS, format!("{openai}/models"))]
    }

    fn parse(&self, report: &ProbeReport) -> ParsedSnapshot {
        let mut snap = ParsedSnapshot::default();

        if let Some(body) = report.body(MODELS) {
            match serde_json::from_str::<ModelsResponse>(body) {
                Ok(json) => {
                    if let Some(entry) = json.models.as_deref().and_then(|v| v.first()) {
                        snap.model = model_from_entry(entry);
                    }
                }
                Err(e) => log::debug!("cloud models parse failed: {e}"),
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

    #[test]
    fn only_the_openai_root_is_polled() {
        let probes = Cloud.probes("https://api.openai.com/v1", "https://api.openai.com/v1");
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].url, "https://api.openai.com/v1/models");
        assert!(probes[0].required);
    }

    #[test]
    fn reads_name_and_context_window() {
        let body = r#"{"models":[{"id":"gpt-4o-mini","context_length":128000}]}"#;
        let snap = Cloud.parse(&report(&[(MODELS, ok(body))]));
        assert_eq!(snap.model.name.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(snap.model.context_size, Some(128_000));
        // Cloud never reports a loaded flag, live context or speeds.
        assert!(!snap.model.loaded);
        assert!(!snap.context.available);
        assert!(!snap.prefill_speed.available);
    }

    /// A provider listing no models must not invent one.
    #[test]
    fn empty_model_list_yields_no_name() {
        let snap = Cloud.parse(&report(&[(MODELS, ok(r#"{"models":[]}"#))]));
        assert!(snap.model.name.is_none());
        assert_eq!(snap.model.context_size, None);
    }
}
