//! llama.cpp `/slots` parsing.
//!
//! `/slots` is the only endpoint that reports *live* per-slot context usage on
//! modern builds, so it carries the context bar even when `/metrics` is enabled.

use serde_json::Value;

use crate::api::types::ContextMetric;

/// Information extracted from the `/slots` endpoint.
#[derive(Debug, Default)]
pub struct SlotsInfo {
    /// Context usage accumulated across active slots (used vs. total).
    pub context: ContextMetric,
    /// Prefill speed reported by a slot's next-token timings (tok/s).
    pub prefill_speed: Option<f64>,
    /// Generation speed reported by a slot's next-token timings (tok/s).
    pub generation_speed: Option<f64>,
    /// Context size (`n_ctx`) advertised by any slot.
    pub context_size: Option<u64>,
    /// Whether at least one slot is actively processing.
    pub active: bool,
}

impl SlotsInfo {
    /// Returns the context metric if the slots yielded any usable numbers.
    pub fn context_opt(&self) -> Option<ContextMetric> {
        self.context.available.then(|| self.context.clone())
    }
}

/// Parse the llama.cpp `/slots` array into a [`SlotsInfo`].
///
/// The endpoint returns an array of slot objects. Idle slots carry only
/// `id` / `n_ctx` / `is_processing`; active slots additionally expose
/// `n_prompt_tokens`, `n_prompt_tokens_processed`, `n_decoded`, `n_ctx`,
/// a `next_token` object with `n_decoded`, and sometimes `timings` with
/// `prompt_per_second` / `predicted_per_second`.
pub fn parse_slots(value: &Value) -> SlotsInfo {
    let mut info = SlotsInfo::default();

    let slots = match value.as_array() {
        Some(a) => a,
        None => return info,
    };

    // Total context = n_ctx of the first slot that reports it (slots share the ctx pool).
    for slot in slots {
        if let Some(n_ctx) = slot.get("n_ctx").and_then(|v| v.as_u64()) {
            if n_ctx > 0 {
                info.context_size = Some(n_ctx);
                break;
            }
        }
    }

    // Used context = the largest (prompt + decoded) footprint any slot reports.
    //
    // Important: a slot keeps its `n_prompt_tokens` / `n_decoded` counters *after*
    // generation finishes — `is_processing` flips back to false while the token
    // counts persist. We therefore read the counters regardless of `is_processing`,
    // so the widget keeps showing the context occupied by the last request instead
    // of snapping back to 0 the moment generation ends. Slots that only ever
    // carried `id`/`n_ctx` (never used) contribute nothing.
    let mut max_used: u64 = 0;
    for slot in slots {
        let processing = slot
            .get("is_processing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if processing {
            info.active = true;
        }

        // Prefer the total prompt size (`n_prompt_tokens`) over the processed
        // counter: right after a request is queued, `n_prompt_tokens_processed`
        // is still 0 while `n_prompt_tokens` already reflects the real prompt.
        let prompt = slot
            .get("n_prompt_tokens")
            .and_then(|v| v.as_u64())
            .or_else(|| {
                slot.get("n_prompt_tokens_processed")
                    .and_then(|v| v.as_u64())
            });
        let decoded = slot.get("n_decoded").and_then(|v| v.as_u64()).or_else(|| {
            slot.get("next_token")
                .and_then(|t| t.get("n_decoded"))
                .and_then(|v| v.as_u64())
        });

        if let Some(p) = prompt {
            let d = decoded.unwrap_or(0);
            max_used = max_used.max(p.saturating_add(d));
        }

        // Per-slot timings (present in some recent builds).
        if let Some(timings) = slot.get("timings") {
            if info.prefill_speed.is_none() {
                info.prefill_speed = timings.get("prompt_per_second").and_then(|v| v.as_f64());
            }
            if info.generation_speed.is_none() {
                info.generation_speed =
                    timings.get("predicted_per_second").and_then(|v| v.as_f64());
            }
        }
    }

    if let Some(total) = info.context_size {
        if total > 0 {
            let used = max_used.min(total);
            let remaining = total.saturating_sub(used);
            let percent = (used as f64 / total as f64).clamp(0.0, 1.0);
            info.context = ContextMetric {
                total: Some(total),
                used: Some(used),
                remaining: Some(remaining),
                percent: Some(percent),
                available: true,
            };
        }
    }

    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn idle_slots_report_context_size_without_usage() {
        let v = json!([{ "id": 0, "n_ctx": 32768, "is_processing": false }]);
        let info = parse_slots(&v);
        assert_eq!(info.context_size, Some(32768));
        assert!(!info.active);
        assert_eq!(info.context.used, Some(0));
        assert_eq!(info.context.total, Some(32768));
    }

    /// Counters survive after generation ends; the widget must keep showing the
    /// footprint of the last request instead of snapping back to zero.
    #[test]
    fn finished_request_keeps_its_context_footprint() {
        let v = json!([{
            "id": 0,
            "n_ctx": 4096,
            "is_processing": false,
            "n_prompt_tokens": 1000,
            "n_decoded": 200
        }]);
        let info = parse_slots(&v);
        assert_eq!(info.context.used, Some(1200));
        assert_eq!(info.context.remaining, Some(2896));
        assert!(!info.active);
    }

    #[test]
    fn reads_next_token_and_timings() {
        let v = json!([{
            "id": 0,
            "n_ctx": 8192,
            "is_processing": true,
            "n_prompt_tokens": 512,
            "next_token": { "n_decoded": 64 },
            "timings": { "prompt_per_second": 120.5, "predicted_per_second": 33.25 }
        }]);
        let info = parse_slots(&v);
        assert!(info.active);
        assert_eq!(info.context.used, Some(576));
        assert_eq!(info.prefill_speed, Some(120.5));
        assert_eq!(info.generation_speed, Some(33.25));
    }

    /// A non-array body (an error object, an HTML page) must degrade quietly.
    #[test]
    fn non_array_body_yields_empty_info() {
        let info = parse_slots(&json!({ "error": "disabled" }));
        assert!(!info.context.available);
        assert_eq!(info.context_size, None);
        assert!(info.context_opt().is_none());
    }

    /// Usage is clamped to the slot's own context size.
    #[test]
    fn usage_above_context_is_clamped() {
        let v = json!([{
            "id": 0,
            "n_ctx": 100,
            "n_prompt_tokens": 500,
            "n_decoded": 500
        }]);
        let info = parse_slots(&v);
        assert_eq!(info.context.used, Some(100));
        assert_eq!(info.context.remaining, Some(0));
        assert_eq!(info.context.percent, Some(1.0));
    }
}
