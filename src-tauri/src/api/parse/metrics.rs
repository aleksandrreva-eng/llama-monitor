//! Metric extraction: turns a flat metric map into the context / speed / raw
//! metric values the widget displays.

use std::collections::{HashMap, HashSet};

use crate::api::parse::prometheus::{find_f64, find_u64, strip_namespace_and_labels};
use crate::api::types::{ContextMetric, RawMetric, SpeedMetric};

/// Extract the context metric from a metrics map.
///
/// Note: modern llama.cpp exposes **no** context metric under `--metrics`
/// (the real metric set is `prompt_*`, `tokens_predicted_*`, `n_decode_total`,
/// `n_tokens_max`, `requests_*`). Context is therefore normally taken from
/// `/slots`; this function only serves builds that do publish context counters.
pub fn extract_context(metrics: &HashMap<String, f64>) -> ContextMetric {
    // Try a few common naming conventions (with and without the llamacpp: prefix).
    let total = find_u64(
        metrics,
        &[
            "llamacpp:ctx_mem_total",
            "ctx_mem_total",
            "llamacpp:context_total",
            "context_total",
            "llamacpp:ctx_total",
            "ctx_total",
        ],
    );
    let used = find_u64(
        metrics,
        &[
            "llamacpp:ctx_mem_used",
            "ctx_mem_used",
            "llamacpp:context_used",
            "context_used",
            "llamacpp:ctx_used",
            "ctx_used",
        ],
    );

    match (total, used) {
        (Some(total), Some(used)) => {
            let remaining = total.saturating_sub(used);
            let percent = (used as f64 / total as f64).clamp(0.0, 1.0);
            ContextMetric {
                total: Some(total),
                used: Some(used),
                remaining: Some(remaining),
                percent: Some(percent),
                available: true,
            }
        }
        // We know total but not used.
        (Some(total), None) => ContextMetric {
            total: Some(total),
            used: None,
            remaining: None,
            percent: None,
            available: true,
        },
        _ => ContextMetric::default(),
    }
}

/// Extract prefill / generation token speeds from a metrics map.
///
/// Common llama.cpp field names vary between builds, so we probe several
/// candidates. Modern builds expose `--metrics` values under a `llamacpp:`
/// prefix (Prometheus namespace), e.g.:
///
/// ```text
/// llamacpp:prompt_tokens_seconds       6.79    <- prefill tok/s (gauge)
/// llamacpp:predicted_tokens_seconds    5.02    <- generation tok/s (gauge)
/// llamacpp:prompt_tokens_total         25      <- cumulative counters
/// llamacpp:prompt_seconds_total        3.68
/// llamacpp:tokens_predicted_total      200
/// llamacpp:tokens_predicted_seconds_total 39.8
/// ```
///
/// Older/alternative builds use unprefixed names such as
/// `prompt_tokens_per_second`. Both families are accepted.
pub fn extract_speeds(metrics: &HashMap<String, f64>) -> (SpeedMetric, SpeedMetric) {
    // Prefill (prompt) throughput.
    let prefill_rates = &[
        "llamacpp:prompt_tokens_seconds",
        "prompt_tokens_seconds",
        "prompt_tokens_per_second",
        "prefill_tokens_per_second",
    ];
    // Generation (predicted) throughput.
    let gen_rates = &[
        "llamacpp:predicted_tokens_seconds",
        "predicted_tokens_seconds",
        "generation_tokens_per_second",
        "tokens_per_second",
    ];

    let prefill = build_speed(
        metrics,
        prefill_rates,
        &["llamacpp:prompt_tokens_total", "prompt_tokens_total"],
        &["llamacpp:prompt_seconds_total", "prompt_seconds_total"],
    );
    let generation = build_speed(
        metrics,
        gen_rates,
        &["llamacpp:tokens_predicted_total", "tokens_predicted_total"],
        &[
            "llamacpp:tokens_predicted_seconds_total",
            "tokens_predicted_seconds_total",
        ],
    );

    // Prefill and generation are only reported separately when BOTH are known.
    let split = prefill.current.is_some() && generation.current.is_some();
    (
        SpeedMetric {
            split_available: split,
            ..prefill
        },
        SpeedMetric {
            split_available: split,
            ..generation
        },
    )
}

/// Build a SpeedMetric from a rate gauge, falling back to
/// `counter_total / seconds_total` when the gauge is missing.
fn build_speed(
    metrics: &HashMap<String, f64>,
    rate_keys: &[&str],
    count_keys: &[&str],
    seconds_keys: &[&str],
) -> SpeedMetric {
    // Prefer the ready-made gauge.
    let mut current = find_f64(metrics, rate_keys).filter(|v| *v > 0.0);

    // Fall back to deriving the rate from the cumulative counters.
    if current.is_none() {
        if let (Some(tokens), Some(seconds)) = (
            find_f64(metrics, count_keys),
            find_f64(metrics, seconds_keys),
        ) {
            if seconds > 0.0 {
                current = Some(tokens / seconds);
            }
        }
    }

    SpeedMetric {
        current,
        avg_30s: None,
        available: current.is_some(),
        split_available: false, // set by the caller once both are known
    }
}

