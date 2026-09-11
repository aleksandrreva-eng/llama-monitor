//! Configuration service: load / save / apply settings.
//!
//! Secrets (API key) are stored in the settings file but never logged.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application settings, persisted as JSON next to the binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub server_host: String,
    pub server_port: u16,
    pub base_path: String,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub theme: String, // "auto" | "light" | "dark"
    pub default_compact: bool,
    pub autorun: bool,
    pub minimize_to_tray: bool,
    pub always_on_top: bool,
    pub show_last_update: bool,
    pub verbose_logging: bool,
    // optional
    pub api_key: Option<String>,
    pub server_label: Option<String>,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
    pub smoothing_window_secs: u32,
    pub window_opacity: f64,
    /// Global hotkey to toggle the window (e.g. "CmdOrCtrl+Shift+M").
    /// `None` or empty string disables the hotkey.
    pub hotkey: Option<String>,
    /// Persisted window position (in logical pixels).
    pub position: Option<WindowPos>,
    /// Persisted window size (in logical pixels).
    pub size: Option<WindowSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowPos {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowSize {
    pub w: f64,
    pub h: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            server_host: "127.0.0.1".to_string(),
            server_port: 8080,
            // Native llama.cpp endpoints (/health, /props, /slots, /metrics) live at the
            // server root. The OpenAI-compatible API is mounted under /v1 separately.
            base_path: String::new(),
            poll_interval_ms: 3000,
            request_timeout_ms: 4000,
            theme: "auto".to_string(),
            default_compact: true,
            autorun: false,
            minimize_to_tray: true,
            always_on_top: false,
            show_last_update: true,
            verbose_logging: false,
            api_key: None,
            server_label: None,
            warning_threshold: 0.75,
            critical_threshold: 0.9,
            smoothing_window_secs: 30,
            window_opacity: 1.0,
            hotkey: Some("CmdOrCtrl+Shift+M".to_string()),
            position: None,
            size: None,
        }
    }
}

impl Settings {
    /// Base URL for **native** llama.cpp endpoints (`/health`, `/props`, `/slots`, `/metrics`).
    /// These are served from the server root, so `base_path` is normally empty.
    pub fn base_url(&self) -> String {
        let path = self.base_path.trim_end_matches('/');
        format!("http://{}:{}{}", self.server_host, self.server_port, path)
    }

    /// Base URL for the **OpenAI-compatible** API (`/v1/models`, `/v1/chat/completions`).
    ///
    /// If the configured `base_path` already ends with `/v1` we use it as-is;
    /// otherwise `/v1` is appended (so a root base_path still reaches the OpenAI routes).
    pub fn openai_base_url(&self) -> String {
        let path = self.base_path.trim_end_matches('/');
        if path.ends_with("/v1") {
            format!("http://{}:{}{}", self.server_host, self.server_port, path)
        } else {
            format!("http://{}:{}{}/v1", self.server_host, self.server_port, path)
        }
    }

    /// Path to the settings file (next to the executable).
    pub fn settings_path() -> PathBuf {
        if let Some(exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            exe.join("llama-monitor-settings.json")
        } else {
            PathBuf::from("llama-monitor-settings.json")
        }
    }

