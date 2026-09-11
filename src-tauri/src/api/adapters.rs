//! Data adapters: parse raw JSON from different llama.cpp endpoints.
//!
//! Different llama.cpp builds expose different fields, so each adapter
//! tolerates missing fields and reports what it could extract.

use serde::Deserialize;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::types::*;
use crate::config::ServerKind;

/// Raw metrics response (Prometheus text format is handled separately).
#[derive(Debug, Deserialize)]
pub struct MetricsResponse {
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, Value>,
}

/// Raw models list.
#[derive(Debug, Deserialize, Default)]
pub struct ModelsResponse {
    pub models: Option<Vec<ModelEntry>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ModelEntry {
    pub id: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub path: Option<String>,
    pub content_type: Option<String>,
    pub n_ctx: Option<u64>,
    /// OpenAI-compatible `/v1/models` reports the context window as
    /// `context_length` (not `n_ctx`); cloud providers (OpenAI/OpenRouter/Groq)
    /// only populate this field. `extract_model_entry_metric` falls back to it.
    pub context_length: Option<u64>,
    pub loaded: Option<bool>,
}

/// Raw server properties / status.
#[derive(Debug, Deserialize, Default)]
pub struct PropsResponse {
    pub llama: Option<LlamaProps>,
    pub server: Option<ServerProps>,
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct LlamaProps {
    pub model: Option<String>,
    pub context: Option<u64>,
    pub quantization: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServerProps {
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub model: Option<String>,
    pub version: Option<String>,
}

/// Parse a Prometheus metrics text blob into a flat key->value map.
pub fn parse_prometheus(text: &str) -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
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
            if let Ok(v) = rest.trim().split_whitespace().next().unwrap_or("").parse::<f64>() {
                map.insert(key, v);
            }
        }
    }
    map
}

/// Parse a llama.cpp-style JSON metrics object into a flat key->f64 map.
pub fn parse_json_metrics(value: &Value) -> std::collections::HashMap<String, f64> {
    let mut map = std::collections::HashMap::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            if let Some(n) = v.as_f64() {
                map.insert(k.clone(), n);
            }
        }
    }
    map
}

