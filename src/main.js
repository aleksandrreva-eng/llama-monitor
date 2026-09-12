import { invoke } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import App from "./App.svelte";
import "./app.css";
import { t } from "./i18n";

// Small helper: translate a key via the current locale. Used for pre-mount
// fatal errors and the error banner, where there is no Svelte component context
// (so the `$t` auto-subscription syntax is unavailable).
function tr(key) {
  return get(t)(key);
}

// Report a failure into the Rust log. The webview console is invisible from
// outside the app in a release build, so the log file is the only channel that
// survives for diagnosis.
//
// `invoke` is imported statically on purpose: the report must work even when the
// page is already broken, so it cannot depend on a dynamic import resolving.
function report(message) {
  try {
    invoke("report_frontend_error", { message: String(message) });
  } catch {
    // Reporting must never itself throw.
  }
}

function escapeHtml(text) {
  return text.replace(/[<>&]/g, (c) => ({ "<": "&lt;", ">": "&gt;", "&": "&amp;" }[c]));
}

// Pre-mount failures: the widget never rendered, so the error *is* the UI.
function showFatal(message) {
  const root = document.getElementById("app");
  if (root) {
    root.innerHTML =
      '<div style="padding:16px;font:12px/1.5 Segoe UI,system-ui,sans-serif;color:#d13438">' +
      "<b>" + tr("fatal_title") + "</b><br><br>" +
      escapeHtml(message) +
      "</div>";
  }
}

// Post-mount failures: the widget is alive and useful, so do NOT tear it down.
// A single bad callback (e.g. a throwing window event handler) used to replace
// the whole UI with a red error screen; now it surfaces as a dismissible banner
// on top of the still-working widget.
let bannerEl = null;
function showBanner(message) {
  if (!bannerEl) {
    bannerEl = document.createElement("div");
    bannerEl.title = tr("banner_dismiss");
    bannerEl.style.cssText =
      "position:fixed;left:0;right:0;top:0;z-index:2147483647;" +
      "background:#d13438;color:#fff;font:11px/1.45 Segoe UI,system-ui,sans-serif;" +
      "padding:4px 8px;white-space:pre-wrap;word-break:break-word;cursor:pointer;";
    bannerEl.addEventListener("click", () => {
      bannerEl.style.display = "none";
    });
    document.body.appendChild(bannerEl);
  }
  bannerEl.style.display = "";
  bannerEl.textContent = message;
}

function describe(err, fallback) {
  if (err && err.stack) return err.stack.split("\n").slice(0, 8).join(" | ");
  if (err && err.message) return err.message;
  if (err != null) return String(err);
  return fallback;
}

// A handler that throws on every event it receives (a resize handler fires many
// times while dragging) would otherwise write the same ERROR line dozens of
// times and bury everything else in the log. Report each distinct message once.
let lastReported = null;
function handleFailure(message, kind) {
  const text = `${message}\n(${kind})`;
  if (text !== lastReported) {
    lastReported = text;
    report(text);
  }
  if (mounted) showBanner(text);
  else showFatal(text);
}

let mounted = false;

// Register handlers BEFORE mounting so a synchronous init failure is captured.
window.addEventListener("error", (e) => {
  handleFailure(describe(e.error, e.message), tr("err_event"));
});

window.addEventListener("unhandledrejection", (e) => {
  handleFailure(describe(e.reason, "unhandled rejection"), "unhandledrejection");
});

// No "boot: ..." breadcrumbs here on purpose. They used to be reported through
// `report_frontend_error`, which logs at ERROR level, so every healthy startup
// wrote two fake errors into the log and made real failures harder to spot.
// The liveness signal is already in the log: a mounted UI issues `set_options`
// calls, so their presence means the frontend is alive and their absence means
// it never mounted.
let app = null;
try {
  app = new App({
    target: document.getElementById("app"),
  });
  mounted = true;
} catch (err) {
  showFatal(describe(err, "mount failed"));
  report(describe(err, "mount failed"));
}

export default app;
