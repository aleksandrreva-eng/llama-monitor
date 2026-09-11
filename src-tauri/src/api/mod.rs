//! Tauri command handlers (the API surface the Svelte UI calls via `tauri::invoke`).

pub mod adapters;
pub mod calculator;
pub mod types;

use std::sync::Mutex;

use tauri::{AppHandle, Manager, State, Theme};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::api::types::MonitoringState;
use crate::config::{Settings, WindowPos, WindowSize};
use crate::AppState;

use log::{info, warn};

/// Current cached monitoring state (pushed by the monitoring service).
static CURRENT_STATE: Mutex<Option<MonitoringState>> = Mutex::new(None);

/// Push a fresh state into the shared cache so `get_state` can return it.
pub fn cache_state(state: MonitoringState) {
    *CURRENT_STATE.lock().unwrap() = Some(state);
}

/// Report a frontend-side error into the Rust log.
///
/// The webview console is not visible from outside the app (WebView2 renders
/// black to capture tools, and there is no devtools in a release build), so any
/// startup failure would otherwise leave no trace. The UI calls this from its
/// global error/unhandledrejection handlers to make such failures diagnosable.
#[tauri::command]
pub fn report_frontend_error(message: String) {
    log::error!("frontend error: {message}");
}

/// Get current settings (with secrets masked).
#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().public_view()
}

/// Save settings to disk and sync side-effects (autostart, hotkey).
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    incoming: Settings,
) -> Result<(), String> {
    let autorun = incoming.autorun;
    let verbose = incoming.verbose_logging;
    let old_hotkey = state.settings.lock().unwrap().hotkey.clone();
    let new_hotkey = incoming.hotkey.clone();

    // `get_settings` returns a `public_view` where every api key is masked as
    // "***". If a profile/setting comes back still masked, keep the real secret
    // instead of overwriting it with the placeholder. Only an explicit,
    // non-"***" value replaces the stored key; an empty string clears it.
    let existing = state.settings.lock().unwrap().clone();
    let mut settings = incoming;
    if settings.api_key.as_deref() == Some("***") {
        settings.api_key = existing.api_key.clone();
    }
    for p in settings.servers.iter_mut() {
        if let Some(old_p) = existing.servers.iter().find(|x| x.id == p.id) {
            if p.api_key.as_deref() == Some("***") {
                p.api_key = old_p.api_key.clone();
            }
        }
    }

    {
        let mut guard = state.settings.lock().unwrap();
        *guard = settings;
        guard
            .save()
            .map_err(|e| format!("failed to save settings: {e}"))?;
    }
    sync_autostart(&app, autorun);
    sync_hotkey(&app, old_hotkey, new_hotkey);
    crate::logging::set_verbose(verbose);
    Ok(())
}

/// Reset settings to defaults.
#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<AppState>) -> Settings {
    let old_hotkey = state.settings.lock().unwrap().hotkey.clone();
    let s = Settings::reset();
    let autorun = s.autorun;
    let new_hotkey = s.hotkey.clone();
    {
        let mut guard = state.settings.lock().unwrap();
        *guard = s.clone();
    }
    sync_autostart(&app, autorun);
    sync_hotkey(&app, old_hotkey, new_hotkey);
    crate::logging::set_verbose(s.verbose_logging);
    s
}

/// Persist the window position (logical pixels).
#[tauri::command]
pub fn save_position(state: State<AppState>, x: f64, y: f64) -> Result<(), String> {
    let mut guard = state.settings.lock().unwrap();
    guard.position = Some(WindowPos { x, y });
    guard.save().map_err(|e| format!("failed to save position: {e}"))
}

/// Persist the window size (logical pixels).
#[tauri::command]
pub fn save_size(state: State<AppState>, w: f64, h: f64) -> Result<(), String> {
    let mut guard = state.settings.lock().unwrap();
    guard.size = Some(WindowSize { w, h });
    guard.save().map_err(|e| format!("failed to save size: {e}"))
}

/// Apply runtime window options (always-on-top, opacity, theme) live.
#[tauri::command]
pub fn set_options(
    app: AppHandle,
    always_on_top: Option<bool>,
    theme: Option<String>,
    compact: Option<bool>,
    opacity: Option<f64>,
) {
    if let Some(win) = app.get_webview_window("main") {
        if let Some(v) = always_on_top {
            let _ = win.set_always_on_top(v);
        }
        let theme_opt = theme.as_deref().map(|t| match t {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        });
        let _ = win.set_theme(theme_opt.flatten());
    }
    if let Some(v) = always_on_top {
        info!("set_options: always_on_top={v}");
    }
    if let Some(v) = theme {
        info!("set_options: theme={v}");
    }
    if let Some(v) = compact {
        info!("set_options: compact={v}");
    }
    if let Some(v) = opacity {
        info!("set_options: opacity={v}");
    }
}

/// Apply persisted window options at startup (always-on-top, opacity, theme).
pub fn apply_window_options(app: &AppHandle, settings: &Settings) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_always_on_top(settings.always_on_top);
        let theme_opt = match settings.theme.as_str() {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        };
        let _ = win.set_theme(theme_opt);
    }
}

/// Enable or disable the Windows autostart registration to match `enable`.
pub fn sync_autostart(app: &AppHandle, enable: bool) {
    let manager = app.autolaunch();
    let res = if enable { manager.enable() } else { manager.disable() };
    match res {
        Ok(()) => {}
        Err(e) => {
            // Disabling when no entry exists yet is expected (nothing to remove).
            if enable {
                warn!("autostart enable failed: {e}");
            }
        }
    }
}

/// Register/unregister the global hotkey to match `new` (disabling when None/empty).
pub fn sync_hotkey(app: &AppHandle, old: Option<String>, new: Option<String>) {
    let new = new.filter(|s| !s.is_empty());
    if old == new {
        return;
    }
    let mgr = app.global_shortcut();
    if let Some(o) = old {
        let _ = mgr.unregister(o.as_str());
    }
    if let Some(n) = new {
        if let Err(e) = mgr.register(n.as_str()) {
            warn!("hotkey register failed: {e}");
        }
    }
}

/// Truncate the local log file and return the last `limit` lines of it.
#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let path = crate::logging::log_path();
    std::fs::write(&path, b"").map_err(|e| format!("failed to clear logs ({path:?}): {e}"))
}

/// Read the tail of the local log file (for the diagnostics view).
#[tauri::command]
pub fn read_logs(limit: usize) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(crate::logging::log_path()).unwrap_or_default();
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].to_vec())
}

/// Current connection status string for diagnostics.
#[tauri::command]
pub fn get_logs() -> Result<String, String> {
    let status = CURRENT_STATE
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.connection.label().to_string())
        .unwrap_or_else(|| "Нет данных".to_string());
    Ok(status)
}
