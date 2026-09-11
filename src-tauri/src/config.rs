//! Configuration service: load / save / apply settings.
//!
//! Secrets (API key) are stored in the settings file but never logged.
//!
//! Servers are now a list of [`ServerProfile`]s with one `active_server_id`.
//! A legacy single-server config (`server_host`/`server_port`/`base_path`) is
//! migrated into the first `Local` profile on load.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Kind of an upstream server. Drives which adapter `fetch_snapshot` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ServerKind {
    /// llama.cpp (local or remote). Native root endpoints + `/v1`.
    #[default]
    Local,
    /// vLLM OpenAI-compatible server. Prometheus `/metrics` with `vllm:` prefix.
    Vllm,
    /// Ollama. Native `/api/*` endpoints + OpenAI-compatible `/v1`.
    Ollama,
    /// Generic OpenAI-compatible cloud (OpenAI/OpenRouter/Groq/Together). `/v1/models` only.
    Cloud,
}

impl ServerKind {
    /// Human-readable Russian label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            ServerKind::Local => "llama.cpp",
            ServerKind::Vllm => "vLLM",
            ServerKind::Ollama => "Ollama",
            ServerKind::Cloud => "Облако (OpenAI)",
        }
    }
}

/// Serde default for `Settings::servers`: an empty list. Unlike `Settings::default()`
/// (which seeds one profile), this lets a legacy file that lacks `servers` deserialize
/// to an empty list so `migrate()` can build the first profile from old fields.
fn empty_servers() -> Vec<ServerProfile> {
    Vec::new()
}

/// A stored server connection profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerProfile {
    /// Stable id used to reference the profile (active_server_id, removal, …).
    pub id: String,
    /// User-facing name shown in the server list and as the widget label.
    pub label: String,
    /// Base URL without any `/v1` suffix, e.g. `http://127.0.0.1:8080` or
    /// `https://api.openai.com/v1`. The adapter derives native/OpenAI bases from it.
    pub url: String,
    pub kind: ServerKind,
    /// Optional bearer token sent as `Authorization: Bearer <api_key>`.
    pub api_key: Option<String>,
}

impl Default for ServerProfile {
    fn default() -> Self {
        ServerProfile {
            id: "default".to_string(),
            label: "Локальный llama.cpp".to_string(),
            url: "http://127.0.0.1:8080".to_string(),
            kind: ServerKind::Local,
            api_key: None,
        }
    }
}

impl ServerProfile {
    /// Base URL for **native** endpoints (`/health`, `/props`, `/slots`, `/metrics`),
    /// which llama.cpp/vLLM serve from the server root. Ollama keeps them under `/api`.
    pub fn native_base_url(&self) -> String {
        let root = self.url.trim_end_matches('/');
        match self.kind {
            // Ollama's native API lives under /api, not the root.
            ServerKind::Ollama => format!("{root}/api"),
            // Local/vLLM/Cloud: native endpoints (where they exist) are at the root.
            _ => root.to_string(),
        }
    }

    /// Base URL for the **OpenAI-compatible** API (`/v1/models`, …).
    ///
    /// If `url` already ends with `/v1` we use it as-is; otherwise `/v1` is appended.
    pub fn openai_base_url(&self) -> String {
        let root = self.url.trim_end_matches('/');
        if root.ends_with("/v1") {
            root.to_string()
        } else {
            format!("{root}/v1")
        }
    }

    /// True when the URL points at loopback (proxy must be bypassed).
    pub fn is_loopback(&self) -> bool {
        let u = self.url.to_ascii_lowercase();
        u.contains("127.0.0.1")
            || u.contains("localhost")
            || u.contains("[::1]")
            || u.contains("::1")
    }
}

