<script>
  import { onMount, tick } from "svelte";
  import Header from "./components/Header.svelte";
  import ContextBar from "./components/ContextBar.svelte";
  import SpeedBlock from "./components/SpeedBlock.svelte";
  import ModelBlock from "./components/ModelBlock.svelte";
  import Footer from "./components/Footer.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import { state, ui, settings, pushLog, initTauri, onTrayEvent } from "./store";
  import { loadSettings, savePosition, saveSize } from "./tauriApi";

  let expanded = false;
  let showSettings = false;
  let showLogsTab = false;
  let geoTimer = null;
  let widgetEl = null;

  onMount(async () => {
    await initTauri();
    await loadSettings();
    pushLog("app started");
    fitWindowToContent();

    // Persist window geometry (debounced) so position/size survive restarts.
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      win.onMoved(({ payload }) => {
        if (typeof payload?.x !== "number" || typeof payload?.y !== "number") return;
        scheduleGeoSave(() => savePosition(payload.x, payload.y));
      });
      win.onResized(({ payload }) => {
        const w = payload?.width ?? payload?.size?.width;
        const h = payload?.height ?? payload?.size?.height;
        if (typeof w !== "number" || typeof h !== "number") return;
        scheduleGeoSave(() => saveSize(w, h));
      });
    } catch (err) {
      console.warn("geometry listeners failed:", err);
    }
  });

  function scheduleGeoSave(fn) {
    if (geoTimer) clearTimeout(geoTimer);
    geoTimer = setTimeout(fn, 400);
  }

  // Last content height actually applied to the window. Lets us skip redundant
  // resizes AND leave a manual height resize alone until the content changes.
  let lastFitHeight = 0;

  // Fit the OS window to the widget's natural content height (keeps the
  // current width so manual horizontal resizes are preserved). The widget is
  // height:100% of the window WITH overflow:hidden, so its scrollHeight just
  // echoes the window height — not the content height. Temporarily release the
  // fixed height (-> auto) so the element grows to its natural size, measure,
  // then restore BEFORE the async IPC so the flip stays inside one frame.
  async function fitWindowToContent() {
    if (!widgetEl) return;
    try {
      const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
      await tick();
      const win = getCurrentWindow();
      const prevHeight = widgetEl.style.height;
      widgetEl.style.height = "auto";
      await tick();
      const h = Math.round(widgetEl.scrollHeight);
      widgetEl.style.height = prevHeight; // "" -> CSS height:100%
      if (Math.abs(h - lastFitHeight) < 2) return; // content unchanged, nothing to do
      lastFitHeight = h;
      const scale = await win.scaleFactor();
      const cur = await win.innerSize(); // physical pixels
      const logicalWidth = cur.width / scale;
      await win.setSize(new LogicalSize(logicalWidth, h));
    } catch (err) {
      console.warn("fit to content failed:", err);
    }
  }

  // Debounced refit. The content height is NOT constant: in compact mode the
  // speed block grows the moment telemetry arrives (the tok/s unit and the
  // 30 s average appear, and a "split unavailable" note may too). Fitting only
  // on mount/toggle would therefore leave the bottom clipped until the user
  // toggles. Refit whenever the monitoring state updates; the guard inside
  // fitWindowToContent() keeps this from thrashing the window.
  let fitTimer = null;
  function scheduleFit() {
    if (fitTimer) clearTimeout(fitTimer);
    fitTimer = setTimeout(fitWindowToContent, 150);
  }

  // Explicitly re-fit when the user toggles compact <-> expanded. Driving this
  // from the handler (after tick()) is reliable, unlike a reactive statement
  // that also depends on the transient render state.
  async function toggleExpanded() {
    expanded = !expanded;
    await tick();
    fitWindowToContent();
  }

  // Refit on every telemetry update (subscribe fires immediately too, so this
  // also covers the initial fit).
  onMount(() => state.subscribe(() => scheduleFit()));

  $: if ($ui) {
    applyOptions($ui);
  }

  $: if ($ui && $ui.theme) {
    document.documentElement.setAttribute("data-theme", $ui.theme);
  }

  async function applyOptions(u) {
    const { setOptions } = await import("./tauriApi");
    await setOptions({
      always_on_top: u.alwaysOnTop,
      theme: u.theme,
      compact: u.compact,
      opacity: u.opacity,
    });
  }

  async function hideToTray() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().hide();
    } catch (err) {
      console.warn("hide failed:", err);
    }
  }

  async function restoreWindow() {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      await win.show();
      await win.unminimize();
      await win.setFocus();
    } catch (err) {
      console.warn("restore failed:", err);
    }
  }

  onTrayEvent("show", restoreWindow);
  onTrayEvent("hide", hideToTray);
</script>

<svelte:window
  on:keydown={(e) => {
    if (e.key === "Escape") showSettings = false;
  }}
/>

<div class="widget" class:expanded style="opacity: {$ui && $ui.opacity}">
  <Header
    {expanded}
    onToggleExpanded={toggleExpanded}
    onHideToTray={hideToTray}
  />

  <div class="body">
    <ContextBar {expanded} />
    <SpeedBlock {expanded} />
    <ModelBlock {expanded} />
  </div>

  <Footer
    {expanded}
    diagnostics={$state.diagnostics}
    onToggleSettings={() => {
      showLogsTab = false;
      showSettings = !showSettings;
    }}
    onShowLogs={() => {
      showLogsTab = true;
      showSettings = true;
    }}
    onClickTray={hideToTray}
  />
</div>

{#if showSettings}
  <SettingsPanel
    onClose={() => (showSettings = false)}
    showLogsInitially={showLogsTab}
  />
{/if}

<style>
  .widget {
    width: 100%;
    height: 100%;
    background: rgba(32, 32, 32, .85);
    backdrop-filter: blur(40px) saturate(160%);
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, .08);
    box-shadow:
      0 12px 40px rgba(0, 0, 0, .45),
      0 2px 8px rgba(0, 0, 0, .3),
      inset 0 1px 0 rgba(255, 255, 255, .05);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: background .25s, border-color .25s;
    user-select: none;
  }

  :global(:root[data-theme="light"]) .widget {
    background: rgba(243, 243, 243, .92);
    border-color: rgba(0, 0, 0, .06);
    box-shadow:
      0 12px 40px rgba(0, 0, 0, .15),
      0 2px 8px rgba(0, 0, 0, .08),
      inset 0 1px 0 rgba(255, 255, 255, .6);
  }

  .body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
</style>
