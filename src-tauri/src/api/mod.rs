//! Tauri command handlers (the API surface the Svelte UI calls via `tauri::invoke`).
//!
//! ## Wire contract
//!
//! `#[tauri::command]` rewrites parameter names to **camelCase** by default
//! (`ArgumentCase::Camel` in `tauri-macros`), so a Rust parameter named
//! `always_on_top` is looked up as `alwaysOnTop` in the JS payload. A mismatch
//! is silent for `Option<T>` — the argument simply arrives as `None` — which is
//! how the always-on-top toggle stayed broken without a single error anywhere.
//!
//! Two ways out, and this module uses the second one everywhere:
//!
//! * **Single-word parameters** (`settings`, `theme`, `x`, `y`, `limit`,
//!   `message`) are unaffected by the conversion and need nothing.
//! * **Multi-word parameters** carry `rename_all = "snake_case"` on the command,
//!   so the JS key equals the Rust name *and* the `Settings` field name. That
//!   keeps the payload identical to what `save_settings` already sends
//!   (`always_on_top`, `poll_interval_ms`, …) instead of mixing two conventions.
//!
//! Rule: **any command with a multi-word parameter gets
//! `rename_all = "snake_case"`**, and the JS call site sends the snake_case key.
//! There are no exceptions left — `scan_lan` used to be one (its parameter was
//! literally spelled `timeoutMs` to satisfy the default conversion), which is
//! exactly the kind of special case that made the original bug invisible.
//!
//! Full table: `docs/wire-contract.md`.

pub mod adapters;
pub mod calculator;
pub mod lan;
pub mod parse;
pub mod probes;
pub mod types;

use tauri::{AppHandle, Manager, State, Theme};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::config::{Settings, WindowPos, WindowSize};
use crate::sync::lock;
use crate::AppState;

use log::{info, warn};

/// How long the hotkey value must stay unchanged before it is registered.
const HOTKEY_DEBOUNCE: Duration = Duration::from_millis(500);

/// Generation counter for the debounced hotkey registration. Each new request
/// bumps it; a scheduled registration only runs if it is still the newest.
static HOTKEY_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Current time as a Unix timestamp in milliseconds.
///
/// `0` on a clock before the epoch, which can only happen if the system clock is
/// badly wrong; every consumer treats a zero timestamp as "unknown".
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
    lock(&state.settings).public_view()
}

/// Save settings to disk and sync side-effects (autostart, hotkey, window options).
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    let autorun = settings.autorun;
    let verbose = settings.verbose_logging;
    let old_hotkey = lock(&state.settings).hotkey.clone();
    let new_hotkey = settings.hotkey.clone();

    // `get_settings` returns a `public_view` where every api key is masked as
    // "***". If a profile/setting comes back still masked, keep the real secret
    // instead of overwriting it with the placeholder. Only an explicit,
    // non-"***" value replaces the stored key; an empty string clears it.
    let existing = lock(&state.settings).clone();
    // Rebind as mutable (Tauri command params must not be declared `mut`).
    let mut settings = settings;
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
    // Free-text inputs reach us mid-typing; clamp before anything reads them.
    settings.sanitize();

    {
        let mut guard = lock(&state.settings);
        *guard = settings;
        guard
            .save()
            .map_err(|e| format!("failed to save settings: {e}"))?;
    }
    // Apply the persisted window options here as well as through `set_options`:
    // the failure mode of the JS path is silent, and "the setting was saved" and
    // "the setting took effect" should be the same event.
    apply_window_options(&app, &lock(&state.settings));
    sync_autostart(&app, autorun);
    sync_hotkey(&app, old_hotkey, new_hotkey);
    crate::logging::set_verbose(verbose);
    Ok(())
}