/// Application settings, persisted as JSON next to the binary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // --- legacy single-server fields (migrated into `servers`; kept for load) ---
    pub server_host: String,
    pub server_port: u16,
    pub base_path: String,
    // --- multi-server model ---
    /// Known server profiles (local, remote, cloud).
    /// Serde default is an EMPTY vec (not `Settings::default`'s single profile), so a
    /// legacy file that lacks `servers` deserializes to an empty list and `migrate()`
    /// can turn its old single-server fields into the first profile.
    #[serde(default = "empty_servers")]
    pub servers: Vec<ServerProfile>,
    /// Id of the profile the widget currently monitors. `None` → first profile.
    pub active_server_id: Option<String>,

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
            servers: vec![ServerProfile::default()],
            active_server_id: Some("default".to_string()),
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
    /// The profile the widget should monitor right now.
    pub fn active_profile(&self) -> Option<&ServerProfile> {
        self.active_server_id
            .as_ref()
            .and_then(|id| self.servers.iter().find(|s| s.id == *id))
            .or_else(|| self.servers.first())
    }

    /// Mutable view of the active profile (used when updating a server).
    pub fn active_profile_mut(&mut self) -> Option<&mut ServerProfile> {
        let id = self
            .active_server_id
            .clone()
            .or_else(|| self.servers.first().map(|s| s.id.clone()));
        id.and_then(move |id| self.servers.iter_mut().find(|s| s.id == id))
    }

    /// Base URL for **native** llama.cpp endpoints of the active server.
    pub fn base_url(&self) -> String {
        self.active_profile()
            .map(|p| p.native_base_url())
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string())
    }

    /// Base URL for the **OpenAI-compatible** API of the active server.
    pub fn openai_base_url(&self) -> String {
        self.active_profile()
            .map(|p| p.openai_base_url())
            .unwrap_or_else(|| "http://127.0.0.1:8080/v1".to_string())
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
    fn migrate(&mut self) -> bool {
        let mut changed = false;

        // Migration 0.1.0 -> 0.1.1: the original default `base_path` was `/v1`, which
        // is wrong for llama.cpp's native endpoints. Files still holding the untouched
        // default `/v1` are rewritten to the empty root.
        if self.base_path == "/v1" {
            log::info!("migrating settings: base_path '/v1' -> '' (native endpoints live at the root)");
            self.base_path = String::new();
            changed = true;
        }

        // Multi-server migration: a legacy file has no `servers` array. Build the
        // first `Local` profile from the old single-server fields and drop them.
        if self.servers.is_empty() {
            let url = format!(
                "http://{}:{}{}",
                if self.server_host.is_empty() {
                    "127.0.0.1"
                } else {
                    self.server_host.as_str()
                },
                self.server_port,
                self.base_path.trim_end_matches('/')
            );
            let label = self
                .server_label
                .clone()
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| "Локальный llama.cpp".to_string());
            log::info!("migrating settings: legacy server {url} -> ServerProfile(default)");
            self.servers.push(ServerProfile {
                id: "default".to_string(),
                label,
                url,
                kind: ServerKind::Local,
                api_key: self.api_key.clone(),
            });
            self.active_server_id = Some("default".to_string());
            // Clear legacy fields so they don't shadow the profile.
            self.server_host = String::new();
            self.server_port = 0;
            self.base_path = String::new();
            changed = true;
        }

        changed
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
        s.api_key = s.api_key.as_ref().map(|_| "***".to_string());
        for p in &mut s.servers {
            p.api_key = p.api_key.as_ref().map(|_| "***".to_string());
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with_url(url: &str, kind: ServerKind) -> ServerProfile {
        ServerProfile {
            id: "x".to_string(),
            label: "t".to_string(),
            url: url.to_string(),
            kind,
            api_key: None,
        }
    }

    #[test]
    fn default_has_one_local_profile() {
        let s = Settings::default();
        assert_eq!(s.servers.len(), 1);
        assert_eq!(s.active_server_id.as_deref(), Some("default"));
        assert_eq!(s.active_profile().unwrap().kind, ServerKind::Local);
    }

    #[test]
    fn default_base_url_is_local_root() {
        let s = Settings::default();
        assert_eq!(s.base_url(), "http://127.0.0.1:8080");
        assert_eq!(s.openai_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn local_native_base_has_no_v1() {
        let p = profile_with_url("http://127.0.0.1:8080", ServerKind::Local);
        assert_eq!(p.native_base_url(), "http://127.0.0.1:8080");
        assert_eq!(p.openai_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn openai_base_does_not_double_v1() {
        let p = profile_with_url("http://127.0.0.1:8080/v1", ServerKind::Local);
        assert_eq!(p.openai_base_url(), "http://127.0.0.1:8080/v1");
        assert_eq!(p.native_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn trailing_slashes_trimmed() {
        let p = profile_with_url("http://127.0.0.1:8080/v1/", ServerKind::Local);
        assert_eq!(p.openai_base_url(), "http://127.0.0.1:8080/v1");
        assert_eq!(p.native_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn custom_prefix_gets_v1_appended() {
        let p = profile_with_url("http://127.0.0.1:8080/llama", ServerKind::Local);
        assert_eq!(p.openai_base_url(), "http://127.0.0.1:8080/llama/v1");
        assert_eq!(p.native_base_url(), "http://127.0.0.1:8080/llama");
    }

    #[test]
    fn ollama_native_base_uses_api_path() {
        let p = profile_with_url("http://127.0.0.1:11434", ServerKind::Ollama);
        assert_eq!(p.native_base_url(), "http://127.0.0.1:11434/api");
        assert_eq!(p.openai_base_url(), "http://127.0.0.1:11434/v1");
    }

    #[test]
    fn cloud_base_keeps_v1() {
        let p = profile_with_url("https://api.openrouter.ai/v1", ServerKind::Cloud);
        assert_eq!(p.openai_base_url(), "https://api.openrouter.ai/v1");
        assert_eq!(p.native_base_url(), "https://api.openrouter.ai/v1");
    }

    #[test]
    fn active_profile_falls_back_to_first() {
        let mut s = Settings::default();
        s.active_server_id = None;
        assert_eq!(s.active_profile().unwrap().id, "default");
    }

    /// An older settings file (before `servers` existed) must load and migrate
    /// its legacy single-server fields into the first profile.
    #[test]
    fn legacy_file_migrates_to_profile() {
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
            "api_key": "sekret",
            "server_label": "Workstation",
            "warning_threshold": 0.75,
            "critical_threshold": 0.9,
            "smoothing_window_secs": 30,
            "window_opacity": 1.0,
            "position": null,
            "size": null
        }"#;
        let mut s: Settings = serde_json::from_str(legacy).expect("legacy file must parse");
        assert!(s.migrate());
        assert_eq!(s.servers.len(), 1);
        let p = &s.servers[0];
        assert_eq!(p.url, "http://192.168.1.50:9000");
        assert_eq!(p.kind, ServerKind::Local);
        assert_eq!(p.label, "Workstation");
        assert_eq!(p.api_key.as_deref(), Some("sekret"));
        assert_eq!(s.active_server_id.as_deref(), Some("default"));
        // Legacy fields cleared after migration.
        assert_eq!(s.server_host, "");
    }

    #[test]
    fn migrate_leaves_existing_servers_alone() {
        let mut s = Settings::default();
        s.servers.push(profile_with_url("http://example:1234", ServerKind::Vllm));
        assert!(!s.migrate());
        assert_eq!(s.servers.len(), 2);
    }
}
