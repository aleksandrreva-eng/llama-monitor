//! Monitoring service: background polling with retry/backoff and smoothing.
//!
//! Runs on a dedicated background task so it never blocks the UI thread.
//! On errors it degrades state gracefully and back-offs instead of spamming.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::api::adapters::*;
use crate::api::calculator::{calculate, SpeedWindow};
use crate::api::types::{ConnectionStatus, MonitoringState, SpeedMetric};
use crate::config::Settings;

/// Background polling service (shared, behind Arc for tokio task).
pub struct MonitoringService {
    settings: Settings,
    window: SpeedWindow,
    prev_state: RwLock<Option<MonitoringState>>,
}

impl MonitoringService {
    pub fn new(settings: Settings) -> Self {
        MonitoringService {
            settings: settings.clone(),
            window: SpeedWindow::new(settings.smoothing_window_secs),
            prev_state: RwLock::new(None),
        }
    }

    /// Clone into an Arc and spawn the polling loop.
    pub fn spawn(self: Arc<Self>, app_handle: AppHandle) {
        let settings = self.settings.clone();
        tauri::async_runtime::spawn(async move {
            Self::run_loop(app_handle, self, settings).await;
        });
    }

    async fn run_loop(app_handle: AppHandle, state: Arc<MonitoringService>, settings: Settings) {
        let mut interval = tokio::time::interval(Duration::from_millis(settings.poll_interval_ms));
        interval.tick().await; // consume initial tick

        // Poll once immediately for a fast first frame.
        Self::poll_and_emit(&app_handle, &state, &settings).await;

        loop {
            interval.tick().await;
            Self::poll_and_emit(&app_handle, &state, &settings).await;
        }
    }

    async fn poll_and_emit(app_handle: &AppHandle, state: &Arc<MonitoringService>, settings: &Settings) {
        match Self::fetch_snapshot(settings).await {
            Ok(snap) => {
                let stale_secs = settings.poll_interval_ms / 1000 * 3;
                let result = calculate(
                    &snap,
                    state.prev_state.read().unwrap().as_ref(),
                    &mut state.window.clone(),
                    stale_secs,
                );
                *state.prev_state.write().unwrap() = Some(result.state.clone());
                let _ = app_handle.emit("monitoring:update", result.state.clone());
                crate::api::cache_state(result.state);
            }
            Err(err) => {
                let degraded = MonitoringState {
                    connection: ConnectionStatus::ServerUnavailable,
                    last_update: state.prev_state.read().unwrap().as_ref().and_then(|s| s.last_update),
                    server_label: settings
                        .server_label
                        .clone()
                        .or_else(|| Some(format!("{}:{}", settings.server_host, settings.server_port))),
                    diagnostics: vec![format!("Ошибка опроса: {err}")],
                    ..Default::default()
                };
                let _ = app_handle.emit("monitoring:update", degraded);
            }
        }
    }

    /// Fetch and parse a snapshot with light, non-inference requests.
    pub async fn fetch_snapshot(settings: &Settings) -> anyhow::Result<ParsedSnapshot> {
        // NOTE: connection reuse is deliberately disabled.
        //
        // Some llama.cpp builds return a spurious `404 Not Found` when a
        // keep-alive socket is reused (reproduced live: the same client + URL
        // succeeded 5/10 times with pooling and 10/10 with pooling off). Since
        // we poll a handful of tiny endpoints every few seconds, the cost of a
        // fresh connection is negligible, while a flaky 404 would silently blank
        // the speeds and emit a misleading "metrics disabled" diagnostic.
        // NOTE: we always talk to loopback (127.0.0.1 / localhost), so proxies
        // must be bypassed unconditionally. A monitoring widget inherits whatever
        // `HTTP_PROXY`/`HTTPS_PROXY` the parent environment exports, and reqwest
        // honours them even for loopback URLs — which silently turns every poll
        // into a CONNECT attempt against an unrelated proxy (observed live: a
        // sandbox-injected `http_proxy=http://127.0.0.1:60410` made the health
        // check never reach the llama.cpp server). A local monitor must never
        // depend on a proxy being correct, so `.no_proxy()` is explicit.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(settings.request_timeout_ms))
            .pool_max_idle_per_host(0)
            .no_proxy()
            .build()?;

        let base = settings.base_url();
        let openai = settings.openai_base_url();
        let mut snap = ParsedSnapshot::default();

        // 1. Health check (native endpoint, server root).
        let health = client.get(format!("{base}/health")).send().await?;
        if !health.status().is_success() {
            anyhow::bail!("health endpoint returned {}", health.status());
        }

        // 2. Models (OpenAI-compatible route, lives under /v1) — for the model name.
        if let Ok(models) = client.get(format!("{openai}/models")).send().await {
            if let Ok(json) = models.json::<ModelsResponse>().await {
                snap.model = extract_model(&json);
            }
        }

        // 3. Props (native endpoint, server root) — model name, context size, quantization.
        //    Props is the authoritative source for the *loaded* model, so it wins on
        //    any field it actually provides; /v1/models only fills the gaps.
        if let Ok(props) = client.get(format!("{base}/props")).send().await {
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
        match client.get(format!("{base}/metrics")).send().await {
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
        if let Ok(resp) = client.get(format!("{base}/slots")).send().await {
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