/// Extract context metric from a metrics map.
///
/// Note: modern llama.cpp exposes **no** context metric under `--metrics`
/// (the real metric set is `prompt_*`, `tokens_predicted_*`, `n_decode_total`,
/// `n_tokens_max`, `requests_*`). Context is therefore normally taken from
/// `/slots`; this function only serves builds that do publish context counters.
pub fn extract_context(metrics: &std::collections::HashMap<String, f64>) -> ContextMetric {
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

    if let (Some(total), Some(used)) = (total, used) {
        let remaining = total.saturating_sub(used);
        let percent = (used as f64 / total as f64).clamp(0.0, 1.0);
        ContextMetric {
            total: Some(total),
            used: Some(used),
            remaining: Some(remaining),
            percent: Some(percent),
            available: true,
        }
    } else if let Some(total) = total {
        // We know total but not used.
        ContextMetric {
            total: Some(total),
            used: None,
            remaining: None,
            percent: None,
            available: true,
        }
    } else {
        ContextMetric::default()
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
pub fn extract_speeds(
    metrics: &std::collections::HashMap<String, f64>,
) -> (SpeedMetric, SpeedMetric) {
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
    (SpeedMetric { split_available: split, ..prefill },
     SpeedMetric { split_available: split, ..generation })
}

/// Build a SpeedMetric from a rate gauge, falling back to
/// `counter_total / seconds_total` when the gauge is missing.
fn build_speed(
    metrics: &std::collections::HashMap<String, f64>,
    rate_keys: &[&str],
    count_keys: &[&str],
    seconds_keys: &[&str],
) -> SpeedMetric {
    // Prefer the ready-made gauge.
    let mut current = find_f64(metrics, rate_keys).filter(|v| *v > 0.0);

    // Fall back to deriving the rate from the cumulative counters.
    if current.is_none() {
        if let (Some(tokens), Some(seconds)) =
            (find_f64(metrics, count_keys), find_f64(metrics, seconds_keys))
        {
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

fn find_u64(map: &std::collections::HashMap<String, f64>, keys: &[&str]) -> Option<u64> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            return Some(*v as u64);
        }
    }
    None
}

fn find_f64(map: &std::collections::HashMap<String, f64>, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(v) = map.get(*k) {
            return Some(*v);
        }
    }
    None
}

/// Extract model info from a models response.
pub fn extract_model(models: &ModelsResponse) -> ModelMetric {
    // Prefer the first loaded model, else the first available.
    let entry = models
        .models
        .as_deref()
        .and_then(|v| v.first())
        .or_else(|| models.models.as_deref().and_then(|v| v.iter().find(|m| m.loaded == Some(true))));

    entry
        .map(|m| ModelMetric {
            name: m.id.clone().or(m.name.clone()).or(m.model.clone()),
            context_size: m.n_ctx,
            loaded: m.loaded.unwrap_or(false),
            quantization: m.content_type.clone(),
            path: m.path.clone(),
            version: None,
        })
        .unwrap_or_default()
}

/// Extract model info from server props.
///
/// Modern llama.cpp builds (`/props`) return a **flat** object:
/// `model_alias`, `model_ftype`, `model_path`, `default_generation_settings.n_ctx`,
/// `build_info`, `total_slots`. Older or alternative builds nest the same data under
/// a `llama` object (`llama.model`, `llama.context`, `llama.quantization`). We accept
/// both, preferring the flat form.
pub fn extract_model_from_props(props: &PropsResponse) -> ModelMetric {
    let f = &props.fields;

    let flat_str = |key: &str| f.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());
    let flat_u64 = |key: &str| f.get(key).and_then(|v| v.as_u64());

    // Model name: `model_alias` is the canonical field; fall back to
    // `model_path` (its basename) and then the legacy `llama.model`.
    let name = flat_str("model_alias")
        .filter(|s| !s.is_empty())
        .or_else(|| flat_str("model_path").filter(|s| !s.is_empty()))
        .or_else(|| props.llama.as_ref().and_then(|l| l.model.clone()))
        .or_else(|| props.server.as_ref().and_then(|s| s.model.clone()));

    // Context size: modern builds expose it under
    // `default_generation_settings.n_ctx`. Older builds expose `llama.context`
    // or a top-level `n_ctx`.
    let context_size = f
        .get("default_generation_settings")
        .and_then(|d| d.get("n_ctx"))
        .and_then(|v| v.as_u64())
        .filter(|&n| n > 0)
        .or_else(|| flat_u64("n_ctx").filter(|&n| n > 0))
        .or_else(|| props.llama.as_ref().and_then(|l| l.context));

    // Quantization: `model_ftype` (e.g. "Q4_0") in modern builds.
    let quantization = flat_str("model_ftype")
        .filter(|s| !s.is_empty())
        .or_else(|| props.llama.as_ref().and_then(|l| l.quantization.clone()));

    // Server version lives in `build_info` (string) on modern builds.
    let version = f
        .get("build_info")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            f.get("build_info")
                .and_then(|v| v.get("build_number"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| props.server.as_ref().and_then(|s| s.version.clone()));

    ModelMetric {
        name,
        context_size,
        loaded: true,
        quantization,
        path: flat_str("model_path"),
        version,
    }
}

/// Merge model info from a higher-priority source into an existing metric.
///
/// `incoming` wins for any field it actually provides; fields it leaves as
/// `None` keep the existing value. `loaded` is OR-ed, so a model reported
/// loaded by either source counts as loaded.
pub fn merge_model(existing: &mut ModelMetric, incoming: ModelMetric) {
    if incoming.name.is_some() {
        existing.name = incoming.name;
    }
    if incoming.context_size.is_some() {
        existing.context_size = incoming.context_size;
    }
    if incoming.quantization.is_some() {
        existing.quantization = incoming.quantization;
    }
    if incoming.path.is_some() {
        existing.path = incoming.path;
    }
    if incoming.version.is_some() {
        existing.version = incoming.version;
    }
    existing.loaded = existing.loaded || incoming.loaded;
}

/// Extract server label from props.
/// Modern builds have no `server` object. We synthesize a readable label from
/// `build_info` / `model_alias` when available; otherwise the caller falls back
/// to `host:port`.
pub fn extract_server_label(props: &PropsResponse) -> Option<String> {
    props
        .server
        .as_ref()
        .and_then(|s| {
            s.hostname.clone().map(|h| match s.port {
                Some(p) => format!("{}:{}", h, p),
                None => h,
            })
        })
        .or_else(|| props.server.as_ref().and_then(|s| s.model.clone()))
}

/// Current time as a Unix timestamp in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A parsed snapshot combining all adapters.
#[derive(Debug, Default)]
pub struct ParsedSnapshot {
    pub context: ContextMetric,
    pub prefill_speed: SpeedMetric,
    pub generation_speed: SpeedMetric,
    pub model: ModelMetric,
    pub server_label: Option<String>,
    pub timestamp_ms: i64,
    pub diagnostics: Vec<String>,
}

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
        if self.context.available {
            Some(self.context.clone())
        } else {
            None
        }
    }
}

