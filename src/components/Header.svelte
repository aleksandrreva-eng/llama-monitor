<script>
  import { state, ui, settings } from "../store";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { saveSettings } from "../tauriApi";

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
    return p ? (p.label || p.url) : null;
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
    ui.update((u) => ({ ...u, alwaysOnTop: !u.alwaysOnTop }));
    saveSettings({ always_on_top: !$ui.alwaysOnTop });
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
      console.warn("close failed:", err);
    }
  }

  let dragging = false;
  async function onDragStart(event) {
    if (event.target.closest("button")) return;
    dragging = true;
    try {
      await getCurrentWindow().startDragging();
    } catch (err) {
      console.warn("drag failed:", err);
    }
  }

  function onDragEnd() {
    dragging = false;
  }

  $: themeIcon = $ui?.theme === "light" ? "🌙" : "☀️";
  $: themeTitle = $ui?.theme === "light" ? "Тема: светлая" : "Тема: тёмная";
  $: modeIcon = expanded ? "▤" : "▭";
  $: modeTitle = expanded ? "Развёрнутый режим" : "Компактный режим";
  $: pinned = $ui?.alwaysOnTop;
</script>

<div class="header" role="button" aria-label="drag area"
     on:pointerdown={onDragStart} on:pointerup={onDragEnd}>
  <div class="status-dot {statusClass}"></div>
  <div class="header-label">{label}</div>

  <div class="header-actions">
    <button class="icon-btn mode" title={themeTitle} on:click={toggleTheme}>{themeIcon}</button>
    <button class="icon-btn mode" title={modeTitle} on:click={onToggleExpanded}>{modeIcon}</button>

    <div class="header-divider"></div>

    <button class="icon-btn" class:pinned title="Поверх окон" on:click={toggleAlwaysOnTop}>📌</button>
    <button class="icon-btn" title="В трей" on:click={onHideToTray}>─</button>
    <button class="icon-btn" title="Закрыть" aria-label="Закрыть" on:click={closeWindow}>✕</button>
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
  .header:active { cursor: grabbing; }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    box-shadow: 0 0 0 0 rgba(22, 163, 74, .5);
    animation: pulse 2s infinite;
  }
  .status-dot.connected { background: #16A34A; box-shadow: 0 0 0 0 rgba(22, 163, 74, .5); }
  .status-dot.connecting { background: #9d5bd0; box-shadow: 0 0 0 0 rgba(157, 91, 208, .5); animation: pulse-purple 2s infinite; }
  .status-dot.disconnected { background: #d13438; box-shadow: 0 0 0 0 rgba(209, 52, 56, .5); animation: pulse-red 2s infinite; }
  .status-dot.error { background: #d13438; box-shadow: 0 0 0 0 rgba(209, 52, 56, .5); animation: pulse-red 2s infinite; }
  .status-dot.stale { background: #8d5b00; box-shadow: 0 0 0 0 rgba(141, 91, 0, .5); animation: pulse-amber 2s infinite; }
  .status-dot.unavailable { background: #a1a1a1; box-shadow: 0 0 0 0 rgba(161, 161, 161, .5); animation: pulse-gray 2s infinite; }

  @keyframes pulse {
    0%   { box-shadow: 0 0 0 0 rgba(22, 163, 74, .4); }
    70%  { box-shadow: 0 0 0 6px rgba(22, 163, 74, 0); }
    100% { box-shadow: 0 0 0 0 rgba(22, 163, 74, 0); }
  }
  @keyframes pulse-purple {
    0%   { box-shadow: 0 0 0 0 rgba(157, 91, 208, .4); }
    70%  { box-shadow: 0 0 0 6px rgba(157, 91, 208, 0); }
    100% { box-shadow: 0 0 0 0 rgba(157, 91, 208, 0); }
  }
  @keyframes pulse-red {
    0%   { box-shadow: 0 0 0 0 rgba(209, 52, 56, .4); }
    70%  { box-shadow: 0 0 0 6px rgba(209, 52, 56, 0); }
    100% { box-shadow: 0 0 0 0 rgba(209, 52, 56, 0); }
  }
  @keyframes pulse-amber {
    0%   { box-shadow: 0 0 0 0 rgba(141, 91, 0, .4); }
    70%  { box-shadow: 0 0 0 6px rgba(141, 91, 0, 0); }
    100% { box-shadow: 0 0 0 0 rgba(141, 91, 0, 0); }
  }
  @keyframes pulse-gray {
    0%   { box-shadow: 0 0 0 0 rgba(161, 161, 161, .4); }
    70%  { box-shadow: 0 0 0 6px rgba(161, 161, 161, 0); }
    100% { box-shadow: 0 0 0 0 rgba(161, 161, 161, 0); }
  }

  .header-label {
    font-size: 12px;
    color: rgba(255, 255, 255, .85);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  :global(:root[data-theme="light"]) .header-label {
    color: rgba(0, 0, 0, .75);
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
    border-radius: 6px;
    background: transparent;
    border: none;
    cursor: pointer;
    color: rgba(255, 255, 255, .7);
    font-size: 12px;
    padding: 0;
    transition: background .15s, color .15s;
  }
  .icon-btn:hover { background: rgba(255, 255, 255, .1); color: #fff; }
  .icon-btn:active { background: rgba(255, 255, 255, .16); }
  .icon-btn.pinned { background: rgba(0, 120, 212, .25); color: #4FC3F7; }

  :global(:root[data-theme="light"]) .icon-btn {
    color: rgba(0, 0, 0, .55);
  }
  :global(:root[data-theme="light"]) .icon-btn:hover {
    background: rgba(0, 0, 0, .08);
    color: #000;
  }
  :global(:root[data-theme="light"]) .icon-btn.pinned {
    background: rgba(0, 120, 212, .15);
    color: #0078d4;
  }

  .icon-btn.mode {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    font-size: 12px;
    color: rgba(255, 255, 255, .6);
  }
  .icon-btn.mode:hover { color: #fff; }
  :global(:root[data-theme="light"]) .icon-btn.mode {
    color: rgba(0, 0, 0, .5);
  }
  :global(:root[data-theme="light"]) .icon-btn.mode:hover {
    color: #000;
  }

  .header-divider {
    width: 1px;
    height: 16px;
    background: rgba(255, 255, 255, .12);
    margin: 0 4px;
    flex-shrink: 0;
  }
  :global(:root[data-theme="light"]) .header-divider {
    background: rgba(0, 0, 0, .12);
  }
</style>
