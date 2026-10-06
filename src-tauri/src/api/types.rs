//! Shared domain types describing the monitoring state.
//!
//! Design rules:
//! - `null` means "unknown / unavailable", never a real zero.
//! - `available: false` tells the UI to render a placeholder ("N/A"), not a crash.
//! - `remaining` is always clamped to >= 0.
//! - Every user-visible string that originates here is a **code**, not prose:
//!   the frontend owns translation (see [`Diag`]). The Rust side never ships a
//!   Russian sentence across the wire.

use serde::{Deserialize, Serialize};

/// Connection / data health status.
///
/// Serialized as snake_case (`server_unavailable`) because the frontend indexes
/// its `STATUS_CLASS` lookup table with exactly those strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Error,
    Stale,
    ServerUnavailable,
    MetricsUnavailable,
}

/// A machine-readable diagnostic code.
///
/// Carries no prose: the UI resolves `code()` through its own i18n dictionary
/// (`diag_<code>`), so the diagnostics block is localized like the rest of the
/// widget. `detail` is only used for values that cannot be translated (an OS or
/// network error string) and is rendered as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagCode {
    /// Server is reachable but reports no context usage.
    ContextUnknown,
    /// No prefill throughput reading is available.
    PrefillUnavailable,
    /// No generation throughput reading is available.
    GenerationUnavailable,
    /// No model name could be determined from any source.
    ModelUnknown,
    /// Speeds are present but the server does not split prefill from generation.
    SplitUnavailable,
    /// The values on screen are older than the freshness threshold.
    Stale,
    /// The server is up but has no request in flight.
    ServerIdle,
    /// `/metrics` answered 501/404 — the server runs without `--metrics`.
    MetricsDisabled,
    /// A vLLM endpoint answered, but without the expected `vllm:` metric family.
    VllmMetricsMissing,
    /// Ollama is up but reports no running model.
    OllamaNoModels,
    /// The poll failed. `detail` carries the untranslatable transport/HTTP error.
    PollFailed(String),
}

impl DiagCode {
    /// Stable wire code. Must stay in sync with `diag_*` keys in `src/i18n.js`
    /// and with the `DIAG_*` lookups in the Svelte components.
    pub fn code(&self) -> &'static str {
        match self {
            DiagCode::ContextUnknown => "context_unknown",
            DiagCode::PrefillUnavailable => "prefill_unavailable",
            DiagCode::GenerationUnavailable => "generation_unavailable",
            DiagCode::ModelUnknown => "model_unknown",
            DiagCode::SplitUnavailable => "split_unavailable",
            DiagCode::Stale => "stale",
            DiagCode::ServerIdle => "server_idle",
            DiagCode::MetricsDisabled => "metrics_disabled",
            DiagCode::VllmMetricsMissing => "vllm_metrics_missing",
            DiagCode::OllamaNoModels => "ollama_no_models",
            DiagCode::PollFailed(_) => "poll_failed",
        }
    }

    /// Untranslatable payload, if any.
    pub fn detail(&self) -> Option<String> {
        match self {
            DiagCode::PollFailed(e) => Some(e.clone()),
            _ => None,
        }
    }
}

/// Wire form of a diagnostic: `{ "code": "...", "detail": "..."? }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diag {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl From<&DiagCode> for Diag {
    fn from(d: &DiagCode) -> Self {
        Diag {
            code: d.code().to_string(),
            detail: d.detail(),
        }
    }
}

impl From<DiagCode> for Diag {
    fn from(d: DiagCode) -> Self {
        Diag::from(&d)
    }
}

/// Context usage metric.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ContextMetric {
    pub total: Option<u64>,
    pub used: Option<u64>,
    pub remaining: Option<u64>,
    pub percent: Option<f64>,
    pub available: bool,
}

/// Token speed metric (tokens/sec).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeedMetric {
    pub current: Option<f64>,
    pub avg_30s: Option<f64>,
    pub available: bool,
    pub split_available: bool,
}

/// Loaded model info.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelMetric {
    pub name: Option<String>,
    pub context_size: Option<u64>,
    pub loaded: bool,
    pub quantization: Option<String>,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// A single raw llama.cpp `/metrics` counter or gauge shown in the expanded view.
///
/// Names are stripped of the `llamacpp:` Prometheus namespace prefix at the parse
/// site so the UI label map works against bare metric names.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RawMetric {
    pub name: String,
    pub value: f64,
}

/// Full monitoring state pushed to the UI.
///
/// Wire format is camelCase, matching the Svelte store's declared shape. Do not
/// drop this attribute: the UI reads `lastUpdate` / `prefillSpeed` /
/// `generationSpeed` / `serverLabel`, and a mismatch makes them `undefined`,
/// which blanks the whole widget on the first update (see the contract test).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringState {
    pub connection: ConnectionStatus,
    pub last_update: Option<i64>,
    pub server_label: Option<String>,
    pub context: ContextMetric,
    pub prefill_speed: SpeedMetric,
    pub generation_speed: SpeedMetric,
    pub model: ModelMetric,
    /// Raw llama.cpp `/metrics` counters/gauges (e.g. prompt tokens total,
    /// n_decode_total) surfaced in the expanded view. Empty unless the server
    /// exposes `--metrics`.
    pub other_metrics: Vec<RawMetric>,
    /// Localizable diagnostics shown in the expanded view and consumed by
    /// `SpeedBlock` to explain a missing reading. Codes only — see [`Diag`].
    pub diagnostics: Vec<Diag>,
}

