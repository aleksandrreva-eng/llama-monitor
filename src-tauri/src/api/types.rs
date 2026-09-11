//! Shared domain types describing the monitoring state.
//!
//! Design rules:
//! - `null` means "unknown / unavailable", never a real zero.
//! - `available: false` tells the UI to render a placeholder ("N/A"), not a crash.
//! - `remaining` is always clamped to >= 0.

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

impl ConnectionStatus {
    pub fn label(self) -> &'static str {
        match self {
            ConnectionStatus::Connected => "Подключено",
            ConnectionStatus::Connecting => "Подключение",
            ConnectionStatus::Disconnected => "Нет соединения",
            ConnectionStatus::Error => "Ошибка данных",
            ConnectionStatus::Stale => "Данные устарели",
            ConnectionStatus::ServerUnavailable => "Сервер недоступен",
            ConnectionStatus::MetricsUnavailable => "Метрики недоступны",
        }
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
    /// Human-readable diagnostics shown in the expanded view.
    #[allow(dead_code)]
    pub diagnostics: Vec<String>,
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
            "diagnostics",
        ];
        let actual: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        for key in expected {
            assert!(
                obj.contains_key(key),
                "MonitoringState is missing wire key `{key}`; got {actual:?}"
            );
        }

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
            let inner = obj[wire]
                .as_object()
                .unwrap_or_else(|| panic!("`{wire}` (Rust field `{rust_field}`) must be an object"));
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
}
