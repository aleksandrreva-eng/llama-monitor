# Wire contract — JS ↔ Rust

Everything the Svelte frontend and the Rust backend exchange: Tauri command
names and argument keys, the event stream, and the persisted settings file.

This document exists because of one bug that took a while to find: the 📌
always-on-top button did nothing until restart, with no error anywhere.
`#[tauri::command]` rewrites parameter names to **camelCase** by default
(`ArgumentCase::Camel` in `tauri-macros`), so a Rust parameter named
`always_on_top` was looked up as `alwaysOnTop` in the JS payload. A mismatch on
an `Option<T>` is **silent** — the argument simply arrives as `None`.

---

## 1. The casing rule

> **Any command with a multi-word parameter carries
> `rename_all = "snake_case"`, and the JS call site sends the snake_case key.**

Single-word parameters (`settings`, `theme`, `x`, `y`, `limit`, `message`) are
unaffected by the conversion and need no attribute.

There are no exceptions. `scan_lan` used to be one — its parameter was spelled
`timeoutMs` in Rust (with `#[allow(non_snake_case)]`) so that the default
camelCase conversion would match the JS key. That worked, but it made "which
convention does this command use?" a per-command question, which is how the
`set_options` bug stayed hidden.

The rule is enforced by review, not by a test: Tauri argument decoding happens
inside the generated command wrapper, so a unit test cannot see it. When adding
a command, check this table and `src-tauri/src/api/mod.rs`.

---

## 2. Commands

| Command | Rust parameters | JS payload keys | `rename_all` |
|---|---|---|---|
| `get_settings` | — | — | — |
| `save_settings` | `settings: Settings` | `settings` | — |
| `reset_settings` | — | — | — |
| `set_options` | `always_on_top: Option<bool>`, `theme: Option<String>` | `always_on_top`, `theme` | `snake_case` |
| `save_position` | `x: f64`, `y: f64` | `x`, `y` | — |
| `save_size` | `w: f64`, `h: f64` | `w`, `h` | — |
| `read_logs` | `limit: usize` | `limit` | — |
| `clear_logs` | — | — | — |
| `report_frontend_error` | `message: String` | `message` | — |
| `scan_lan` | `timeout_ms: Option<u64>` | `timeout_ms` | `snake_case` |

Call sites live in `src/tauriApi.js` (the only module that calls `invoke`, apart
from `main.js` reporting startup failures).

---

## 3. Events

| Event | Direction | Payload |
|---|---|---|
| `monitoring:update` | Rust → JS | `MonitoringState` (see §4) |

`monitoring:update` is the only event. The tray does **not** emit anything: show
/ hide is handled entirely inside `main.rs`, so there is no `tray:event` channel
and nothing in the frontend listens for one.

---

## 4. `MonitoringState` payload

Serialized with `#[serde(rename_all = "camelCase")]` from
`src-tauri/src/api/types.rs`. The frontend store (`src/store.js`, `EMPTY_STATE`)
declares exactly this shape, and `mergeState()` fills in any missing key so a
partial payload cannot blank the widget.

```jsonc
{
  "connection": "connected",          // snake_case enum: connected | connecting |
                                      // disconnected | error | stale |
                                      // server_unavailable | metrics_unavailable
  "lastUpdate": 1759761234567,        // Unix ms, null before the first success
  "serverLabel": "Локальный llama.cpp",
  "context": {
    "total": 32768, "used": 1500, "remaining": 31268,
    "percent": 0.0458,                // null when `used` is unknown
    "available": true
  },
  "prefillSpeed": {
    "current": 412.5,
    "avg30s": 398.1,                  // note: `avg_30s` → `avg30s`, not `avg30S`
    "available": true,
    "splitAvailable": true
  },
  "generationSpeed": { "current": 41.2, "avg30s": 40.8, "available": true, "splitAvailable": true },
  "model": {
    "name": "Ornith-1.5-35B-A3B", "contextSize": 32768, "loaded": true,
    "quantization": "Q4_K_M", "path": "/models/…gguf", "version": "b10929"
  },
  "otherMetrics": [ { "name": "prompt_tokens_total", "value": 25.0 } ],
  "diagnostics": [ { "code": "metrics_disabled" }, { "code": "poll_failed", "detail": "…" } ]
}
```

Guarded by `monitoring_state_wire_format_matches_the_frontend_contract` in
`types.rs`. If that test is red, the widget will render `undefined` in a
template and blank out.

### Diagnostics are codes, not prose

`diagnostics` entries are `{ code, detail? }`. The frontend builds its i18n key
as `diag_<code>` (`src/lib/diagnostics.js`), so the block is localized like the
rest of the widget. `detail` carries only values that cannot be translated — an
OS or network error string — and is appended verbatim.

`code` values are the `DiagCode::code()` strings in `types.rs`.
`scripts/check-i18n.mjs` fails the build if a code has no `diag_<code>` entry in
either locale.

---

## 5. Settings file

`%LOCALAPPDATA%\llama-monitor\llama-monitor-settings.json`, written atomically
(temp file + `rename`). Field names are **snake_case** — the file is
`serde_json` output of `Settings`, not a Tauri command payload, so the
`rename_all` question does not apply here. The frontend reads the same object
back from `get_settings` and passes it to `save_settings` unchanged.

Legacy single-server fields (`server_host`, `server_port`, `base_path`,
`api_key`, `server_label`) are read once during migration and are marked
`#[serde(skip_serializing)]`: they never appear in the file or in the payload
the UI receives, so the frontend cannot mistake a dead field for a live setting.

API keys are masked as `"***"` by `public_view()` before they reach the UI.
`save_settings` treats an incoming `"***"` as "unchanged" rather than as a new
key.