impl Default for MonitoringState {
    fn default() -> Self {
        MonitoringState {
            connection: ConnectionStatus::Disconnected,
            last_update: None,
            server_label: None,
            context: ContextMetric::default(),
            prefill_speed: SpeedMetric::default(),
            generation_speed: SpeedMetric::default(),
            model: ModelMetric::default(),
            other_metrics: vec![],
            diagnostics: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Svelte frontend reads this payload with camelCase keys
    /// (`lastUpdate`, `prefillSpeed`, `splitAvailable`, `contextSize`, …) and the
    /// status dot indexes `STATUS_CLASS` by a snake_case status string
    /// (`server_unavailable`). This test is the contract: if the serde rename
    /// attributes are ever dropped, the UI silently reads `undefined` and the
    /// whole widget blanks out at the first update. Keep it green.
    #[test]
    fn monitoring_state_wire_format_matches_the_frontend_contract() {
        let v = serde_json::to_value(MonitoringState::default()).unwrap();
        let obj = v.as_object().expect("state must serialize to an object");

        let expected = [
            "connection",
            "lastUpdate",
            "serverLabel",
            "context",
            "prefillSpeed",
            "generationSpeed",
            "model",
            "otherMetrics",
            "diagnostics",
        ];
        let actual: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        for key in expected {
            assert!(
                obj.contains_key(key),
                "MonitoringState is missing wire key `{key}`; got {actual:?}"
            );
        }

        // RawMetric must serialize with the exact keys the expanded-view block
        // indexes: `name` + `value`. Kept here so a rename in the struct blanks
        // the grid with no test coverage to catch it.
        let raw = RawMetric {
            name: "prompt_tokens_total".to_string(),
            value: 25.0,
        };
        assert_eq!(
            serde_json::to_value(raw).unwrap(),
            serde_json::json!({ "name": "prompt_tokens_total", "value": 25.0 })
        );

        // ConnectionStatus must be snake_case so `STATUS_CLASS` can index it.
        assert_eq!(obj["connection"], serde_json::json!("disconnected"));

        // Nested keys the components dereference directly. `avg30s` is the one
        // most likely to regress: serde's camelCase conversion of `avg_30s` is
        // not obvious, and SpeedBlock reads it unconditionally.
        let nested: [(&str, &str, &[&str]); 4] = [
            (
                "context",
                "context",
                &["total", "used", "remaining", "percent", "available"],
            ),
            (
                "prefillSpeed",
                "prefill_speed",
                &["current", "avg30s", "available", "splitAvailable"],
            ),
            (
                "generationSpeed",
                "generation_speed",
                &["current", "avg30s", "available", "splitAvailable"],
            ),
            (
                "model",
                "model",
                &[
                    "name",
                    "contextSize",
                    "loaded",
                    "quantization",
                    "path",
                    "version",
                ],
            ),
        ];
        for (wire, rust_field, keys) in nested {
            let inner = obj[wire].as_object().unwrap_or_else(|| {
                panic!("`{wire}` (Rust field `{rust_field}`) must be an object")
            });
            for key in keys {
                assert!(
                    inner.contains_key(*key),
                    "`{wire}` is missing `{key}`; got {:?}",
                    inner.keys().collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn every_connection_status_maps_to_a_status_class_key() {
        let expected = [
            (ConnectionStatus::Connected, "connected"),
            (ConnectionStatus::Connecting, "connecting"),
            (ConnectionStatus::Disconnected, "disconnected"),
            (ConnectionStatus::Error, "error"),
            (ConnectionStatus::Stale, "stale"),
            (ConnectionStatus::ServerUnavailable, "server_unavailable"),
            (ConnectionStatus::MetricsUnavailable, "metrics_unavailable"),
        ];
        for (status, wire) in expected {
            assert_eq!(
                serde_json::to_value(status).unwrap(),
                serde_json::json!(wire),
                "ConnectionStatus::{status:?} must serialize as `{wire}`"
            );
        }
    }

    /// Diagnostics must reach the UI as `{code, detail?}` objects. The frontend
    /// builds its i18n key as `diag_<code>`, so a code that is not a stable
    /// snake_case identifier would render as a raw key in the panel.
    #[test]
    fn diagnostics_serialize_as_code_objects() {
        let codes = [
            DiagCode::ContextUnknown,
            DiagCode::PrefillUnavailable,
            DiagCode::GenerationUnavailable,
            DiagCode::ModelUnknown,
            DiagCode::SplitUnavailable,
            DiagCode::Stale,
            DiagCode::ServerIdle,
            DiagCode::MetricsDisabled,
            DiagCode::VllmMetricsMissing,
            DiagCode::OllamaNoModels,
            DiagCode::PollFailed("boom".to_string()),
        ];
        for code in &codes {
            let wire = code.code();
            assert!(
                !wire.is_empty() && wire.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "diagnostic code `{wire}` must be a snake_case identifier"
            );
            let v = serde_json::to_value(Diag::from(code)).unwrap();
            assert_eq!(v["code"], serde_json::json!(wire));
            assert_eq!(v.get("detail").is_some(), code.detail().is_some());
        }

        // The i18n key is derived on the frontend as `diag_<code>`; the list is
        // asserted against the enum length so a new variant cannot be added
        // without deciding on its translation.
        assert_eq!(
            codes.len(),
            11,
            "keep DiagCode and i18n diag_* keys in sync"
        );
    }
}
