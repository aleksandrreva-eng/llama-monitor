//! Prometheus text-format and JSON metric parsing.
//!
//! Different builds answer `/metrics` differently: modern llama.cpp serves the
//! Prometheus text format (with a `llamacpp:` namespace), some alternative
//! builds serve a flat JSON object. Both are normalized into the same
//! `HashMap<String, f64>` so the metric extractors do not care which arrived.

use serde_json::Value;
use std::collections::HashMap;

/// Parse a Prometheus metrics text blob into a flat key->value map.
///
/// Namespace prefixes are **kept** (`llamacpp:prompt_tokens_total`), because the
/// extractors probe both prefixed and unprefixed families. Label sets are
/// dropped: `metric{slot="0"} 1` becomes `metric -> 1`.
pub fn parse_prometheus(text: &str) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // token metrics often look like: llama_load_time_ms 123.4 or
        // llama_context_n_tokens_total{...} 45
        let key: String = line
            .split(|c| c == '{' || c == ' ')
            .next()
            .unwrap_or("")
            .to_string();
        if let Some((_, rest)) = line.split_once(' ') {
            if let Ok(v) = rest
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("")
                .parse::<f64>()
            {
                map.insert(key, v);
            }
        }
    }
    map
}

/// Parse a llama.cpp-style JSON metrics object into a flat key->f64 map.
pub fn parse_json_metrics(value: &Value) -> HashMap<String, f64> {
    let mut map = HashMap::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(n) = v.as_f64() {
                map.insert(k.clone(), n);
            }
        }
    }
    map
}

/// Parse a `/metrics` body without trusting the `Content-Type` header.
///
/// Builds disagree about the header (some send `text/plain`, some
/// `application/json`, some nothing useful), so the body itself decides: a JSON
/// object starts with `{`. This replaces the old header sniffing, which silently
/// skipped the JSON branch whenever a build mislabelled its content type.
pub fn parse_metrics_body(text: &str) -> HashMap<String, f64> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        match serde_json::from_str::<Value>(trimmed) {
            Ok(v) => parse_json_metrics(&v),
            Err(e) => {
                log::debug!("metrics body looked like JSON but failed to parse: {e}");
                HashMap::new()
            }
        }
    } else {
        parse_prometheus(trimmed)
    }
}

/// First present key from `keys`, as `u64`.
pub(crate) fn find_u64(map: &HashMap<String, f64>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| map.get(*k).map(|v| *v as u64))
}

/// First present key from `keys`, as `f64`.
pub(crate) fn find_f64(map: &HashMap<String, f64>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| map.get(*k).copied())
}

/// Strip the `llamacpp:` Prometheus namespace prefix and any `{...}` label set
/// from a raw metric key, returning the bare metric name.
///
/// Examples:
/// - `llamacpp:n_slots_busy{slot="0"}` → `n_slots_busy`
/// - `n_slots_busy` → `n_slots_busy`
/// - `llamacpp:requests_processing` → `requests_processing`
pub fn strip_namespace_and_labels(raw_key: &str) -> String {
    let key = raw_key.trim();
    // Drop the `llamacpp:` namespace prefix (Prometheus namespace).
    let key = key.strip_prefix("llamacpp:").unwrap_or(key).trim();
    // Drop any trailing label set, e.g. `{slot="0",...}`.
    key.split('{').next().unwrap_or("").trim().to_string()
}

