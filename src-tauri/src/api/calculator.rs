//! Metrics calculator: turns raw parsed snapshots into a MonitoringState.
//!
//! Owns the sliding-window averaging for speeds and decides the overall
//! connection status from partial data. Pure logic — no I/O.

use crate::api::adapters::ParsedSnapshot;
use crate::api::types::*;

/// Sliding window of recent speed samples (tokens/sec) for smoothing.
#[derive(Debug, Clone, Default)]
pub struct SpeedWindow {
    pub prefill: Vec<f64>,
    pub generation: Vec<f64>,
    pub window_secs: u32,
}

impl SpeedWindow {
    pub fn new(window_secs: u32) -> Self {
        SpeedWindow {
            prefill: Vec::new(),
            generation: Vec::new(),
            window_secs,
        }
    }

    fn average(samples: &[f64]) -> Option<f64> {
        if samples.is_empty() {
            None
        } else {
            Some(samples.iter().sum::<f64>() / samples.len() as f64)
        }
    }
}

/// Push a value into a sliding window, trimming to the window length.
fn push_window(samples: &mut Vec<f64>, value: f64, window_secs: u32) {
    samples.push(value);
    if samples.len() > (window_secs as usize) {
        samples.drain(0..samples.len() - window_secs as usize);
    }
}

/// Result of applying a new snapshot on top of previous state.
#[derive(Debug)]
pub struct CalcResult {
    pub state: MonitoringState,
    pub diagnostics: Vec<String>,
}

/// Apply a fresh snapshot + optional previous window, returning new state.
pub fn calculate(
    snap: &ParsedSnapshot,
    prev: Option<&MonitoringState>,
    window: &mut SpeedWindow,
    stale_secs: u64,
) -> CalcResult {
    let mut diagnostics = Vec::new();

    // Connection status: derive from availability of data.
    let context_ok = snap.context.available;
    let model_ok = snap.model.name.is_some() || snap.model.loaded;
    let any_metric = snap.prefill_speed.available || snap.generation_speed.available;

    let status = if !model_ok && !any_metric {
        ConnectionStatus::MetricsUnavailable
    } else {
        ConnectionStatus::Connected
    };

    // Staleness check against previous update.
    let stale = prev
        .and_then(|p| p.last_update)
        .map(|last| {
            (snap.timestamp_ms - last) as u64 / 1000 > stale_secs
        })
        .unwrap_or(false);

    if !context_ok {
        diagnostics.push("Контекст неизвестен".to_string());
    }
    if !snap.prefill_speed.available {
        diagnostics.push("Скорость prefill недоступна".to_string());
    }
    if !snap.generation_speed.available {
        diagnostics.push("Скорость generation недоступна".to_string());
    }
    if snap.model.name.is_none() {
        diagnostics.push("Модель не определена".to_string());
    }
    if !snap.prefill_speed.split_available && (snap.prefill_speed.available || snap.generation_speed.available) {
        diagnostics.push("Разделение prefill/generation недоступно".to_string());
    }
    if stale {
        diagnostics.push("Данные устарели".to_string());
    }

    // Diagnostics gathered while fetching (e.g. "server idle", missing endpoints).
    diagnostics.extend(snap.diagnostics.iter().cloned());

    // Speed smoothing.
    let mut prefill = snap.prefill_speed.clone();
    let mut generation = snap.generation_speed.clone();

    if let Some(v) = prefill.current {
        push_window(&mut window.prefill, v, window.window_secs);
        prefill.avg_30s = SpeedWindow::average(&window.prefill);
    }
    if let Some(v) = generation.current {
        push_window(&mut window.generation, v, window.window_secs);
        generation.avg_30s = SpeedWindow::average(&window.generation);
    }

    let state = MonitoringState {
        connection: if stale {
            ConnectionStatus::Stale
        } else {
            status
        },
        last_update: Some(snap.timestamp_ms),
        server_label: snap.server_label.clone(),
        context: snap.context.clone(),
        prefill_speed: prefill,
        generation_speed: generation,
        model: snap.model.clone(),
        diagnostics: diagnostics.clone(),
    };

    CalcResult { state, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::adapters::ParsedSnapshot;
    use crate::api::types::SpeedMetric;

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

    #[test]
    fn connected_when_model_present() {
        let snap = snapshot_with_model();
        let mut w = SpeedWindow::new(30);
        let res = calculate(&snap, None, &mut w, 9);
        assert_eq!(res.state.connection, ConnectionStatus::Connected);
    }

    #[test]
    fn metrics_unavailable_when_nothing_present() {
        let snap = ParsedSnapshot {
            timestamp_ms: 1_700_000_000_000,
            ..Default::default()
        };
        let mut w = SpeedWindow::new(30);
        let res = calculate(&snap, None, &mut w, 9);
        assert_eq!(res.state.connection, ConnectionStatus::MetricsUnavailable);
        assert!(res
            .diagnostics
            .iter()
            .any(|d| d.contains("Контекст неизвестен")));
        assert!(res.diagnostics.iter().any(|d| d.contains("Модель не определена")));
    }

    #[test]
    fn stale_when_gap_exceeds_threshold() {
        let snap = snapshot_with_model();
        let mut prev = calculate(&snap, None, &mut SpeedWindow::new(30), 9).state;

        // Second snapshot arrives 30s later with a stale threshold of 9s.
        let mut snap2 = snapshot_with_model();
        snap2.timestamp_ms += 30_000;
        prev.last_update = Some(snap.timestamp_ms);

        let res = calculate(&snap2, Some(&prev), &mut SpeedWindow::new(30), 9);
        assert_eq!(res.state.connection, ConnectionStatus::Stale);
        assert!(res.diagnostics.iter().any(|d| d.contains("устарел")));
    }

    #[test]
    fn speed_window_smooths_values() {
        let mut w = SpeedWindow::new(30);
        let mut snap = snapshot_with_model();
        snap.generation_speed = SpeedMetric {
            current: Some(10.0),
            available: true,
            ..Default::default()
        };
        calculate(&snap, None, &mut w, 9);

        snap.generation_speed.current = Some(20.0);
        let res = calculate(&snap, None, &mut w, 9);
        // Average of [10, 20] = 15.
        assert_eq!(res.state.generation_speed.avg_30s, Some(15.0));
    }

    #[test]
    fn speed_window_trims_to_window_size() {
        let mut w = SpeedWindow::new(2);
        for v in [1.0, 2.0, 3.0] {
            let mut snap = snapshot_with_model();
            snap.generation_speed = SpeedMetric {
                current: Some(v),
                available: true,
                ..Default::default()
            };
            calculate(&snap, None, &mut w, 9);
        }
        // Only the last 2 samples (2.0, 3.0) should remain -> avg 2.5.
        assert_eq!(w.generation.len(), 2);
        assert_eq!(SpeedWindow::average(&w.generation), Some(2.5));
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
        let mut w = SpeedWindow::new(30);
        let res = calculate(&snap, None, &mut w, 9);
        assert!(res
            .diagnostics
            .iter()
            .any(|d| d.contains("Разделение prefill/generation недоступно")));
    }
}
