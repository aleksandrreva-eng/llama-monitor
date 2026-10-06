import { writable, get } from "svelte/store";

export { get };
import { listen } from "@tauri-apps/api/event";

// The declared shape below IS the contract between the UI and the Rust side.
// Every component dereferences these fields directly (`pf.available`,
// `m.contextSize`, …), so a missing key used to throw
// "Cannot read properties of undefined" inside a template and blank the whole
// widget. `mergeState` guarantees the UI always sees a complete object, even if
// the backend payload is partial or a field is renamed again.
const EMPTY_STATE = {
  connection: "disconnected",
  lastUpdate: null,
  serverLabel: null,
  context: { total: null, used: null, remaining: null, percent: null, available: false },
  prefillSpeed: { current: null, avg30s: null, available: false, splitAvailable: false },
  generationSpeed: { current: null, avg30s: null, available: false, splitAvailable: false },
  model: {
    name: null,
    contextSize: null,
    loaded: false,
    quantization: null,
    path: null,
    version: null,
  },
  otherMetrics: [],
  // Each entry is `{ code, detail? }` — a translatable code, not prose.
  diagnostics: [],
};

export const state = writable(EMPTY_STATE);

/// Merge a backend payload over EMPTY_STATE, one level deep for the nested
/// metric objects, so no key the UI reads can ever be `undefined`.
export function mergeState(payload) {
  if (!payload || typeof payload !== "object") return get(state);
  const nested = (key) => ({ ...EMPTY_STATE[key], ...(payload[key] || {}) });
  return {
    ...EMPTY_STATE,
    ...payload,
    context: nested("context"),
    prefillSpeed: nested("prefillSpeed"),
    generationSpeed: nested("generationSpeed"),
    model: nested("model"),
    otherMetrics: Array.isArray(payload.otherMetrics) ? payload.otherMetrics : [],
    diagnostics: Array.isArray(payload.diagnostics) ? payload.diagnostics : [],
  };
}

export const settings = writable(null);
// Live window options. `compact` is deliberately absent: the expanded/compact
// toggle is component state seeded from `Settings.default_compact`, and a
// `ui.compact` field nothing read only made that setting look wired up.
export const ui = writable({ alwaysOnTop: false, theme: "auto", opacity: 1.0 });

let logs = [];
export const logStore = writable([]);

export function pushLog(msg) {
  const ts = new Date().toLocaleTimeString();
  logs = [...logs.slice(-199), `[${ts}] ${msg}`];
  logStore.set(logs);
}

/// Subscribe to the backend's state stream.
///
/// The tray has no event channel: show/hide is handled entirely in Rust
/// (`main.rs`), which is what made the previous `listen("tray:event")` handler
/// dead code — the event was never emitted, so nothing it did could ever run.
export async function initTauri() {
  try {
    await listen("monitoring:update", (e) => {
      state.set(mergeState(e.payload));
    });
  } catch (err) {
    pushLog("listen error: " + err);
  }
}