/// Verbatim `GET /metrics` output captured from a real llama.cpp build
/// (10380) started with `--metrics`, after one generation request.
/// Note the `llamacpp:` namespace prefix and that there is NO ctx metric.
///
/// Lives at module level (not inside `tests`) so the metric-extraction tests in
/// `parse::metrics` can assert against the same real payload.
#[cfg(test)]
pub(crate) const REAL_METRICS: &str = "\
# HELP llamacpp:prompt_tokens_total Number of prompt tokens processed.
# TYPE llamacpp:prompt_tokens_total counter
llamacpp:prompt_tokens_total 25
# HELP llamacpp:prompt_seconds_total Prompt process time
# TYPE llamacpp:prompt_seconds_total counter
llamacpp:prompt_seconds_total 3.68
# HELP llamacpp:tokens_predicted_total Number of generation tokens processed.
# TYPE llamacpp:tokens_predicted_total counter
llamacpp:tokens_predicted_total 200
# HELP llamacpp:tokens_predicted_seconds_total Predict process time
# TYPE llamacpp:tokens_predicted_seconds_total counter
llamacpp:tokens_predicted_seconds_total 39.8
# HELP llamacpp:n_decode_total Total number of llama_decode() calls
# TYPE llamacpp:n_decode_total counter
llamacpp:n_decode_total 202
# HELP llamacpp:n_tokens_max Largest observed n_tokens.
# TYPE llamacpp:n_tokens_max counter
llamacpp:n_tokens_max 224
# HELP llamacpp:prompt_tokens_seconds Average prompt throughput in tokens/s.
# TYPE llamacpp:prompt_tokens_seconds gauge
llamacpp:prompt_tokens_seconds 6.79348
# HELP llamacpp:predicted_tokens_seconds Average generation throughput in tokens/s.
# TYPE llamacpp:predicted_tokens_seconds gauge
llamacpp:predicted_tokens_seconds 5.02513
# HELP llamacpp:requests_processing Number of requests processing.
# TYPE llamacpp:requests_processing gauge
llamacpp:requests_processing 0
# HELP llamacpp:n_busy_slots_per_decode Average number of busy slots per llama_decode() call
# TYPE llamacpp:n_busy_slots_per_decode gauge
llamacpp:n_busy_slots_per_decode 1
";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_prometheus_text() {
        let text = "\
# HELP llama_context_n_tokens_total Tokens
llama_context_n_tokens_total 4096
llama_context_n_ctx_total 32768
prompt_tokens_per_second 412.5
generation_tokens_per_second 38.25
";
        let map = parse_prometheus(text);
        assert_eq!(map.get("llama_context_n_tokens_total"), Some(&4096.0));
        assert_eq!(map.get("prompt_tokens_per_second"), Some(&412.5));
    }

    #[test]
    fn parses_real_llamacpp_metrics_names() {
        let map = parse_prometheus(REAL_METRICS);
        // The parser must keep the `llamacpp:` prefix intact.
        assert_eq!(map.get("llamacpp:prompt_tokens_seconds"), Some(&6.79348));
        assert_eq!(map.get("llamacpp:predicted_tokens_seconds"), Some(&5.02513));
        assert_eq!(map.get("llamacpp:prompt_tokens_total"), Some(&25.0));
        assert_eq!(
            map.get("llamacpp:tokens_predicted_seconds_total"),
            Some(&39.8)
        );
    }

    #[test]
    fn parses_prometheus_with_labels() {
        // Prometheus lines with label sets must still be split correctly.
        let text = "llama_context_n_tokens_total{slot=\"0\"} 1234\n";
        let map = parse_prometheus(text);
        assert_eq!(map.get("llama_context_n_tokens_total"), Some(&1234.0));
    }

    #[test]
    fn parses_json_metrics_flat() {
        let v = json!({ "ctx_total": 32768.0, "ctx_used": 8000.0 });
        let map = parse_json_metrics(&v);
        assert_eq!(map.get("ctx_total"), Some(&32768.0));
        assert_eq!(map.get("ctx_used"), Some(&8000.0));
    }

    /// The body decides the format, not the header: a build that labels its
    /// Prometheus payload as `application/json` used to be silently dropped.
    #[test]
    fn metrics_body_sniffs_format_from_content() {
        let json_map = parse_metrics_body(r#"{ "ctx_total": 4096 }"#);
        assert_eq!(json_map.get("ctx_total"), Some(&4096.0));

        let prom_map = parse_metrics_body(REAL_METRICS);
        assert_eq!(prom_map.get("llamacpp:prompt_tokens_total"), Some(&25.0));

        // Malformed JSON must degrade to an empty map, never panic.
        assert!(parse_metrics_body("{ not json").is_empty());
    }

    #[test]
    fn strips_namespace_and_labels() {
        assert_eq!(
            strip_namespace_and_labels("llamacpp:n_slots_busy{slot=\"0\"}"),
            "n_slots_busy"
        );
        assert_eq!(strip_namespace_and_labels("n_slots_busy"), "n_slots_busy");
        assert_eq!(
            strip_namespace_and_labels("llamacpp:requests_processing"),
            "requests_processing"
        );
    }
}
