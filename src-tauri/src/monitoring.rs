//! Monitoring service: background polling with retry/backoff and smoothing.
//!
//! Runs on a dedicated background task so it never blocks the UI thread.
//! On errors it degrades state gracefully and back-offs instead of spamming.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::api::adapters::*;
use crate::api::calculator::{calculate, SpeedWindow};
use crate::api::types::{ConnectionStatus, ContextMetric, MonitoringState, SpeedMetric};
use crate::config::{ServerKind, Settings};

/// Background polling service (shared, behind Arc for tokio task).
pub struct MonitoringService {
    /// Sliding window of recent speed samples — reset when the active server
    /// changes so we never average speeds from two different upstreams.
    window: Mutex<SpeedWindow>,
    /// Previous state, used by the calculator for staleness + smoothing.
    prev_state: RwLock<Option<MonitoringState>>,
    /// The active server id we polled last; lets us detect a switch.
    last_server_id: Mutex<Option<String>>,
}

impl MonitoringService {
    pub fn new() -> Self {
        MonitoringService {
            window: Mutex::new(SpeedWindow::new(30)),
            prev_state: RwLock::new(None),
            last_server_id: Mutex::new(None),
        }
    }

    /// Clone into an Arc and spawn the polling loop. The loop reads the *live*
    /// settings from `AppState` on every poll, so server switches made through
    /// the UI take effect immediately (no restart needed).
    pub fn spawn(self: Arc<Self>, app_handle: AppHandle) {
        tauri::async_runtime::spawn(async move {
            Self::run_loop(app_handle, self).await;
        });
    }

    async fn run_loop(app_handle: AppHandle, state: Arc<MonitoringService>) {
        // Interval is seeded once from the current settings; a later change to
        // the poll interval applies on next restart (recreating the interval on
        // every tick would reset its cadence). Everything else is read live.
        let initial = app_handle
            .state::<crate::AppState>()
            .settings
            .lock()
            .unwrap()
            .clone();
        let mut interval =
            tokio::time::interval(Duration::from_millis(initial.poll_interval_ms.max(250)));
        interval.tick().await; // consume initial tick

        // Poll once immediately for a fast first frame.
        Self::poll_and_emit(&app_handle, &state).await;

        loop {
            interval.tick().await;
            Self::poll_and_emit(&app_handle, &state).await;
        }
    }

    async fn poll_and_emit(app_handle: &AppHandle, state: &Arc<MonitoringService>) {
        // Read the live settings so a server switch (or any other change) made
        // through the UI is picked up on the very next poll. The service no
        // longer holds a stale clone captured at startup.
        let settings = app_handle
            .state::<crate::AppState>()
            .settings
            .lock()
            .unwrap()
            .clone();

        // Detect an active-server change and reset smoothing/previous state so
        // we don't blend speeds or context from two different upstreams.
        let active_id = settings
            .active_server_id
            .clone()
            .or_else(|| settings.servers.first().map(|p| p.id.clone()));
        {
            let mut last = state.last_server_id.lock().unwrap();
            if *last != active_id {
                *state.window.lock().unwrap() =
                    SpeedWindow::new(settings.smoothing_window_secs);
                *state.prev_state.write().unwrap() = None;
                *last = active_id;
            }
        }

        match Self::fetch_snapshot(&settings).await {
            Ok(snap) => {
                let stale_secs = (settings.poll_interval_ms / 1000) * 3;
                let mut window = state.window.lock().unwrap();
                let prev = state.prev_state.read().unwrap();
                let result = calculate(&snap, prev.as_ref(), &mut window, stale_secs);
                drop(prev);
                *state.prev_state.write().unwrap() = Some(result.state.clone());
                let _ = app_handle.emit("monitoring:update", result.state.clone());
                crate::api::cache_state(result.state);
            }
            Err(err) => {
                let label = match settings.active_profile() {
                    Some(p) if !p.label.is_empty() => p.label.clone(),
                    Some(p) => p.url.clone(),
                    None => "llama.cpp".to_string(),
                };
                let degraded = MonitoringState {
                    connection: ConnectionStatus::ServerUnavailable,
                    last_update: state
                        .prev_state
                        .read()
                        .unwrap()
                        .as_ref()
                        .and_then(|s| s.last_update),
                    server_label: Some(label),
                    diagnostics: vec![format!("Ошибка опроса: {err}")],
                    ..Default::default()
                };
                let _ = app_handle.emit("monitoring:update", degraded);
            }
        }
    }

