//! Monitoring service: background polling with adaptive cadence and smoothing.
//!
//! Runs on a dedicated background task so it never blocks the UI thread. On
//! errors it degrades state gracefully — keeping the last known values on
//! screen — and slows the poll cadence instead of hammering a dead server.
//!
//! The loop reads the *live* settings on every tick, so a server switch, a
//! changed interval or a new smoothing window takes effect without a restart.

use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::api::adapters::{adapter_for, ParsedSnapshot};
use crate::api::calculator::{calculate, degraded_status, stale_threshold_secs, SpeedWindow};
use crate::api::probes::{build_client, run_probes};
use crate::api::types::{ConnectionStatus, Diag, DiagCode, MonitoringState};
use crate::config::{ServerKind, Settings};
use crate::sync::{lock, read, write};

/// Floor for the poll interval: below this a "monitor" would itself become load.
const MIN_INTERVAL_MS: u64 = 250;

/// Each consecutive failure multiplies the delay by this factor…
const BACKOFF_FACTOR: u32 = 2;

/// …up to this many steps. Capped at 4× the configured interval (12 s at the
/// 3 s default) so a recovered server is noticed within a few seconds: a widget
/// that takes a minute to come back reads as broken.
const MAX_BACKOFF_STEPS: u32 = 2;

/// Background polling service (shared, behind Arc for the tokio task).
pub struct MonitoringService {
    /// Sliding window of recent speed samples — reset when the active server
    /// changes so we never average speeds from two different upstreams.
    window: Mutex<SpeedWindow>,
    /// Last successfully built state. Used to keep showing the last known
    /// values when a poll fails, and to age them for the `stale` status.
    prev_state: RwLock<Option<MonitoringState>>,
    /// The active server id we polled last; lets us detect a switch.
    last_server_id: Mutex<Option<String>>,
    /// Consecutive failed polls, driving the backoff.
    failures: Mutex<u32>,
}

impl Default for MonitoringService {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitoringService {
    pub fn new() -> Self {
        MonitoringService {
            window: Mutex::new(SpeedWindow::for_interval(30, 3000)),
            prev_state: RwLock::new(None),
            last_server_id: Mutex::new(None),
            failures: Mutex::new(0),
        }
    }

    /// Clone into an Arc and spawn the polling loop.
    pub fn spawn(self: Arc<Self>, app_handle: AppHandle) {
        tauri::async_runtime::spawn(async move {
            Self::run_loop(app_handle, self).await;
        });
    }

    async fn run_loop(app_handle: AppHandle, state: Arc<MonitoringService>) {
        // Poll once immediately for a fast first frame.
        Self::poll_and_emit(&app_handle, &state).await;

        loop {
            // The delay is recomputed every tick from the *current* settings, so
            // changing the poll interval in the UI applies from the next tick.
            // A `tokio::time::interval` seeded at startup (the previous design)
            // ignored the change until the app was restarted.
            let settings = current_settings(&app_handle);
            let delay = backoff_delay(settings.poll_interval_ms, *lock(&state.failures));
            tokio::time::sleep(delay).await;
            Self::poll_and_emit(&app_handle, &state).await;
        }
    }

    async fn poll_and_emit(app_handle: &AppHandle, state: &Arc<MonitoringService>) {
        let settings = current_settings(app_handle);

        // Detect an active-server change and reset smoothing/previous state so
        // we don't blend speeds or context from two different upstreams.
        let active_id = settings
            .active_server_id
            .clone()
            .or_else(|| settings.servers.first().map(|p| p.id.clone()));
        {
            let mut last = lock(&state.last_server_id);
            if *last != active_id {
                *lock(&state.window) = SpeedWindow::for_interval(
                    settings.smoothing_window_secs,
                    settings.poll_interval_ms,
                );
                *write(&state.prev_state) = None;
                *lock(&state.failures) = 0;
                *last = active_id;
            }
        }

        match Self::fetch_snapshot(&settings).await {
            Ok(snap) => {
                *lock(&state.failures) = 0;
                let next = {
                    let mut window = lock(&state.window);
                    calculate(&snap, &mut window)
                };
                *write(&state.prev_state) = Some(next.clone());
                let _ = app_handle.emit("monitoring:update", next);
            }
            Err(err) => {
                *lock(&state.failures) += 1;
                let degraded = Self::degraded_state(&settings, state, err);
                let _ = app_handle.emit("monitoring:update", degraded);
            }
        }
    }