/// Reset settings to defaults.
#[tauri::command]
pub fn reset_settings(app: AppHandle, state: State<AppState>) -> Settings {
    let old_hotkey = lock(&state.settings).hotkey.clone();
    let s = Settings::reset();
    let autorun = s.autorun;
    let new_hotkey = s.hotkey.clone();
    *lock(&state.settings) = s.clone();
    apply_window_options(&app, &s);
    sync_autostart(&app, autorun);
    sync_hotkey(&app, old_hotkey, new_hotkey);
    crate::logging::set_verbose(s.verbose_logging);
    s
}

/// Persist the window position (logical pixels).
#[tauri::command]
pub fn save_position(state: State<AppState>, x: f64, y: f64) -> Result<(), String> {
    let mut guard = lock(&state.settings);
    guard.position = Some(WindowPos { x, y });
    guard
        .save()
        .map_err(|e| format!("failed to save position: {e}"))
}

/// Persist the window size (logical pixels).
#[tauri::command]
pub fn save_size(state: State<AppState>, w: f64, h: f64) -> Result<(), String> {
    let mut guard = lock(&state.settings);
    guard.size = Some(WindowSize { w, h });
    guard
        .save()
        .map_err(|e| format!("failed to save size: {e}"))
}

/// Apply runtime window options (always-on-top, theme) live.
///
/// Only the two options the *window* actually owns live here. `compact` is pure
/// UI state (it belongs to the Svelte store) and `opacity` is applied as CSS on
/// the widget element — both used to be accepted by this command and then only
/// written to the log, which made it look like they were applied.
///
/// `rename_all = "snake_case"` makes the JS payload keys identical to the Rust
/// parameter names (`always_on_top`), which is also the name used by
/// `save_settings` and by the `Settings` JSON. Without it the macro would look
/// for `alwaysOnTop`, silently deliver `None`, and the 📌 button would be a
/// no-op until restart — the exact defect this attribute fixes.
#[tauri::command(rename_all = "snake_case")]
pub fn set_options(app: AppHandle, always_on_top: Option<bool>, theme: Option<String>) {
    let Some(win) = app.get_webview_window("main") else {
        warn!("set_options: main window is not available");
        return;
    };

    if let Some(v) = always_on_top {
        if let Err(e) = win.set_always_on_top(v) {
            warn!("set_options: set_always_on_top({v}) failed: {e}");
        } else {
            info!("set_options: always_on_top={v}");
        }
    }

    if let Some(t) = theme {
        let theme_opt = match t.as_str() {
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            // "auto" (or anything unrecognised) follows the OS theme.
            _ => None,
        };
        if let Err(e) = win.set_theme(theme_opt) {
            warn!("set_options: set_theme({t}) failed: {e}");
        } else {
            info!("set_options: theme={t}");
        }
    }
}

/// Apply persisted window options at startup (always-on-top, theme).
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
    let res = if enable {
        manager.enable()
    } else {
        manager.disable()
    };
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
///
/// Registration is **debounced**. The settings panel persists on every
/// keystroke, so typing `CmdOrCtrl+Shift+M` used to attempt seventeen
/// registrations — each of the intermediate values (`C`, `Cm`, `Cmd`, …) fails
/// and writes a warning, burying anything useful in the log. Only the value
/// that survives [`HOTKEY_DEBOUNCE`] untouched is actually applied.
pub fn sync_hotkey(app: &AppHandle, old: Option<String>, new: Option<String>) {
    let new = new.filter(|s| !s.is_empty());
    if old == new {
        return;
    }
    // Bump the generation and let a later call supersede this one.
    let generation = HOTKEY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(HOTKEY_DEBOUNCE).await;
        if HOTKEY_GENERATION.load(Ordering::SeqCst) != generation {
            return; // a newer value arrived while we waited
        }
        let mgr = app.global_shortcut();
        if let Some(o) = old {
            // Unregistering a shortcut that was itself superseded and never
            // registered is expected to fail; there is nothing to report.
            let _ = mgr.unregister(o.as_str());
        }
        if let Some(n) = new {
            if let Err(e) = mgr.register(n.as_str()) {
                warn!("hotkey register failed: {e}");
            }
        }
    });
}

/// Truncate the local log file.
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
