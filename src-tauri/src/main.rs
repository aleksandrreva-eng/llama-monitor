//! llama-monitor — floating Windows 11 widget for llama.cpp server monitoring.
//!
//! Architecture:
//!   UI (Svelte) -> State -> Monitoring Service -> Data Adapters -> Calculator.

// Windows: hide the console window in release builds. Without this the
// packaged app opens a black terminal alongside the widget. It must stay
// attached in debug builds so `cargo tauri dev` keeps showing logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The `diagnostics` field of `MonitoringState` is intentionally never read on
// the Rust side — it is serialized and consumed by the Svelte frontend.
#![allow(dead_code)]

mod api;
mod config;
mod logging;
mod monitoring;

use std::sync::Mutex;

use config::Settings;
use monitoring::MonitoringService;
use tauri::menu::{Menu, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindow, WindowEvent, Wry};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::Builder as GlobalShortcutBuilder;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

/// Global application state.
pub struct AppState {
    pub settings: Mutex<Settings>,
}

fn main() -> anyhow::Result<()> {
    let settings = Settings::load();
    logging::init_file_logger(settings.verbose_logging);

    let app_state = AppState {
        settings: Mutex::new(settings.clone()),
    };

    // Global hotkey toggles the widget window (show/hide). The actual shortcut
    // is registered at runtime from settings (so it's user-configurable).
    let shortcut_plugin = GlobalShortcutBuilder::new()
        .with_handler(|app, _shortcut, _event| {
            if let Some(win) = app.get_webview_window("main") {
                let visible = win.is_visible().unwrap_or(false);
                if visible {
                    let _ = win.hide();
                } else {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
        })
        .build();

    tauri::Builder::default()
        // Only one widget at a time. A second launch would otherwise create a
        // duplicate window and fight over the global hotkey ("HotKey already
        // registered"). Instead it focuses the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::default(),
            None,
        ))
        .plugin(shortcut_plugin)
        .manage(app_state)
        .setup(move |app| {
            // Start the background monitoring service. It reads the live
            // settings (and the active server profile) from AppState every
            // poll, so switching servers / editing settings needs no restart.
            let service = std::sync::Arc::new(MonitoringService::new());
            service.spawn(app.app_handle().clone());

            // Create the main window.
            let window = WebviewWindow::builder(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .resizable(true)
            .min_inner_size(420.0, 280.0)
            .inner_size(420.0, 460.0)
            .decorations(false)
            .shadow(true)
            .always_on_top(settings.always_on_top)
            .build()
            .expect("failed to build window");

            // Restore window position/size if present.
            if let Some(ref pos) = settings.position {
                let _ = window.set_position(tauri::LogicalPosition::new(pos.x, pos.y));
            }
            if let Some(ref size) = settings.size {
                let _ = window.set_size(tauri::LogicalSize::new(size.w, size.h));
            }

            // Apply persisted window options (opacity, theme, always-on-top)
            // and sync the Windows autostart registration.
            api::apply_window_options(app.app_handle(), &settings);
            api::sync_autostart(app.app_handle(), settings.autorun);
            log::info!(
                "llama-monitor started; server={}:{}{} hotkey={:?}",
                settings.server_host,
                settings.server_port,
                settings.base_path,
                settings.hotkey
            );

            // Register the configured global hotkey (if any).
            if let Some(h) = settings.hotkey.clone().filter(|s| !s.is_empty()) {
                let mgr = app.global_shortcut();
                if let Err(e) = mgr.register(h.as_str()) {
                    // Another app (or a stale process) may already own this
                    // combination; that is not fatal — the widget still works.
                    log::warn!("hotkey '{h}' not registered: {e}");
                }
            }

            // Build the system-tray menu with Show/Hide, status, and Quit.
            let show_item = MenuItemBuilder::with_id("show", "Показать")
                .enabled(true)
                .build(app)?;
            let hide_item = MenuItemBuilder::with_id("hide", "Свернуть в трей")
                .enabled(true)
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Выйти")
                .enabled(true)
                .build(app)?;

            let items: [&dyn tauri::menu::IsMenuItem<_>; 3] =
                [&show_item, &hide_item, &quit_item];
            let menu = Menu::with_items(app, &items)?;

            let tray = TrayIconBuilder::with_id("llama-monitor-tray")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("llama.cpp Monitor")
                .build(app)?;

            // Route tray clicks to the frontend so it can toggle the window.
            {
                let _ = tray.on_tray_icon_event(|tray, event| {
                    let handle = tray.app_handle();
                    let _ = handle.emit("tray:event", format!("{:?}", event));
                });
            }
            {
                let _ = tray.on_menu_event(|app, event| {
                    let id = event.id().0.clone();
                    if id == "quit" {
                        app.exit(0);
                    } else {
                        let _ = app.emit("tray:event", id);
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::get_settings,
            api::save_settings,
            api::reset_settings,
            api::set_options,
            api::save_position,
            api::save_size,
            api::get_logs,
            api::read_logs,
            api::clear_logs,
            api::report_frontend_error,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app: &AppHandle<Wry>, event: RunEvent| {
            // Intercept the window close request: hide to tray instead of
            // quitting when the user enabled "minimize to tray on close".
            if let RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } = event
            {
                if label == "main" {
                    let minimize =
                        app.state::<AppState>().settings.lock().unwrap().minimize_to_tray;
                    if minimize {
                        api.prevent_close();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                }
            }
        });

    Ok(())
}
