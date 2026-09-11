import { invoke } from "@tauri-apps/api/core";
import { settings, ui, pushLog, get } from "./store";

export async function loadSettings() {
  try {
    const s = await invoke("get_settings");
    settings.set(s);
    ui.update((u) => ({
      ...u,
      compact: s.default_compact,
      alwaysOnTop: s.always_on_top,
      theme: s.theme,
      opacity: s.window_opacity,
    }));
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
    ui.update((u) => ({
      ...u,
      compact: merged.default_compact,
      alwaysOnTop: merged.always_on_top,
      theme: merged.theme,
      opacity: merged.window_opacity,
    }));
    pushLog("settings saved");
  } catch (err) {
    pushLog("saveSettings error: " + err);
  }
}

export async function resetSettings() {
  try {
    const s = await invoke("reset_settings");
    settings.set(s);
    ui.update((u) => ({
      ...u,
      compact: s.default_compact,
      alwaysOnTop: s.always_on_top,
      theme: s.theme,
      opacity: s.window_opacity,
    }));
    pushLog("settings reset");
  } catch (err) {
    pushLog("resetSettings error: " + err);
  }
}

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
