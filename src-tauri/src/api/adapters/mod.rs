//! Data adapters: one implementation per upstream engine.
//!
//! Each adapter declares the endpoints it wants polled and knows how to read
//! them. The monitoring loop does not know or care which engine is behind a
//! profile — adding a new one means adding a module here, not editing the loop
//! (spec §11: "должна быть возможность добавить новые источники данных без
//! переписывания UI").
//!
//! Parsing is pure and synchronous: it receives a [`ProbeReport`] of already
//! fetched bodies, so every adapter is unit-testable without a live server.

pub mod cloud;
pub mod llamacpp;
pub mod ollama;
pub mod vllm;

use crate::api::probes::{Probe, ProbeReport};
use crate::api::types::{ContextMetric, DiagCode, ModelMetric, RawMetric, SpeedMetric};
use crate::config::ServerKind;

/// Everything the adapters managed to extract from one poll.
#[derive(Debug, Default)]
pub struct ParsedSnapshot {
    pub context: ContextMetric,
    pub prefill_speed: SpeedMetric,
    pub generation_speed: SpeedMetric,
    pub model: ModelMetric,
    /// Raw llama.cpp `/metrics` counters/gauges for the expanded view. Empty
    /// unless the server exposes `--metrics`.
    pub other_metrics: Vec<RawMetric>,
    pub server_label: Option<String>,
    /// Unix milliseconds; stamped by the caller so parsing stays pure.
    pub timestamp_ms: i64,
    pub diagnostics: Vec<DiagCode>,
}

impl ParsedSnapshot {
    /// Record a diagnostic, ignoring duplicates.
    ///
    /// Deduplication matters because the same condition can be observed from
    /// several probes in one poll, and a repeated line would push the real
    /// problem out of the compact view.
    pub fn diag(&mut self, code: DiagCode) {
        if !self.diagnostics.contains(&code) {
            self.diagnostics.push(code);
        }
    }
}

/// One upstream engine.
pub trait Adapter: Send + Sync {
    /// Endpoints to poll, in order.
    ///
    /// `native` is the engine's native root (`/health`, `/props`, `/slots`,
    /// `/metrics` for llama.cpp; `/api` for Ollama) and `openai` the
    /// OpenAI-compatible root (`/v1`).
    fn probes(&self, native: &str, openai: &str) -> Vec<Probe>;

    /// Turn the probe outcomes into a snapshot. Pure — no I/O, no clock.
    fn parse(&self, report: &ProbeReport) -> ParsedSnapshot;
}

/// The adapter for a given server kind.
pub fn adapter_for(kind: ServerKind) -> &'static dyn Adapter {
    match kind {
        ServerKind::Local => &llamacpp::LlamaCpp,
        ServerKind::Vllm => &vllm::Vllm,
        ServerKind::Ollama => &ollama::Ollama,
        ServerKind::Cloud => &cloud::Cloud,
    }
}

/// Detect the engine from a raw `/metrics` body.
///
/// - A Prometheus blob containing `vllm:` metrics → [`ServerKind::Vllm`].
/// - A Prometheus blob containing `llamacpp:` metrics → [`ServerKind::Local`].
/// - Otherwise → `None` (the caller decides whether that is an error).
pub fn detect_kind_from_metrics(text: &str) -> Option<ServerKind> {
    if text.contains("vllm:") {
        Some(ServerKind::Vllm)
    } else if text.contains("llamacpp:") {
        Some(ServerKind::Local)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `adapter_for` must return the adapter that polls *this* engine's
    /// endpoints. Each kind is identified by an endpoint only it uses: a wrong
    /// mapping would silently poll another engine's API and report "metrics
    /// unavailable" forever, with nothing in the log to show why.
    ///
    /// The bases come from a real `ServerProfile`, so this also covers the
    /// profile → native/OpenAI root derivation (notably Ollama's `/api`).
    #[test]
    fn every_kind_resolves_to_its_own_adapter() {
        let urls = |kind| {
            let profile = crate::config::ServerProfile {
                kind,
                url: "http://h:1".to_string(),
                ..Default::default()
            };
            adapter_for(kind)
                .probes(&profile.native_base_url(), &profile.openai_base_url())
                .into_iter()
                .map(|p| p.url)
                .collect::<Vec<_>>()
        };

        // llama.cpp: native root endpoints.
        let local = urls(ServerKind::Local);
        assert!(local.contains(&"http://h:1/props".to_string()), "{local:?}");
        assert!(local.contains(&"http://h:1/slots".to_string()), "{local:?}");

        // vLLM has /metrics but none of the llama.cpp-only endpoints.
        let vllm = urls(ServerKind::Vllm);
        assert!(vllm.contains(&"http://h:1/metrics".to_string()), "{vllm:?}");
        assert!(!vllm.contains(&"http://h:1/props".to_string()), "{vllm:?}");
        assert!(!vllm.contains(&"http://h:1/slots".to_string()), "{vllm:?}");

        // Ollama's native root carries /api, and /api/ps is all it has.
        assert_eq!(
            urls(ServerKind::Ollama),
            vec!["http://h:1/api/ps".to_string()]
        );

        // Cloud polls the OpenAI-compatible root only.
        assert_eq!(
            urls(ServerKind::Cloud),
            vec!["http://h:1/v1/models".to_string()]
        );
    }

    /// Every adapter must poll at least one required probe, otherwise a dead
    /// server would look like a server with missing metrics.
    #[test]
    fn every_adapter_has_a_required_probe() {
        for kind in [
            ServerKind::Local,
            ServerKind::Vllm,
            ServerKind::Ollama,
            ServerKind::Cloud,
        ] {
            let probes = adapter_for(kind).probes("http://h:1", "http://h:1/v1");
            assert!(
                probes.iter().any(|p| p.required),
                "{kind:?} has no required probe"
            );
            for p in &probes {
                assert!(
                    p.url.starts_with("http://h:1"),
                    "{kind:?} probe {} built a bad url: {}",
                    p.id,
                    p.url
                );
            }
        }
    }

    #[test]
    fn detects_engines_from_metrics_text() {
        assert_eq!(detect_kind_from_metrics("vllm:x 1"), Some(ServerKind::Vllm));
        assert_eq!(
            detect_kind_from_metrics("llamacpp:x 1"),
            Some(ServerKind::Local)
        );
        assert_eq!(detect_kind_from_metrics("nothing here"), None);
    }

    #[test]
    fn diag_deduplicates() {
        let mut snap = ParsedSnapshot::default();
        snap.diag(DiagCode::ServerIdle);
        snap.diag(DiagCode::ServerIdle);
        assert_eq!(snap.diagnostics.len(), 1);
        snap.diag(DiagCode::ContextUnknown);
        assert_eq!(snap.diagnostics.len(), 2);
    }
}