    /// Fetch and parse a snapshot with light, non-inference requests.
    ///
    /// Dispatches on the active server's [`ServerKind`] so each upstream engine
    /// is polled through its own endpoints and parsed by its own adapter.
    pub async fn fetch_snapshot(settings: &Settings) -> anyhow::Result<ParsedSnapshot> {
        let kind = settings
            .active_profile()
            .map(|p| p.kind)
            .unwrap_or(ServerKind::Local);
        let api_key = settings.active_profile().and_then(|p| p.api_key.clone());
        let base = settings.base_url();
        let openai = settings.openai_base_url();

        // NOTE: connection reuse is deliberately disabled.
        //
        // Some llama.cpp builds return a spurious `404 Not Found` when a
        // keep-alive socket is reused (reproduced live: the same client + URL
        // succeeded 5/10 times with pooling and 10/10 with pooling off). Since
        // we poll a handful of tiny endpoints every few seconds, the cost of a
        // fresh connection is negligible, while a flaky 404 would silently blank
        // the speeds and emit a misleading "metrics disabled" diagnostic.
        // NOTE: for loopback servers proxies must be bypassed unconditionally.
        // A monitoring widget inherits whatever `HTTP_PROXY`/`HTTPS_PROXY` the
        // parent environment exports, and reqwest honours them even for loopback
        // URLs — which silently turns every poll into a CONNECT attempt against
        // an unrelated proxy (observed live: a sandbox-injected
        // `http_proxy=http://127.0.0.1:60410` made the health check never reach
        // the llama.cpp server). A local monitor must never depend on a proxy
        // being correct, so `.no_proxy()` is explicit for loopback. For remote
        // and cloud servers we keep the system proxy instead.
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_millis(settings.request_timeout_ms))
            .pool_max_idle_per_host(0);
        if settings.active_profile().map(|p| p.is_loopback()).unwrap_or(true) {
            builder = builder.no_proxy();
        }
        let client = builder.build()?;

