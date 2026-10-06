<script>
  import { state, ui, settings, pushLog } from "../store";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { saveSettings } from "../tauriApi";
  import { t } from "../i18n";

  export let expanded = false;
  export let onToggleExpanded;
  export let onHideToTray;

  $: s = $state;
  // Fallback label when the backend hasn't yet reported one: derive it from the
  // active server profile (label, else url) instead of the legacy host:port.
  $: activeLabel = (() => {
    const st = $settings;
    if (!st || !st.servers || !st.servers.length) return null;
    const id = st.active_server_id || st.servers[0].id;
    const p = st.servers.find((x) => x.id === id);
    return p ? p.label || p.url : null;
  })();
  $: label = s?.serverLabel || activeLabel || "llama.cpp";

  const STATUS_CLASS = {
    connected: "connected",
    connecting: "connecting",
    disconnected: "disconnected",
    error: "error",
    stale: "stale",
    server_unavailable: "unavailable",
    metrics_unavailable: "unavailable",
  };
  $: statusClass = STATUS_CLASS[s?.connection] || "unavailable";

  function toggleAlwaysOnTop() {
    // Compute the next value BEFORE updating the store: `ui.update` applies
    // synchronously, so reading `$ui.alwaysOnTop` afterwards already yields the
    // new value — and we would persist (and immediately restore) the opposite,
    // making the button a no-op.
    const next = !$ui.alwaysOnTop;
    ui.update((u) => ({ ...u, alwaysOnTop: next }));
    saveSettings({ always_on_top: next });
  }

  function toggleTheme() {
    const next = $ui.theme === "light" ? "dark" : "light";
    ui.update((u) => ({ ...u, theme: next }));
    saveSettings({ theme: next });
  }

  async function closeWindow() {
    try {
      await getCurrentWindow().close();
    } catch (err) {
      pushLog("close failed: " + err);
    }
  }

  let dragging = false;
  async function onDragStart(event) {
    if (event.target.closest("button")) return;
    dragging = true;
    try {
      await getCurrentWindow().startDragging();
    } catch (err) {
      pushLog("drag failed: " + err);
    }
  }

  function onDragEnd() {
    dragging = false;
  }

  $: themeIcon = $ui?.theme === "light" ? "🌙" : "☀️";
  $: themeTitle = $ui?.theme === "light" ? $t("theme_light") : $t("theme_dark");
  $: modeIcon = expanded ? "▤" : "▭";
  $: modeTitle = expanded ? $t("mode_expanded") : $t("mode_compact");
  $: pinned = $ui?.alwaysOnTop;
</script>

<div
  class="header"
  role="button"
  aria-label="drag area"
  on:pointerdown={onDragStart}
  on:pointerup={onDragEnd}
>
  <div class="status-dot {statusClass}"></div>
  <div class="header-label">{label}</div>

  <div class="header-actions">
    <button class="icon-btn mode" title={themeTitle} on:click={toggleTheme}>{themeIcon}</button>
    <button class="icon-btn mode" title={modeTitle} on:click={onToggleExpanded}>{modeIcon}</button>

    <div class="header-divider"></div>

    <button
      class="icon-btn"
      class:pinned
      title={$t("title_always_on_top")}
      on:click={toggleAlwaysOnTop}>📌</button
    >
    <button class="icon-btn" title={$t("title_to_tray")} on:click={onHideToTray}>─</button>
    <button
      class="icon-btn"
      title={$t("title_close")}
      aria-label={$t("title_close")}
      on:click={closeWindow}>✕</button
    >
  </div>
</div>

<style>
  .header {
    display: flex;
    align-items: center;
    height: 36px;
    padding: 0 6px 0 12px;
    gap: 6px;
    cursor: grab;
  }
  .header:active {
    cursor: grabbing;
  }

  /* The dot's `color` is the single source of its colour: the background uses
     it directly and the pulsing ring derives from it via `currentColor`. That
     replaced five near-identical @keyframes blocks whose rgba() rings had to be
     kept in sync with the hex backgrounds by hand. */
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: currentColor;
    animation: pulse 2s infinite;
  }
  .status-dot.connected {
    color: var(--status-connected);
  }
  .status-dot.connecting {
    color: var(--status-connecting);
  }
  .status-dot.disconnected {
    color: var(--status-disconnected);
  }
  .status-dot.error {
    color: var(--status-error);
  }
  .status-dot.stale {
    color: var(--status-stale);
  }
  .status-dot.unavailable {
    color: var(--status-unavailable);
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 color-mix(in srgb, currentColor 40%, transparent);
    }
    70% {
      box-shadow: 0 0 0 6px color-mix(in srgb, currentColor 0%, transparent);
    }
    100% {
      box-shadow: 0 0 0 0 color-mix(in srgb, currentColor 0%, transparent);
    }
  }

  .header-label {
    font-size: 12px;
    color: var(--text-header);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }

  .header-actions {
    display: flex;
    gap: 2px;
    align-items: center;
    flex-shrink: 0;
  }

  .icon-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-icon);
    font-size: 12px;
    padding: 0;
    transition:
      background 0.15s,
      color 0.15s;
  }
  .icon-btn:hover {
    background: var(--inset);
    color: var(--text-icon-hover);
  }
  .icon-btn:active {
    background: var(--inset-strong);
  }
  .icon-btn.pinned {
    background: var(--pin-bg);
    color: var(--pin-fg);
  }

  .icon-btn.mode {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    font-size: 12px;
  }

  .header-divider {
    width: 1px;
    height: 16px;
    background: var(--control-border);
    margin: 0 4px;
    flex-shrink: 0;
  }
</style>