    /// Load settings, creating defaults if the file is missing.
    pub fn load() -> Settings {
        let path = Self::settings_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => {
                let mut s: Settings = serde_json::from_str(&json).unwrap_or_else(|e| {
                    log::warn!("settings parse error, using defaults: {e}");
                    Settings::default()
                });
                if s.migrate() {
                    let _ = s.save();
                }
                s
            }
            Err(_) => {
                let s = Settings::default();
                let _ = s.save();
                s
            }
        }
    }

    /// Bring an older settings file up to date. Returns `true` if anything changed
    /// (and therefore should be persisted).
    ///
    /// Migration 0.1.0 -> 0.1.1: the original default `base_path` was `/v1`, which
    /// is wrong for llama.cpp's native endpoints (`/health`, `/props`, `/slots`,
    /// `/metrics` are served from the root). Files still holding the untouched
    /// default `/v1` are rewritten to the empty root. A non-default custom path
    /// (e.g. `/llama`) is preserved, since `openai_base_url()` appends `/v1` itself.
    fn migrate(&mut self) -> bool {
        if self.base_path == "/v1" {
            log::info!("migrating settings: base_path '/v1' -> '' (native endpoints live at the root)");
            self.base_path = String::new();
            return true;
        }
        false
    }

    /// Persist settings to disk.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_path();
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Reset to defaults and persist.
    pub fn reset() -> Settings {
        let s = Settings::default();
        let _ = s.save();
        s
    }

    /// Returns a copy suitable for sending to the UI, with secrets masked.
    pub fn public_view(&self) -> Settings {
        let mut s = self.clone();
        s.api_key = if self.api_key.is_some() {
            Some("***".to_string())
        } else {
            None
        };
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with_base(base: &str) -> Settings {
        Settings {
            server_host: "127.0.0.1".to_string(),
            server_port: 8080,
            base_path: base.to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn default_base_path_is_server_root() {
        let s = Settings::default();
        assert_eq!(s.base_path, "", "native llama.cpp endpoints live at the root");
        assert_eq!(s.base_url(), "http://127.0.0.1:8080");
    }

    #[test]
    fn native_base_url_has_no_v1() {
        let s = settings_with_base("");
        assert_eq!(s.base_url(), "http://127.0.0.1:8080");
        // The OpenAI route is derived separately and always gains /v1.
        assert_eq!(s.openai_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn openai_base_url_does_not_double_v1() {
        let s = settings_with_base("/v1");
        assert_eq!(s.openai_base_url(), "http://127.0.0.1:8080/v1");
        assert_eq!(s.base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn trailing_slashes_are_trimmed() {
        let s = settings_with_base("/v1/");
        assert_eq!(s.openai_base_url(), "http://127.0.0.1:8080/v1");
        assert_eq!(s.base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn custom_prefix_gets_v1_appended() {
        let s = settings_with_base("/llama");
        assert_eq!(s.openai_base_url(), "http://127.0.0.1:8080/llama/v1");
        assert_eq!(s.base_url(), "http://127.0.0.1:8080/llama");
    }

    /// An older settings file (before `hotkey` existed) must load without
    /// discarding the user's other values.
    #[test]
    fn old_settings_file_without_hotkey_still_loads() {
        let legacy = r#"{
            "server_host": "192.168.1.50",
            "server_port": 9000,
            "base_path": "/v1",
            "poll_interval_ms": 1500,
            "request_timeout_ms": 4000,
            "theme": "dark",
            "default_compact": false,
            "autorun": false,
            "minimize_to_tray": true,
            "always_on_top": true,
            "show_last_update": true,
            "verbose_logging": false,
            "api_key": null,
            "server_label": "Workstation",
            "warning_threshold": 0.75,
            "critical_threshold": 0.9,
            "smoothing_window_secs": 30,
            "window_opacity": 1.0,
            "position": null,
            "size": null
        }"#;
        let mut s: Settings = serde_json::from_str(legacy).expect("legacy file must parse");
        assert_eq!(s.server_host, "192.168.1.50");
        assert_eq!(s.server_port, 9000);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.server_label.as_deref(), Some("Workstation"));
        // The new field falls back to its default rather than failing.
        assert_eq!(s.hotkey.as_deref(), Some("CmdOrCtrl+Shift+M"));

        // Migration rewrites the stale default base_path and reports the change.
        assert!(s.migrate());
        assert_eq!(s.base_path, "");
    }

    #[test]
    fn migrate_leaves_custom_base_path_alone() {
        let mut s = settings_with_base("/llama");
        assert!(!s.migrate());
        assert_eq!(s.base_path, "/llama");
    }
}