        match kind {
            ServerKind::Local => Self::fetch_llamacpp(&client, base, openai, api_key.as_deref()).await,
            ServerKind::Vllm => Self::fetch_vllm(&client, base, openai, api_key.as_deref()).await,
            ServerKind::Ollama => Self::fetch_ollama(&client, base, api_key.as_deref()).await,
            ServerKind::Cloud => Self::fetch_cloud(&client, openai, api_key.as_deref()).await,
        }
    }

    /// Attach an `Authorization: Bearer <key>` header when an API key is present.
    fn auth<'a>(
        req: reqwest::RequestBuilder,
        api_key: Option<&'a str>,
    ) -> reqwest::RequestBuilder {
        match api_key {
            Some(key) if !key.is_empty() => req.bearer_auth(key),
            _ => req,
        }
    }

    /// llama.cpp (local or remote): health, /v1/models, /props, /metrics, /slots.
    async fn fetch_llamacpp(
        client: &reqwest::Client,
        base: String,
        openai: String,
        api_key: Option<&str>,
    ) -> anyhow::Result<ParsedSnapshot> {
        let mut snap = ParsedSnapshot::default();

        // 1. Health check (native endpoint, server root).
        let health = Self::auth(client.get(format!("{base}/health")), api_key).send().await?;
        if !health.status().is_success() {
            anyhow::bail!("health endpoint returned {}", health.status());
        }

        // 2. Models (OpenAI-compatible route, lives under /v1) — for the model name.
        if let Ok(models) = Self::auth(client.get(format!("{openai}/models")), api_key).send().await {
            if let Ok(json) = models.json::<ModelsResponse>().await {
                snap.model = extract_model(&json);
            }
        }

        // 3. Props (native endpoint, server root) — model name, context size, quantization.
        //    Props is the authoritative source for the *loaded* model, so it wins on
        //    any field it actually provides; /v1/models only fills the gaps.
        if let Ok(props) = Self::auth(client.get(format!("{base}/props")), api_key).send().await {
            if let Ok(json) = props.json::<PropsResponse>().await {
                let m = extract_model_from_props(&json);
                merge_model(&mut snap.model, m);
                snap.server_label = extract_server_label(&json).or(snap.server_label);
            }
        }

        // 4. Metrics (speeds; context only on builds that publish it) — Prometheus
        //    text, else JSON, at the server root. Many builds run WITHOUT --metrics
        //    (HTTP 501); in that case we fall back to /slots below, which is on by
        //    default. Only a *definitive* 501/404 produces a diagnostic — transient
        //    failures (connection refused, 502 during model reload, timeouts) stay
        //    silent, otherwise every restart would flash a misleading warning.
        let mut got_metrics = false;
        match Self::auth(client.get(format!("{base}/metrics")), api_key).send().await {
            Ok(resp) if resp.status().is_success() => {
                let ct = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                if ct.contains("text/plain") {
                    if let Ok(text) = resp.text().await {
                        let map = parse_prometheus(&text);
                        apply_metrics(&mut snap, &map);
                        got_metrics = true;
                    }
                } else {
                    // JSON-style metrics from alternative builds.
                    match resp.json::<MetricsResponse>().await {
                        Ok(json) => {
                            let map = parse_json_metrics(&serde_json::Value::Object(
                                json.fields.into_iter().collect(),
                            ));
                            apply_metrics(&mut snap, &map);
                            got_metrics = true;
                        }
                        Err(e) => {
                            log::debug!("metrics JSON parse failed: {e}");
                        }
                    }
                }
            }
            Ok(resp) => {
                let code = resp.status().as_u16();
                if code == 501 || code == 404 {
                    // Definitive: the server was started without --metrics.
                    snap.diagnostics
                        .push("Метрики отключены на сервере (нет --metrics)".to_string());
                } else {
                    log::debug!("metrics endpoint returned {code}");
                }
            }
            Err(e) => {
                // Connection-level failure: the health check already covers the
                // "server down" case, so don't add a second, misleading message.
                log::debug!("metrics request failed: {e}");
            }
        }

        // 5. Slots fallback (native endpoint, server root). Also queried when metrics are
        //    available, because /slots is the only source of the live per-slot context usage.
        if let Ok(resp) = Self::auth(client.get(format!("{base}/slots")), api_key).send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    let s = parse_slots(&json);
                    // Prefer Prometheus/JSON metrics when they supplied real context
                    // numbers; otherwise take what /slots reports.
                    if !snap.context.available {
                        if let Some(c) = s.context_opt() {
                            snap.context = c;
                        }
                    }
                    // /slots timings give the instantaneous rate, which we label as
                    // "current". The avg_30s value is filled in by the calculator.
                    if !snap.prefill_speed.available {
                        if let Some(v) = s.prefill_speed {
                            snap.prefill_speed = SpeedMetric {
                                current: Some(v),
                                available: true,
                                ..Default::default()
                            };
                        }
                    }
                    if !snap.generation_speed.available {
                        if let Some(v) = s.generation_speed {
                            snap.generation_speed = SpeedMetric {
                                current: Some(v),
                                available: true,
                                ..Default::default()
                            };
                        }
                    }
                    if snap.model.context_size.is_none() {
                        snap.model.context_size = s.context_size;
                    }
                    if !s.active && !got_metrics {
                        snap.diagnostics.push("Сервер простаивает".to_string());
                    }
                }
            }
        }

        snap.timestamp_ms = now_ms();
        Ok(snap)
    }

    /// vLLM: health + /v1/models + Prometheus `/metrics` with `vllm:` metrics.
    /// vLLM gives live context (KV-cache occupancy) and per-token latencies, so
    /// we surface context percent + prefill/generation speeds from `/metrics`.
    async fn fetch_vllm(
        client: &reqwest::Client,
        base: String,
        openai: String,
        api_key: Option<&str>,
    ) -> anyhow::Result<ParsedSnapshot> {
        let mut snap = ParsedSnapshot::default();

        // vLLM exposes /health at the root.
        let health = Self::auth(client.get(format!("{base}/health")), api_key).send().await?;
        if !health.status().is_success() {
            anyhow::bail!("health endpoint returned {}", health.status());
        }

        // Model name from /v1/models.
        if let Ok(models) = Self::auth(client.get(format!("{openai}/models")), api_key).send().await {
            if let Ok(json) = models.json::<ModelsResponse>().await {
                if let Some(entry) = json.models.as_ref().and_then(|v| v.first()) {
                    snap.model = extract_model_entry_metric(entry);
                }
            }
        }

        // Metrics (Prometheus, vllm: prefix) — context (KV occupancy) + speeds.
        if let Ok(resp) = Self::auth(client.get(format!("{base}/metrics")), api_key).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Some(ServerKind::Vllm) = detect_kind_from_metrics(&text) {
                        let ctx = extract_vllm_context(&text);
                        if let Some(percent) = ctx.percent {
                            snap.context = ContextMetric {
                                total: ctx.total,
                                used: ctx.used,
                                remaining: None,
                                percent: Some(percent),
                                available: true,
                            };
                        }
                        let (p, g) = extract_vllm_speeds(&text);
                        snap.prefill_speed = p;
                        snap.generation_speed = g;
                    } else {
                        snap.diagnostics
                            .push("vLLM метрики не обнаружены ( нет vllm: prefix)".to_string());
                    }
                }
            }
        }

        snap.timestamp_ms = now_ms();
        Ok(snap)
    }

    /// Ollama: /api/ps lists running models with context usage. No live speeds.
    async fn fetch_ollama(
        client: &reqwest::Client,
        native: String,
        api_key: Option<&str>,
    ) -> anyhow::Result<ParsedSnapshot> {
        let mut snap = ParsedSnapshot::default();

        // Ollama's /api/ps lists running models (native base already includes /api).
        if let Ok(resp) = Self::auth(client.get(format!("{native}/ps")), api_key).send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(ServerKind::Ollama) = detect_kind_from_ps(&json) {
                        if let (Some(model), Some(ctx)) = parse_ollama_ps(&json) {
                            snap.model = model;
                            snap.context = ctx;
                        }
                    } else {
                        snap.diagnostics
                            .push("Ollama: /api/ps не вернул запущенные модели".to_string());
                    }
                }
            }
        }

        snap.timestamp_ms = now_ms();
        Ok(snap)
    }

    /// Cloud (OpenAI-compatible): only /v1/models. No health endpoint, no live
    /// context or speeds — the model name + context window is all we get.
    async fn fetch_cloud(
        client: &reqwest::Client,
        openai: String,
        api_key: Option<&str>,
    ) -> anyhow::Result<ParsedSnapshot> {
        let mut snap = ParsedSnapshot::default();

        // Cloud: only /v1/models (with Authorization if api_key present).
        let resp = Self::auth(client.get(format!("{openai}/models")), api_key).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("models endpoint returned {}", resp.status());
        }
        if let Ok(json) = resp.json::<ModelsResponse>().await {
            if let Some(entry) = json.models.as_ref().and_then(|v| v.first()) {
                snap.model = extract_model_entry_metric(entry);
            }
        }

        snap.timestamp_ms = now_ms();
        Ok(snap)
    }
}

/// Apply a parsed metrics map to a snapshot.
///
/// Context is only taken when the build actually publishes a ctx metric — most
/// modern builds expose none under `--metrics`, so the `/slots` step later fills
/// it in. Speeds are always taken from whatever the map provides.
fn apply_metrics(snap: &mut ParsedSnapshot, map: &std::collections::HashMap<String, f64>) {
    let ctx = extract_context(map);
    if ctx.available {
        snap.context = ctx;
    }
    let (p, g) = extract_speeds(map);
    snap.prefill_speed = p;
    snap.generation_speed = g;
}
