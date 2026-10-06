//! Metrics calculator: turns a parsed snapshot into a [`MonitoringState`].
//!
//! Owns the sliding-window averaging for speeds and decides the overall
//! connection status from partial data. Pure logic — no I/O, no clock.

use crate::api::adapters::ParsedSnapshot;
use crate::api::types::{ConnectionStatus, Diag, DiagCode, MonitoringState};

/// Smallest poll interval the window math will assume (matches the loop's own
/// floor), so a misconfigured interval cannot produce a zero-sized window.
const MIN_INTERVAL_MS: u64 = 250;

/// Hard cap on the number of retained samples, so a tiny interval combined with
/// a long smoothing window cannot grow the buffer without bound.
const MAX_SAMPLES: usize = 600;

/// Sliding window of recent speed samples (tokens/sec) used for smoothing.
///
/// The capacity is a **sample count derived from the configured window in
/// seconds and the poll interval** — the previous implementation used the
/// configured seconds directly as a sample count, so a "30 s average" at a 3 s
/// poll interval actually averaged 90 seconds.
#[derive(Debug, Clone, Default)]
pub struct SpeedWindow {
    pub prefill: Vec<f64>,
    pub generation: Vec<f64>,
    /// Number of samples to keep.
    pub capacity: usize,
}

impl SpeedWindow {
    /// A window covering `window_secs` of wall-clock time at `poll_interval_ms`.
    pub fn for_interval(window_secs: u32, poll_interval_ms: u64) -> Self {
        let per_sample_ms = poll_interval_ms.max(MIN_INTERVAL_MS);
        let capacity = ((window_secs as u64).saturating_mul(1000) / per_sample_ms)
            .clamp(1, MAX_SAMPLES as u64) as usize;
        SpeedWindow {
            prefill: Vec::new(),
            generation: Vec::new(),
            capacity,
        }
    }

    /// Append a prefill sample, trimming to the window.
    pub fn push_prefill(&mut self, value: f64) {
        push_window(&mut self.prefill, value, self.capacity);
    }

    /// Append a generation sample, trimming to the window.
    pub fn push_generation(&mut self, value: f64) {
        push_window(&mut self.generation, value, self.capacity);
    }

    /// Mean of the retained prefill samples.
    pub fn average_prefill(&self) -> Option<f64> {
        average(&self.prefill)
    }

    /// Mean of the retained generation samples.
    pub fn average_generation(&self) -> Option<f64> {
        average(&self.generation)
    }
}

/// Push a value into a sliding window, trimming to `capacity` samples.
fn push_window(samples: &mut Vec<f64>, value: f64, capacity: usize) {
    let capacity = capacity.max(1);
    samples.push(value);
    if samples.len() > capacity {
        samples.drain(0..samples.len() - capacity);
    }
}

fn average(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        None
    } else {
        Some(samples.iter().sum::<f64>() / samples.len() as f64)
    }
}

/// Connection status for a **failed** poll.
///
/// While the last good reading is still recent the server simply "went away",
/// and `ServerUnavailable` is the actionable message. Once the data still on
/// screen is older than the freshness threshold the honest message changes: what
/// the user is looking at is out of date. The real cause always travels in the
/// `poll_failed` diagnostic.
///
/// This is the only place `Stale` can be produced. Deriving it inside
/// `calculate` (as the previous implementation did) made it unreachable: that
/// function only ever runs on a *successful* poll, where the data is by
/// definition fresh.
pub fn degraded_status(last_update: Option<i64>, now_ms: i64, stale_secs: u64) -> ConnectionStatus {
    match last_update {
        Some(last) if (now_ms - last).max(0) as u64 / 1000 > stale_secs => ConnectionStatus::Stale,
        _ => ConnectionStatus::ServerUnavailable,
    }
}

/// Freshness threshold: three poll intervals, at least one second.
///
/// Integer-dividing the interval first (the old `(ms / 1000) * 3`) collapsed to
/// zero for any interval below 1000 ms, marking every reading as stale.
pub fn stale_threshold_secs(poll_interval_ms: u64) -> u64 {
    (poll_interval_ms.saturating_mul(3) / 1000).max(1)
}

