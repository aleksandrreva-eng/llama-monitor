import { invoke } from "@tauri-apps/api/core";
import { settings, ui, pushLog, get } from "./store";
import { syncLocaleFromSettings } from "./i18n";

/// Mirror the persisted window options into the UI store. `default_compact` is
/// NOT part of this: it seeds the widget's initial expanded state in App.svelte
/// (writing it to `ui.compact` — which nothing read — was the reason the setting
/// looked functional while doing nothing).
function syncUi(s) {
  ui.update((u) => ({
    ...u,
    alwaysOnTop: s.always_on_top,
    theme: s.theme,
    opacity: s.window_opacity,
  }));
}

export async function loadSettings() {
  try {
    const s = await invoke("get_settings");
    settings.set(s);
    syncUi(s);
    syncLocaleFromSettings(s.language);
  } catch (err) {
    pushLog("loadSettings error: " + err);
  }
}

export async function saveSettings(patch) {
  try {
    const current = get(settings);
    const merged = { ...current, ...patch };
    await invoke("save_settings", { settings: merged });
    settings.set(merged);
    syncUi(merged);
    pushLog("settings saved");
  } catch (err) {
    pushLog("saveSettings error: " + err);
  }
}

export async function resetSettings() {
  try {
    const s = await invoke("reset_settings");
    settings.set(s);
    syncUi(s);
    syncLocaleFromSettings(s.language);
    pushLog("settings reset");
  } catch (err) {
    pushLog("resetSettings error: " + err);
  }
}

/// Apply runtime window options. Keys are snake_case because the command
/// carries `rename_all = "snake_case"` — see the wire contract in
/// `src-tauri/src/api/mod.rs` and `docs/wire-contract.md`.
export async function setOptions(patch) {
  try {
    await invoke("set_options", patch);
  } catch (err) {
    pushLog("setOptions error: " + err);
  }
}

export async function clearLogs() {
  try {
    await invoke("clear_logs");
  } catch (err) {
    pushLog("clearLogs error: " + err);
  }
}

export async function readLogs(limit = 300) {
  try {
    return await invoke("read_logs", { limit });
  } catch (err) {
    pushLog("readLogs error: " + err);
    return [];
  }
}

export async function savePosition(x, y) {
  try {
    await invoke("save_position", { x, y });
  } catch (err) {
    pushLog("savePosition error: " + err);
  }
}

export async function saveSize(w, h) {
  try {
    await invoke("save_size", { w, h });
  } catch (err) {
    pushLog("saveSize error: " + err);
  }
}

/// Scan the LAN for inference servers and return discovered profiles.
/// `timeout_ms` is snake_case because `scan_lan` carries
/// `rename_all = "snake_case"` — see `docs/wire-contract.md`.
export async function scanLan(timeoutMs = 9000) {
  try {
    return await invoke("scan_lan", { timeout_ms: timeoutMs });
  } catch (err) {
    pushLog("scanLan error: " + err);
    return [];
  }
}