/// Detect the upstream engine from a raw response so [`fetch_snapshot`] can pick
/// the right adapter.
///
/// Detection is heuristic and based on the *first* signal we see:
/// - A Prometheus blob containing `vllm:` metrics → [`ServerKind::Vllm`].
/// - A Prometheus blob containing `llamacpp:` metrics → [`ServerKind::Local`].
/// - An Ollama `/api/ps` array (each entry has a `name` + `context_length`) → [`ServerKind::Ollama`].
/// - Otherwise → [`ServerKind::Cloud`] (OpenAI-compatible `/v1/models`).
///
/// `context_length` is the OpenAI model field for the model's context window;
/// it is populated for cloud responses and for Ollama entries that carry it.
pub fn detect_kind_from_metrics(text: &str) -> Option<ServerKind> {
    if text.contains("vllm:") {
        Some(ServerKind::Vllm)
    } else if text.contains("llamacpp:") {
        Some(ServerKind::Local)
    } else {
        None
    }
}

/// Detect Ollama from the `/api/ps` payload: an array whose entries are running
/// models with a `name` and an `info` object carrying `context_length`.
pub fn detect_kind_from_ps(value: &Value) -> Option<ServerKind> {
    if let Some(arr) = value.as_array() {
        if let Some(entry) = arr.first() {
            if entry.get("name").and_then(|v| v.as_str()).is_some() {
                return Some(ServerKind::Ollama);
            }
        }
    }
    None
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
    let map = parse_prometheus(text);
    let used = find_f64(&map, &["vllm:kv_cache_usage_sys", "vllm:kv_cache_used"]);
    let total = find_f64(&map, &["vllm:kv_cache_usage_max", "vllm:kv_cache_max"]);
    let (used, total) = (used, total);
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
    let map = parse_prometheus(text);
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

/// Extract the model name and context window from an OpenAI `/v1/models` entry.
///
/// OpenAI-compatible servers list `context_length` (the model's window) on each
/// model object. Cloud providers do NOT expose live context/speed, so we only
/// populate name + context_size + loaded.
pub fn extract_cloud_model(entry: &Value) -> ModelMetric {
    let name = entry
        .get("id")
        .or_else(|| entry.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let context_size = entry
        .get("context_length")
        .or_else(|| entry.get("contextSize"))
        .and_then(|v| v.as_u64());
    ModelMetric {
        name,
        context_size,
        loaded: false,
        quantization: None,
        path: None,
        version: None,
    }
}

/// Extract model info from a deserialized [`ModelEntry`] (OpenAI `/v1/models`).
pub fn extract_model_entry_metric(entry: &ModelEntry) -> ModelMetric {
    ModelMetric {
        name: entry.id.clone().or_else(|| entry.name.clone()).or_else(|| entry.model.clone()),
        // llama.cpp/vLLM expose the window as `n_ctx`; OpenAI-compatible clouds
        // use `context_length`. Take whichever the upstream actually provides.
        context_size: entry.n_ctx.or(entry.context_length),
        loaded: entry.loaded.unwrap_or(false),
        quantization: entry.content_type.clone(),
        path: entry.path.clone(),
        version: None,
    }
}

/// Parse an Ollama `/api/ps` array: returns the running model + context usage.
///
/// Ollama reports how many context tokens each model currently has loaded via
/// the `context` field (used) — when present. `total` comes from `context_length`
/// if the entry carries it.
pub fn parse_ollama_ps(value: &Value) -> (Option<ModelMetric>, Option<ContextMetric>) {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return (None, None),
    };
    let first = match arr.first() {
        Some(e) => e,
        None => return (None, None),
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
            .or_else(|| slot.get("n_prompt_tokens_processed").and_then(|v| v.as_u64()));
        let decoded = slot
            .get("n_decoded")
            .and_then(|v| v.as_u64())
            .or_else(|| {
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
                info.generation_speed = timings.get("predicted_per_second").and_then(|v| v.as_f64());
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

    // --- §15: server returns valid metrics -------------------------------

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

    /// Verbatim `GET /metrics` output captured from a real llama.cpp build
    /// (10380) started with `--metrics`, after one generation request.
    /// Note the `llamacpp:` namespace prefix and that there is NO ctx metric.
    const REAL_METRICS: &str = "\
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
        assert!(!ctx.available, "no context metric exists in the real metric set");
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

    // --- §15: context fully known / partially unknown --------------------

    #[test]
    fn context_full_known() {
        let mut m = std::collections::HashMap::new();
        m.insert("ctx_total".to_string(), 32768.0);
        m.insert("ctx_used".to_string(), 8192.0);
        let c = extract_context(&m);
        assert!(c.available);
        assert_eq!(c.total, Some(32768));
        assert_eq!(c.used, Some(8192));
        assert_eq!(c.remaining, Some(24576));
        assert!((c.percent.unwrap() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn context_total_only_marks_used_unknown() {
        let mut m = std::collections::HashMap::new();
        m.insert("ctx_total".to_string(), 32768.0);
        let c = extract_context(&m);
        assert!(c.available);
        assert_eq!(c.total, Some(32768));
        assert_eq!(c.used, None, "used must be None, never a fake zero");
        assert_eq!(c.remaining, None);
        assert_eq!(c.percent, None);
    }

    #[test]
    fn context_absent_is_unavailable() {
        let m = std::collections::HashMap::new();
        let c = extract_context(&m);
        assert!(!c.available);
        assert_eq!(c.total, None);
    }

    #[test]
    fn context_used_greater_than_total_clamps_remaining() {
        let mut m = std::collections::HashMap::new();
        m.insert("ctx_total".to_string(), 100.0);
        m.insert("ctx_used".to_string(), 150.0);
        let c = extract_context(&m);
        assert_eq!(c.remaining, Some(0), "negative remaining must clamp to 0");
        assert_eq!(c.percent, Some(1.0), "percent must clamp to 1.0");
    }

    // --- §15: speeds available / unavailable -----------------------------

    #[test]
    fn speeds_available_when_rate_present() {
        let mut m = std::collections::HashMap::new();
        m.insert("prompt_tokens_per_second".to_string(), 400.0);
        m.insert("generation_tokens_per_second".to_string(), 40.0);
        let (p, g) = extract_speeds(&m);
        assert!(p.available);
        assert_eq!(p.current, Some(400.0));
        assert!(g.available);
        assert_eq!(g.current, Some(40.0));
        assert!(p.split_available);
    }

    #[test]
    fn speeds_unavailable_when_absent() {
        let m = std::collections::HashMap::new();
        let (p, g) = extract_speeds(&m);
        assert!(!p.available);
        assert_eq!(p.current, None, "must be None, not 0.0");
        assert!(!g.available);
        assert!(!p.split_available);
    }

    // --- §15: model known / unknown / not loaded -------------------------

    #[test]
    fn model_from_models_list() {
        let json = json!({
            "models": [
                { "id": "ornith-1.5-35b", "n_ctx": 32768, "loaded": true }
            ]
        });
        let resp: ModelsResponse = serde_json::from_value(json).unwrap();
        let m = extract_model(&resp);
        assert_eq!(m.name.as_deref(), Some("ornith-1.5-35b"));
        assert_eq!(m.context_size, Some(32768));
        assert!(m.loaded);
    }

    #[test]
    fn model_unknown_yields_no_name() {
        let resp = ModelsResponse { models: None };
        let m = extract_model(&resp);
        assert_eq!(m.name, None, "must not invent a model name");
        assert!(!m.loaded);
    }

    #[test]
    fn model_from_props_fallback() {
        let json = json!({
            "llama": { "model": "fallback-model", "context": 4096, "quantization": "Q5_K_M" }
        });
        let props: PropsResponse = serde_json::from_value(json).unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name.as_deref(), Some("fallback-model"));
        assert_eq!(m.context_size, Some(4096));
        assert_eq!(m.quantization.as_deref(), Some("Q5_K_M"));
    }

    /// Real payload shape from a modern llama.cpp build: a flat object with
    /// `model_alias` / `model_ftype` / `model_path` and nested
    /// `default_generation_settings.n_ctx`. There is no `llama` object.
    #[test]
    fn model_from_modern_flat_props() {
        let json = json!({
            "default_generation_settings": { "n_ctx": 192256 },
            "total_slots": 4,
            "model_alias": "F:\\\\Models\\\\gemma-4-12B-it-QAT-Q4_0.gguf",
            "model_ftype": "Q4_0",
            "model_path": "F:\\\\Models\\\\gemma-4-12B-it-QAT-Q4_0.gguf",
            "endpoint_metrics": false,
            "build_info": "b10897-18c17b4d6",
            "is_sleeping": false
        });
        let props: PropsResponse = serde_json::from_value(json).unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name.as_deref(), Some("F:\\\\Models\\\\gemma-4-12B-it-QAT-Q4_0.gguf"));
        assert_eq!(m.quantization.as_deref(), Some("Q4_0"));
        assert_eq!(m.context_size, Some(192_256));
        assert_eq!(m.version.as_deref(), Some("b10897-18c17b4d6"));
        assert!(m.loaded);
    }

    #[test]
    fn model_from_props_falls_back_to_model_path_when_alias_empty() {
        let json = json!({
            "model_alias": "",
            "model_path": "/models/mistral-7b.gguf",
            "model_ftype": "Q8_0"
        });
        let props: PropsResponse = serde_json::from_value(json).unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name.as_deref(), Some("/models/mistral-7b.gguf"));
    }

    // --- §15: malformed JSON must not panic ------------------------------

    #[test]
    fn malformed_models_json_does_not_panic() {
        let parsed: Result<ModelsResponse, _> = serde_json::from_str("{ this is not json");
        assert!(parsed.is_err());
    }

    #[test]
    fn empty_props_is_tolerated() {
        let props: PropsResponse = serde_json::from_str("{}").unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name, None);
    }

    // --- /slots (native fallback when --metrics is absent) ---------------

    /// Real payload captured live from llama.cpp (idle slots + one active).
    #[test]
    fn parses_slots_with_active_slot() {
        let json = json!([
            {"id":0,"n_ctx":192256,"speculative":false,"is_processing":false},
            {"id":1,"n_ctx":192256,"speculative":false,"is_processing":false},
            {"id":2,"n_ctx":192256,"speculative":false,"is_processing":false},
            {"id":3,"n_ctx":192256,"speculative":false,"is_processing":true,
             "id_task":1,"n_prompt_tokens":2418,"n_prompt_tokens_processed":0,
             "n_prompt_tokens_cache":0,
             "next_token":[{"has_next_token":true,"has_new_line":false,
                            "n_remain":-1,"n_decoded":0}]}
        ]);
        let info = parse_slots(&json);
        assert!(info.active, "slot 3 is processing");
        assert_eq!(info.context_size, Some(192_256));
        assert!(info.context.available);
        assert_eq!(info.context.total, Some(192_256));
        assert_eq!(info.context.used, Some(2418));
        assert_eq!(info.context.remaining, Some(192_256 - 2418));
    }

    #[test]
    fn parses_slots_all_idle_reports_zero_used() {
        // Truly untouched slots expose no token counters at all -> 0 used.
        let json = json!([
            {"id":0,"n_ctx":8192,"is_processing":false},
            {"id":1,"n_ctx":8192,"is_processing":false}
        ]);
        let info = parse_slots(&json);
        assert!(!info.active);
        assert!(info.context.available);
        assert_eq!(info.context.used, Some(0));
        assert_eq!(info.context.remaining, Some(8192));
    }

    /// Regression: after generation ends, `is_processing` flips to false but the
    /// slot KEEPS its `n_prompt_tokens` counter (observed live on llama.cpp).
    /// The widget must keep showing that occupied context, not snap back to 0.
    #[test]
    fn parses_slots_keeps_context_after_generation_ends() {
        let json = json!([
            {"id":0,"n_ctx":192256,"is_processing":false},
            {"id":1,"n_ctx":192256,"is_processing":false},
            {"id":2,"n_ctx":192256,"is_processing":false},
            {"id":3,"n_ctx":192256,"is_processing":false,
             "n_prompt_tokens":226,"n_prompt_tokens_processed":0}
        ]);
        let info = parse_slots(&json);
        assert!(!info.active, "no slot is currently processing");
        assert!(info.context.available);
        assert_eq!(info.context.used, Some(226), "retained prompt must be counted");
        assert_eq!(info.context.remaining, Some(192_256 - 226));
    }

    /// Regression: a just-queued request reports `n_prompt_tokens_processed = 0`
    /// while `n_prompt_tokens` already holds the real prompt size. We must use
    /// the planned size, otherwise the context bar shows 0% during prefill.
    #[test]
    fn parses_slots_prefers_planned_prompt_over_unprocessed_zero() {
        let json = json!([
            {"id":0,"n_ctx":32768,"is_processing":true,
             "n_prompt_tokens":2418,"n_prompt_tokens_processed":0}
        ]);
        let info = parse_slots(&json);
        assert_eq!(info.context.used, Some(2418));
    }

    // --- model merging (/v1/models + /props) -----------------------------

    #[test]
    fn merge_model_props_fills_gaps_left_by_models_list() {
        // /v1/models knows only the name; /props adds quantization + version.
        let mut base = ModelMetric {
            name: Some("gemma-4-12B".to_string()),
            loaded: false,
            ..Default::default()
        };
        let props = ModelMetric {
            name: Some("gemma-4-12B".to_string()),
            context_size: Some(192_256),
            quantization: Some("Q4_0".to_string()),
            version: Some("b10897".to_string()),
            loaded: true,
            ..Default::default()
        };
        merge_model(&mut base, props);
        assert_eq!(base.name.as_deref(), Some("gemma-4-12B"));
        assert_eq!(base.context_size, Some(192_256));
        assert_eq!(base.quantization.as_deref(), Some("Q4_0"));
        assert_eq!(base.version.as_deref(), Some("b10897"));
        assert!(base.loaded);
    }

    #[test]
    fn merge_model_does_not_erase_existing_values_with_none() {
        let mut base = ModelMetric {
            name: Some("kept".to_string()),
            quantization: Some("Q5_K_M".to_string()),
            ..Default::default()
        };
        merge_model(&mut base, ModelMetric::default());
        assert_eq!(base.name.as_deref(), Some("kept"));
        assert_eq!(base.quantization.as_deref(), Some("Q5_K_M"));
    }

    #[test]
    fn parses_slots_with_timings_for_speeds() {
        let json = json!([
            {"id":0,"n_ctx":4096,"is_processing":true,"n_prompt_tokens_processed":512,"n_decoded":64,
             "timings":{"prompt_per_second":412.5,"predicted_per_second":38.25}}
        ]);
        let info = parse_slots(&json);
        assert_eq!(info.prefill_speed, Some(412.5));
        assert_eq!(info.generation_speed, Some(38.25));
    }

    #[test]
    fn parses_slots_non_array_is_tolerated() {
        // A server that answers /slots with an error object must not panic.
        let info = parse_slots(&json!({"error":{"code":501,"message":"nope"}}));
        assert!(!info.active);
        assert!(!info.context.available);
        assert!(info.context_opt().is_none());
    }

    #[test]
    fn parses_slots_used_is_clamped_to_total() {
        let json = json!([
            {"id":0,"n_ctx":100,"is_processing":true,"n_prompt_tokens_processed":500,"n_decoded":900}
        ]);
        let info = parse_slots(&json);
        assert_eq!(info.context.used, Some(100));
        assert_eq!(info.context.remaining, Some(0));
        assert_eq!(info.context.percent, Some(1.0));
    }

    // --- vLLM / Ollama / Cloud adapters (multi-server feature) ---------------

    #[test]
    fn detects_vllm_from_metrics_prefix() {
        assert_eq!(
            detect_kind_from_metrics("# HELP vllm:kv_cache_usage_sys"),
            Some(ServerKind::Vllm)
        );
        assert_eq!(
            detect_kind_from_metrics("# HELP llamacpp:prompt_tokens_total"),
            Some(ServerKind::Local)
        );
        assert_eq!(detect_kind_from_metrics("# just some help"), None);
    }

    #[test]
    fn detects_ollama_from_ps_array() {
        let v = json!([{"name":"llama3.1","info":{"model":"meta-llama/..."}}]);
        assert_eq!(detect_kind_from_ps(&v), Some(ServerKind::Ollama));
        // A non-array or nameless payload is not Ollama.
        assert_eq!(detect_kind_from_ps(&json!({"ok":true})), None);
    }

    #[test]
    fn vllm_context_ratio_is_clamped() {
        let text = "\
# TYPE vllm:kv_cache_usage_sys gauge
vllm:kv_cache_usage_sys 8000
# TYPE vllm:kv_cache_usage_max gauge
vllm:kv_cache_usage_max 32768
";
        let c = extract_vllm_context(text);
        assert_eq!(c.used, Some(8000));
        assert_eq!(c.total, Some(32768));
        assert!((c.percent.unwrap() - 8000.0 / 32768.0).abs() < 1e-9);
    }

    #[test]
    fn vllm_context_missing_fields_is_unavailable() {
        let c = extract_vllm_context("# no metrics here\n");
        assert!(c.percent.is_none());
    }

    #[test]
    fn vllm_speeds_invert_latency_to_tok_per_second() {
        let text = "\
vllm:time_to_first_token_seconds 0.2
vllm:inter_token_latency_seconds 0.05
";
        let (prefill, generation) = extract_vllm_speeds(text);
        assert!((prefill.current.unwrap() - 5.0).abs() < 1e-9); // 1 / 0.2
        assert!((generation.current.unwrap() - 20.0).abs() < 1e-9); // 1 / 0.05
        assert!(prefill.split_available);
    }

    #[test]
    fn vllm_speeds_zero_latency_is_unavailable() {
        let text = "vllm:time_to_first_token_seconds 0\n";
        let (prefill, _) = extract_vllm_speeds(text);
        assert!(!prefill.available);
    }

    #[test]
    fn ollama_ps_reports_model_and_context() {
        let v = json!([
            {"name":"gemma-2-2b","context_length":8192,
             "info":{"model":"gemma-2:latest"},"context":2048}
        ]);
        let (model, ctx) = parse_ollama_ps(&v);
        let model = model.unwrap();
        // Compare the owned `Option<String>` (no cross-lifetime `&str` quirk).
        assert_eq!(model.name, Some("gemma-2-2b".to_string()));
        let ctx = ctx.unwrap();
        assert!(ctx.available);
        assert_eq!(ctx.used, Some(2048));
        assert_eq!(ctx.total, Some(8192));
    }

    #[test]
    fn ollama_ps_context_total_only_when_no_loaded_tokens() {
        let v = json!([{"name":"qwen","context_length":4096,"info":{}}]);
        let (_, ctx) = parse_ollama_ps(&v);
        let c = ctx.unwrap();
        assert!(c.available);
        assert_eq!(c.total, Some(4096));
        assert_eq!(c.used, None, "loaded tokens unknown -> None, not 0");
    }

    #[test]
    fn cloud_model_reads_context_length() {
        let v = json!({"id":"gpt-4o-mini","context_length":128000});
        let m = extract_cloud_model(&v);
        assert_eq!(m.name.as_deref(), Some("gpt-4o-mini"));
        assert_eq!(m.context_size, Some(128000));
        assert!(!m.loaded);
    }

    #[test]
    fn cloud_model_without_context_length_has_no_size() {
        let v = json!({"id":"gpt-3.5-turbo"});
        let m = extract_cloud_model(&v);
        assert_eq!(m.name.as_deref(), Some("gpt-3.5-turbo"));
        assert_eq!(m.context_size, None);
    }
}