    /// Build the state shown while the server cannot be reached.
    ///
    /// The last known context, speeds and model are **kept**: a single failed
    /// poll used to blank the whole widget (the old code built this state from
    /// `..Default::default()`), so a one-poll network blip made a working server
    /// look like it had no model and no context at all. Only the connection
    /// status and the diagnostics change; the footer's relative timestamp shows
    /// how old the values are.
    fn degraded_state(
        settings: &Settings,
        state: &MonitoringService,
        err: anyhow::Error,
    ) -> MonitoringState {
        let previous = read(&state.prev_state).clone();
        let now = crate::api::now_ms();
        let stale_secs = stale_threshold_secs(settings.poll_interval_ms);
        let connection = degraded_status(
            previous.as_ref().and_then(|s| s.last_update),
            now,
            stale_secs,
        );

        let mut degraded = previous.unwrap_or_default();
        degraded.connection = connection;
        if degraded.server_label.is_none() {
            degraded.server_label = Some(profile_label(settings));
        }
        degraded.diagnostics = vec![Diag::from(DiagCode::PollFailed(err.to_string()))];
        if connection == ConnectionStatus::Stale {
            degraded.diagnostics.push(Diag::from(DiagCode::Stale));
        }
        degraded
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

        let adapter = adapter_for(kind);
        let probes = adapter.probes(&settings.base_url(), &settings.openai_base_url());
        let client = build_client(settings)?;
        let report = run_probes(&client, &probes, api_key.as_deref()).await?;

        let mut snap = adapter.parse(&report);
        // Stamped here rather than inside `parse` so the parsers stay pure.
        snap.timestamp_ms = crate::api::now_ms();
        Ok(snap)
    }
}

/// Read the live settings out of the shared app state.
fn current_settings(app_handle: &AppHandle) -> Settings {
    lock(&app_handle.state::<crate::AppState>().settings).clone()
}

/// Human-readable label for the active profile (its name, else its URL).
fn profile_label(settings: &Settings) -> String {
    match settings.active_profile() {
        Some(p) if !p.label.is_empty() => p.label.clone(),
        Some(p) => p.url.clone(),
        None => "llama.cpp".to_string(),
    }
}

/// Delay before the next poll.
///
/// `interval_ms` is the user's configured interval and `failures` the number of
/// consecutive failed polls. Doubling per failure up to [`MAX_BACKOFF_STEPS`]
/// keeps a dead server from being polled at full rate while still noticing
/// recovery quickly. The first success resets `failures` to zero, so the loop
/// returns to the configured cadence immediately.
fn backoff_delay(interval_ms: u64, failures: u32) -> Duration {
    let interval = interval_ms.max(MIN_INTERVAL_MS);
    let steps = failures.min(MAX_BACKOFF_STEPS);
    let factor = BACKOFF_FACTOR.saturating_pow(steps) as u64;
    Duration::from_millis(interval.saturating_mul(factor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ContextMetric, ModelMetric};

    #[test]
    fn no_backoff_while_healthy() {
        assert_eq!(backoff_delay(3000, 0), Duration::from_millis(3000));
    }

    #[test]
    fn delay_doubles_per_failure_and_is_capped() {
        assert_eq!(backoff_delay(3000, 1), Duration::from_millis(6000));
        assert_eq!(backoff_delay(3000, 2), Duration::from_millis(12000));
        // Capped: a long outage must not push the delay to minutes, or recovery
        // would take that long to notice.
        assert_eq!(backoff_delay(3000, 3), Duration::from_millis(12000));
        assert_eq!(backoff_delay(3000, 99), Duration::from_millis(12000));
    }

    /// A misconfigured interval cannot make the loop spin.
    #[test]
    fn interval_has_a_floor() {
        assert_eq!(backoff_delay(0, 0), Duration::from_millis(MIN_INTERVAL_MS));
        assert_eq!(backoff_delay(10, 0), Duration::from_millis(MIN_INTERVAL_MS));
    }

    /// An absurd interval must not overflow into a zero-length delay.
    #[test]
    fn absurd_interval_saturates_instead_of_overflowing() {
        let d = backoff_delay(u64::MAX, 2);
        assert!(d > Duration::from_secs(1));
    }

    /// The degraded state keeps the last known readings: a single failed poll
    /// must not blank a working server's model and context.
    #[test]
    fn degraded_state_keeps_last_known_values() {
        let settings = Settings::default();
        let state = MonitoringService::new();
        let previous = MonitoringState {
            connection: ConnectionStatus::Connected,
            last_update: Some(crate::api::now_ms()),
            server_label: Some("lab".to_string()),
            context: ContextMetric {
                total: Some(8192),
                used: Some(2048),
                remaining: Some(6144),
                percent: Some(0.25),
                available: true,
            },
            model: ModelMetric {
                name: Some("Ornith".to_string()),
                loaded: true,
                ..Default::default()
            },
            ..Default::default()
        };
        *write(&state.prev_state) = Some(previous.clone());

        let degraded =
            MonitoringService::degraded_state(&settings, &state, anyhow::anyhow!("refused"));

        assert_eq!(degraded.connection, ConnectionStatus::ServerUnavailable);
        assert_eq!(degraded.context.total, Some(8192));
        assert_eq!(degraded.context.used, Some(2048));
        assert_eq!(degraded.model.name.as_deref(), Some("Ornith"));
        assert_eq!(degraded.server_label.as_deref(), Some("lab"));
        assert_eq!(degraded.last_update, previous.last_update);
        assert_eq!(degraded.diagnostics.len(), 1);
        assert_eq!(degraded.diagnostics[0].code, "poll_failed");
        assert_eq!(degraded.diagnostics[0].detail.as_deref(), Some("refused"));
    }

    /// With nothing ever received there is nothing to keep — but the label and
    /// the cause must still reach the UI.
    #[test]
    fn degraded_state_without_history_reports_unavailable() {
        let settings = Settings::default();
        let state = MonitoringService::new();

        let degraded =
            MonitoringService::degraded_state(&settings, &state, anyhow::anyhow!("refused"));

        assert_eq!(degraded.connection, ConnectionStatus::ServerUnavailable);
        assert!(!degraded.context.available);
        assert_eq!(degraded.last_update, None);
        assert_eq!(
            degraded.server_label.as_deref(),
            Some("Локальный llama.cpp")
        );
        assert_eq!(degraded.diagnostics[0].code, "poll_failed");
    }

    /// Once the kept data is older than the threshold, the status escalates to
    /// `stale` and says so in the diagnostics.
    #[test]
    fn degraded_state_escalates_to_stale() {
        let settings = Settings::default();
        let state = MonitoringService::new();
        *write(&state.prev_state) = Some(MonitoringState {
            last_update: Some(crate::api::now_ms() - 60_000),
            ..Default::default()
        });

        let degraded =
            MonitoringService::degraded_state(&settings, &state, anyhow::anyhow!("refused"));

        assert_eq!(degraded.connection, ConnectionStatus::Stale);
        let codes: Vec<&str> = degraded
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(codes.contains(&"stale"));
        assert!(codes.contains(&"poll_failed"));
    }

    #[test]
    fn profile_label_prefers_the_name_then_the_url() {
        let mut settings = Settings::default();
        assert_eq!(profile_label(&settings), "Локальный llama.cpp");
        settings.servers[0].label = String::new();
        assert_eq!(profile_label(&settings), "http://127.0.0.1:8080");
    }
}