/// Apply a fresh snapshot to the smoothing window, returning the new state.
pub fn calculate(snap: &ParsedSnapshot, window: &mut SpeedWindow) -> MonitoringState {
    let mut diagnostics: Vec<DiagCode> = Vec::new();

    // Connection status: derive from availability of data.
    let model_ok = snap.model.name.is_some() || snap.model.loaded;
    let any_metric = snap.prefill_speed.available || snap.generation_speed.available;

    let connection = if !model_ok && !any_metric {
        ConnectionStatus::MetricsUnavailable
    } else {
        ConnectionStatus::Connected
    };

    if !snap.context.available {
        diagnostics.push(DiagCode::ContextUnknown);
    }
    if !snap.prefill_speed.available {
        diagnostics.push(DiagCode::PrefillUnavailable);
    }
    if !snap.generation_speed.available {
        diagnostics.push(DiagCode::GenerationUnavailable);
    }
    if snap.model.name.is_none() {
        diagnostics.push(DiagCode::ModelUnknown);
    }
    if !snap.prefill_speed.split_available
        && (snap.prefill_speed.available || snap.generation_speed.available)
    {
        diagnostics.push(DiagCode::SplitUnavailable);
    }

    // Diagnostics gathered while fetching (e.g. "server idle", missing endpoints).
    for code in &snap.diagnostics {
        if !diagnostics.contains(code) {
            diagnostics.push(code.clone());
        }
    }

    // Speed smoothing.
    let mut prefill = snap.prefill_speed.clone();
    let mut generation = snap.generation_speed.clone();

    if let Some(v) = prefill.current {
        window.push_prefill(v);
        prefill.avg_30s = window.average_prefill();
    }
    if let Some(v) = generation.current {
        window.push_generation(v);
        generation.avg_30s = window.average_generation();
    }

    MonitoringState {
        connection,
        last_update: Some(snap.timestamp_ms),
        server_label: snap.server_label.clone(),
        context: snap.context.clone(),
        prefill_speed: prefill,
        generation_speed: generation,
        model: snap.model.clone(),
        other_metrics: snap.other_metrics.clone(),
        diagnostics: diagnostics.iter().map(Diag::from).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ModelMetric, SpeedMetric};

    fn snapshot_with_model() -> ParsedSnapshot {
        ParsedSnapshot {
            model: ModelMetric {
                name: Some("test-model".to_string()),
                loaded: true,
                ..Default::default()
            },
            timestamp_ms: 1_700_000_000_000,
            ..Default::default()
        }
    }

    fn codes(state: &MonitoringState) -> Vec<String> {
        state.diagnostics.iter().map(|d| d.code.clone()).collect()
    }

    #[test]
    fn connected_when_model_present() {
        let snap = snapshot_with_model();
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        assert_eq!(state.connection, ConnectionStatus::Connected);
    }

    #[test]
    fn metrics_unavailable_when_nothing_present() {
        let snap = ParsedSnapshot {
            timestamp_ms: 1_700_000_000_000,
            ..Default::default()
        };
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        assert_eq!(state.connection, ConnectionStatus::MetricsUnavailable);

        let codes = codes(&state);
        assert!(codes.iter().any(|c| c == "context_unknown"));
        assert!(codes.iter().any(|c| c == "model_unknown"));
    }

    /// The UI builds its i18n key from the code, so the payload must carry
    /// codes and never prose.
    #[test]
    fn diagnostics_are_codes_not_prose() {
        let snap = snapshot_with_model();
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        for d in &state.diagnostics {
            assert!(
                d.code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "`{}` is not a translatable code",
                d.code
            );
        }
    }

    /// Diagnostics from the adapters and from the calculator must not duplicate.
    #[test]
    fn diagnostics_are_deduplicated() {
        let mut snap = snapshot_with_model();
        snap.diagnostics = vec![DiagCode::ContextUnknown, DiagCode::ServerIdle];
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        let codes = codes(&state);
        assert_eq!(codes.iter().filter(|c| *c == "context_unknown").count(), 1);
        assert!(codes.iter().any(|c| c == "server_idle"));
    }

    #[test]
    fn speed_window_smooths_values() {
        let mut w = SpeedWindow::for_interval(30, 3000);
        let mut snap = snapshot_with_model();
        snap.generation_speed = SpeedMetric {
            current: Some(10.0),
            available: true,
            ..Default::default()
        };
        calculate(&snap, &mut w);

        snap.generation_speed.current = Some(20.0);
        let state = calculate(&snap, &mut w);
        // Average of [10, 20] = 15.
        assert_eq!(state.generation_speed.avg_30s, Some(15.0));
    }

    #[test]
    fn speed_window_trims_to_window_size() {
        let mut w = SpeedWindow::for_interval(1, 1000);
        assert_eq!(w.capacity, 1);
        for v in [1.0, 2.0, 3.0] {
            let mut snap = snapshot_with_model();
            snap.generation_speed = SpeedMetric {
                current: Some(v),
                available: true,
                ..Default::default()
            };
            calculate(&snap, &mut w);
        }
        // Only the last sample survives.
        assert_eq!(w.generation.len(), 1);
        assert_eq!(w.average_generation(), Some(3.0));
    }

    /// The window must cover the configured number of *seconds*, not that many
    /// samples: 30 s at a 3 s interval is 10 samples, not 30.
    #[test]
    fn window_capacity_is_derived_from_seconds_and_interval() {
        assert_eq!(SpeedWindow::for_interval(30, 3000).capacity, 10);
        assert_eq!(SpeedWindow::for_interval(30, 1000).capacity, 30);
        assert_eq!(SpeedWindow::for_interval(5, 1000).capacity, 5);
        // Sub-second intervals are floored, never zero.
        assert_eq!(SpeedWindow::for_interval(1, 100).capacity, 4);
        // Absurd combinations are capped instead of growing without bound.
        assert_eq!(
            SpeedWindow::for_interval(u32::MAX, 250).capacity,
            MAX_SAMPLES
        );
        // A zero interval cannot produce a zero-sized window.
        assert!(SpeedWindow::for_interval(30, 0).capacity >= 1);
    }

    #[test]
    fn split_unavailable_adds_diagnostic() {
        let mut snap = snapshot_with_model();
        snap.prefill_speed = SpeedMetric {
            current: Some(100.0),
            available: true,
            split_available: false,
            ..Default::default()
        };
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        assert!(codes(&state).iter().any(|c| c == "split_unavailable"));
    }

    /// A failed poll must not wipe what is already on screen.
    #[test]
    fn calculate_never_returns_stale_and_keeps_the_timestamp() {
        let snap = snapshot_with_model();
        let mut w = SpeedWindow::for_interval(30, 3000);
        let state = calculate(&snap, &mut w);
        // `calculate` runs on a successful poll, so the data is fresh by
        // definition; `Stale` belongs to the failure path only.
        assert_ne!(state.connection, ConnectionStatus::Stale);
        assert_eq!(state.last_update, Some(snap.timestamp_ms));
    }

    /// `Stale` is produced only by the failed-poll path, and only once the data
    /// on screen has actually aged out.
    #[test]
    fn degraded_status_escalates_to_stale_once_data_ages() {
        let now = 1_700_000_000_000;
        let stale_secs = 9;

        // Fresh data, one failed poll: the server went away.
        assert_eq!(
            degraded_status(Some(now - 3_000), now, stale_secs),
            ConnectionStatus::ServerUnavailable
        );
        // Data older than the threshold: what is on screen is out of date.
        assert_eq!(
            degraded_status(Some(now - 30_000), now, stale_secs),
            ConnectionStatus::Stale
        );
        // Exactly at the threshold is still "just went away".
        assert_eq!(
            degraded_status(Some(now - 9_000), now, stale_secs),
            ConnectionStatus::ServerUnavailable
        );
        // No previous reading at all: nothing to be stale.
        assert_eq!(
            degraded_status(None, now, stale_secs),
            ConnectionStatus::ServerUnavailable
        );
        // A clock that jumped backwards must not panic or mark stale.
        assert_eq!(
            degraded_status(Some(now + 60_000), now, stale_secs),
            ConnectionStatus::ServerUnavailable
        );
    }

    /// A poll interval below one second used to yield a zero-second threshold,
    /// which marked every reading as stale.
    #[test]
    fn stale_threshold_never_collapses_to_zero() {
        assert_eq!(stale_threshold_secs(3000), 9);
        assert_eq!(stale_threshold_secs(1000), 3);
        assert_eq!(stale_threshold_secs(500), 1);
        assert_eq!(stale_threshold_secs(0), 1);
    }
}
