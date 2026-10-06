//! Model identity: raw `/v1/models` and `/props` shapes plus the logic that
//! merges them into a single [`ModelMetric`].

use serde::Deserialize;
use serde_json::Value;

use crate::api::types::ModelMetric;

/// Raw models list (`GET /v1/models`).
#[derive(Debug, Deserialize, Default)]
pub struct ModelsResponse {
    pub models: Option<Vec<ModelEntry>>,
}

/// One entry of `GET /v1/models`.
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
    /// only populate this field.
    pub context_length: Option<u64>,
    pub loaded: Option<bool>,
}

/// Raw server properties / status (`GET /props`).
#[derive(Debug, Deserialize, Default)]
pub struct PropsResponse {
    pub llama: Option<LlamaProps>,
    pub server: Option<ServerProps>,
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, Value>,
}

/// Legacy nested `llama` object of `/props`.
#[derive(Debug, Deserialize, Default)]
pub struct LlamaProps {
    pub model: Option<String>,
    pub context: Option<u64>,
    pub quantization: Option<String>,
}

/// Legacy nested `server` object of `/props`.
#[derive(Debug, Deserialize, Default)]
pub struct ServerProps {
    pub hostname: Option<String>,
    pub port: Option<u16>,
    pub model: Option<String>,
    pub version: Option<String>,
}

/// Extract model info from a `/v1/models` response.
///
/// A server may advertise several models; prefer the one it reports as
/// `loaded`, otherwise take the first entry.
pub fn extract_model(models: &ModelsResponse) -> ModelMetric {
    let entries = models.models.as_deref();
    let entry = entries
        .and_then(|v| v.iter().find(|m| m.loaded == Some(true)))
        .or_else(|| entries.and_then(|v| v.first()));

    entry.map(model_from_entry).unwrap_or_default()
}

/// Map a single [`ModelEntry`] to a [`ModelMetric`].
pub fn model_from_entry(entry: &ModelEntry) -> ModelMetric {
    ModelMetric {
        name: entry
            .id
            .clone()
            .or_else(|| entry.name.clone())
            .or_else(|| entry.model.clone()),
        // llama.cpp/vLLM expose the window as `n_ctx`; OpenAI-compatible clouds
        // use `context_length`. Take whichever the upstream actually provides.
        context_size: entry.n_ctx.or(entry.context_length),
        loaded: entry.loaded.unwrap_or(false),
        quantization: entry.content_type.clone(),
        path: entry.path.clone(),
        version: None,
    }
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

/// Extract a readable server label from props.
///
/// Modern builds have no `server` object. We synthesize a label from
/// `hostname`/`port` when present; otherwise the caller falls back to the
/// configured profile label or URL.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str, loaded: Option<bool>) -> ModelEntry {
        ModelEntry {
            id: Some(id.to_string()),
            loaded,
            ..Default::default()
        }
    }

    /// A server advertising several models must report the one it says is
    /// loaded. The previous implementation called `first()` before looking for
    /// `loaded == true`, which made the preference unreachable for any
    /// non-empty list.
    #[test]
    fn prefers_the_loaded_model_over_the_first() {
        let resp = ModelsResponse {
            models: Some(vec![
                entry("first-model", Some(false)),
                entry("loaded-model", Some(true)),
            ]),
        };
        assert_eq!(
            extract_model(&resp).name.as_deref(),
            Some("loaded-model"),
            "the loaded entry must win"
        );
    }

    /// With no `loaded` flag anywhere, the first entry is still the answer.
    #[test]
    fn falls_back_to_the_first_model_when_none_is_flagged() {
        let resp = ModelsResponse {
            models: Some(vec![entry("alpha", None), entry("beta", None)]),
        };
        assert_eq!(extract_model(&resp).name.as_deref(), Some("alpha"));
    }

    /// An empty or missing list must not panic and must not invent a name.
    #[test]
    fn empty_models_list_yields_default() {
        let empty = ModelsResponse {
            models: Some(vec![]),
        };
        assert_eq!(extract_model(&empty).name, None);
        assert!(!extract_model(&empty).loaded);
        assert_eq!(extract_model(&ModelsResponse::default()).name, None);
    }

    /// OpenAI-compatible clouds only send `context_length`; the window must
    /// still reach the UI.
    #[test]
    fn context_length_is_used_when_n_ctx_is_absent() {
        let resp = ModelsResponse {
            models: Some(vec![ModelEntry {
                id: Some("gpt-x".to_string()),
                context_length: Some(128_000),
                ..Default::default()
            }]),
        };
        assert_eq!(extract_model(&resp).context_size, Some(128_000));
    }

    #[test]
    fn props_flat_form_wins_over_legacy_nested() {
        let props: PropsResponse = serde_json::from_value(json!({
            "model_alias": "flat-name",
            "model_ftype": "Q4_K_M",
            "model_path": "/models/x.gguf",
            "default_generation_settings": { "n_ctx": 8192 },
            "build_info": "b10929",
            "llama": { "model": "legacy-name", "context": 2048 }
        }))
        .unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name.as_deref(), Some("flat-name"));
        assert_eq!(m.context_size, Some(8192));
        assert_eq!(m.quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(m.version.as_deref(), Some("b10929"));
        assert!(m.loaded);
    }

    #[test]
    fn props_legacy_nested_form_still_parses() {
        let props: PropsResponse = serde_json::from_value(json!({
            "llama": { "model": "legacy", "context": 4096, "quantization": "Q8_0" }
        }))
        .unwrap();
        let m = extract_model_from_props(&props);
        assert_eq!(m.name.as_deref(), Some("legacy"));
        assert_eq!(m.context_size, Some(4096));
        assert_eq!(m.quantization.as_deref(), Some("Q8_0"));
    }

    /// A zero `n_ctx` is a "not set" marker on some builds and must not win.
    #[test]
    fn props_zero_n_ctx_is_ignored() {
        let props: PropsResponse = serde_json::from_value(json!({
            "default_generation_settings": { "n_ctx": 0 },
            "n_ctx": 0,
            "llama": { "context": 4096 }
        }))
        .unwrap();
        assert_eq!(extract_model_from_props(&props).context_size, Some(4096));
    }

    #[test]
    fn merge_keeps_existing_fields_and_ors_loaded() {
        let mut base = ModelMetric {
            name: Some("from-models".to_string()),
            context_size: Some(4096),
            loaded: false,
            ..Default::default()
        };
        let incoming = ModelMetric {
            name: Some("from-props".to_string()),
            loaded: true,
            ..Default::default()
        };
        merge_model(&mut base, incoming);
        assert_eq!(base.name.as_deref(), Some("from-props"));
        // Untouched fields survive.
        assert_eq!(base.context_size, Some(4096));
        assert!(base.loaded);
    }

    #[test]
    fn server_label_uses_host_and_port() {
        let props: PropsResponse =
            serde_json::from_value(json!({ "server": { "hostname": "box", "port": 8080 } }))
                .unwrap();
        assert_eq!(extract_server_label(&props).as_deref(), Some("box:8080"));
    }
}