/// Collect every llama.cpp `/metrics` counter/gauge for the expanded view.
///
/// Unlike the context and speed cards — which only surface a handful of derived
/// values — this returns **all** metric families the server exposes under
/// `--metrics`. The prompt/generation speed cards already consume
/// `prompt_seconds_total` and `predicted_seconds_total`, so those are dropped
/// here (the user sees them on the SpeedBlock cards) and the rest are rendered
/// as-is in the expanded metrics grid.
///
/// The Prometheus text format prefixes every metric with the `llamacpp:`
/// namespace and appends a label set, e.g. `llamacpp:n_slots_busy{slot="0"} 1`.
/// The namespace prefix and any label set are stripped, leaving a bare metric
/// name (`n_slots_busy`) plus its value — so the grid shows the same clean keys
/// whether the build uses the Prometheus or the JSON `/metrics` format.
///
/// When a metric family has several labelled series (e.g. per-slot counters),
/// only the first series' value is kept; the value is stored exactly as parsed
/// (no smoothing) since it is purely informational.
///
/// `prompt_seconds_total` and `predicted_seconds_total` are cumulative
/// wall-clock-second counters; the remaining metrics are cumulative token
/// counts or live gauges.
pub fn collect_other_metrics(map: &HashMap<String, f64>) -> Vec<RawMetric> {
    // Dedupe by the stripped name: Prometheus emits one line per labelled
    // series, so the same family can appear several times.
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for (raw_key, value) in map {
        // Skip the two speed totals already shown on the speed cards.
        if matches!(
            strip_namespace_and_labels(raw_key).as_str(),
            "prompt_seconds_total" | "predicted_seconds_total"
        ) {
            continue;
        }

        let key = strip_namespace_and_labels(raw_key);
        if key.is_empty() || !seen.insert(key.clone()) {
            continue;
        }
        out.push(RawMetric {
            name: key,
            value: *value,
        });
    }
    // `map` iteration order is unspecified; sort so the expanded grid is stable
    // between polls instead of reshuffling on every update.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A vLLM KV-cache occupancy reading (used vs. total tokens in the KV cache).
#[derive(Debug, Clone)]
pub struct VllmContext {
    pub used: Option<u64>,
    pub total: Option<u64>,
    pub percent: Option<f64>,
}

/// Parse vLLM `/metrics` (Prometheus text) into a context metric.
///
/// vLLM exposes `vllm:kv_cache_usage_sys` (used system KV tokens) and
/// `vllm:kv_cache_usage_max` (max KV tokens for the engine). The ratio of the
/// two is the context occupancy. We do NOT know the model's configured context
/// window from these counters, so `total` is the engine max, not a model limit.
pub fn extract_vllm_context(text: &str) -> VllmContext {
    let map = crate::api::parse::prometheus::parse_prometheus(text);
    let used = find_f64(&map, &["vllm:kv_cache_usage_sys", "vllm:kv_cache_used"]);
    let total = find_f64(&map, &["vllm:kv_cache_usage_max", "vllm:kv_cache_max"]);
    let percent = match (used, total) {
        (Some(u), Some(t)) if t > 0.0 => Some((u / t).clamp(0.0, 1.0)),
        _ => None,
    };
    VllmContext {
        used: used.map(|v| v as u64),
        total: total.map(|v| v as u64),
        percent,
    }
}

/// Parse vLLM `/metrics` into prefill / generation speeds.
///
/// - Prefill: `vllm:time_to_first_token_seconds` (seconds from request to first
///   output token).
/// - Generation: `vllm:inter_token_latency_seconds` (per-token latency).
///
/// We invert each latency (1 / seconds) into tok/s so the UI keeps its tok/s
/// contract. Guarded against zero latency.
pub fn extract_vllm_speeds(text: &str) -> (SpeedMetric, SpeedMetric) {
    let map = crate::api::parse::prometheus::parse_prometheus(text);
    let ttft = find_f64(&map, &["vllm:time_to_first_token_seconds", "vllm:ttft_s"]);
    let itl = find_f64(&map, &["vllm:inter_token_latency_seconds", "vllm:itl_s"]);

    let prefill = rate_speed(ttft);
    let generation = rate_speed(itl);

    let split = prefill.current.is_some() && generation.current.is_some();
    (
        SpeedMetric {
            split_available: split,
            ..prefill
        },
        SpeedMetric {
            split_available: split,
            ..generation
        },
    )
}

/// Invert a per-token latency (seconds) into tok/s, guarding against zero.
fn rate_speed(latency: Option<f64>) -> SpeedMetric {
    match latency {
        Some(v) if v > 0.0 => SpeedMetric {
            current: Some(1.0 / v),
            available: true,
            ..Default::default()
        },
        _ => SpeedMetric::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::parse::prometheus::{parse_prometheus, REAL_METRICS};

    #[test]
    fn extracts_speeds_from_real_metrics() {
        let map = parse_prometheus(REAL_METRICS);
        let (prefill, generation) = extract_speeds(&map);
        assert!(prefill.available, "prefill speed must be parsed");
        assert!(generation.available, "generation speed must be parsed");
        assert!((prefill.current.unwrap() - 6.79348).abs() < 1e-6);
        assert!((generation.current.unwrap() - 5.02513).abs() < 1e-6);
        // Both known -> the prefill/generation split is available.
        assert!(prefill.split_available);
        assert!(generation.split_available);
    }

    #[test]
    fn derives_speed_from_counters_when_gauge_is_absent() {
        // Some builds expose only cumulative counters. 25 tok / 3.68 s = 6.79 tok/s.
        let text = "\
llamacpp:prompt_tokens_total 25
llamacpp:prompt_seconds_total 3.68
llamacpp:tokens_predicted_total 200
llamacpp:tokens_predicted_seconds_total 39.8
";
        let map = parse_prometheus(text);
        let (prefill, generation) = extract_speeds(&map);
        assert!((prefill.current.unwrap() - (25.0 / 3.68)).abs() < 1e-9);
        assert!((generation.current.unwrap() - (200.0 / 39.8)).abs() < 1e-9);
    }

    #[test]
    fn zero_seconds_counter_does_not_divide_by_zero() {
        let text = "\
llamacpp:prompt_tokens_total 0
llamacpp:prompt_seconds_total 0
";
        let map = parse_prometheus(text);
        let (prefill, _) = extract_speeds(&map);
        assert!(!prefill.available);
        assert_eq!(prefill.current, None);
    }

    #[test]
    fn real_metrics_have_no_context_and_that_is_expected() {
        // Documents why context must come from /slots on these builds.
        let map = parse_prometheus(REAL_METRICS);
        let ctx = extract_context(&map);
        assert!(
            !ctx.available,
            "no context metric exists in the real metric set"
        );
    }

    #[test]
    fn context_full_known() {
        let mut m = HashMap::new();
        m.insert("ctx_total".to_string(), 32768.0);
        m.insert("ctx_used".to_string(), 8192.0);
        let c = extract_context(&m);
        assert!(c.available);
        assert_eq!(c.total, Some(32768));
        assert_eq!(c.used, Some(8192));
        assert_eq!(c.remaining, Some(24576));
        assert!((c.percent.unwrap() - 0.25).abs() < 1e-9);
    }

    /// Total known, used unknown: `percent` must stay `None`, never `0.0` — the
    /// UI renders "unknown" from that, and a fake zero would read as "empty".
    #[test]
    fn context_total_only_leaves_percent_unknown() {
        let mut m = HashMap::new();
        m.insert("ctx_total".to_string(), 4096.0);
        let c = extract_context(&m);
        assert!(c.available);
        assert_eq!(c.total, Some(4096));
        assert_eq!(c.used, None);
        assert_eq!(c.percent, None);
    }

    /// `used` beyond `total` must clamp rather than underflow.
    #[test]
    fn context_used_above_total_is_clamped() {
        let mut m = HashMap::new();
        m.insert("ctx_total".to_string(), 100.0);
        m.insert("ctx_used".to_string(), 250.0);
        let c = extract_context(&m);
        assert_eq!(c.remaining, Some(0));
        assert_eq!(c.percent, Some(1.0));
    }

    #[test]
    fn other_metrics_drop_speed_totals_and_are_stable() {
        let map = parse_prometheus(REAL_METRICS);
        let out = collect_other_metrics(&map);
        let names: Vec<&str> = out.iter().map(|m| m.name.as_str()).collect();
        assert!(!names.contains(&"prompt_seconds_total"));
        assert!(!names.contains(&"predicted_seconds_total"));
        assert!(names.contains(&"n_decode_total"));
        // Sorted, so the grid does not reshuffle between polls.
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn vllm_context_and_speeds() {
        let text = "\
vllm:kv_cache_usage_sys 512
vllm:kv_cache_usage_max 1024
vllm:time_to_first_token_seconds 0.25
vllm:inter_token_latency_seconds 0.05
";
        let ctx = extract_vllm_context(text);
        assert_eq!(ctx.used, Some(512));
        assert_eq!(ctx.total, Some(1024));
        assert!((ctx.percent.unwrap() - 0.5).abs() < 1e-9);

        let (prefill, generation) = extract_vllm_speeds(text);
        assert!((prefill.current.unwrap() - 4.0).abs() < 1e-9);
        assert!((generation.current.unwrap() - 20.0).abs() < 1e-9);
        assert!(prefill.split_available);
    }

    /// A zero latency must not become an infinite tok/s reading.
    #[test]
    fn vllm_zero_latency_is_not_infinite_speed() {
        let text = "vllm:time_to_first_token_seconds 0\n";
        let (prefill, _) = extract_vllm_speeds(text);
        assert!(!prefill.available);
        assert_eq!(prefill.current, None);
    }
}
