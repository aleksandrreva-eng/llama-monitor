<script>
  import { onMount, onDestroy, tick } from "svelte";
  import Header from "./components/Header.svelte";
  import ContextBar from "./components/ContextBar.svelte";
  import SpeedBlock from "./components/SpeedBlock.svelte";
  import ModelBlock from "./components/ModelBlock.svelte";
  import MetricsBlock from "./components/MetricsBlock.svelte";
  import Footer from "./components/Footer.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import { state, ui, settings, pushLog, initTauri, get } from "./store";
  import { loadSettings, savePosition, saveSize, setOptions } from "./tauriApi";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { createWindowFitter } from "./lib/windowFit";

  let expanded = false;
  let showSettings = false;
  let showLogsTab = false;
  let geoTimer = null;
  let widgetEl = null;

  const fitter = createWindowFitter(() => widgetEl);

  // `default_compact` seeds the initial mode once the settings have loaded:
  // compact means "not expanded". Applying it here (rather than at declaration)
  // is what makes the setting actually work.
  function seedExpanded() {
    const s = get(settings);
    if (!s) return;
    expanded = !s.default_compact;
    // The window was already fitted for the default (compact) layout, so
    // re-measure for the seeded one.
    fitter.reset();
    tick().then(fitter.fit);
  }

  let unsubscribeState = null;
  let unlistenMove = null;
  let unlistenResize = null;

  onMount(async () => {
    await initTauri();
    await loadSettings();
    seedExpanded();
    pushLog("app started");
    fitter.fit();

    // Refit whenever telemetry lands: the content height is not constant (the
    // speed block grows the moment a reading arrives, diagnostics change
    // length). `subscribe` fires immediately too, covering the first frame.
    unsubscribeState = state.subscribe(() => fitter.schedule());

    // Persist window geometry (debounced) so position/size survive restarts.
    try {
      const win = getCurrentWindow();
      unlistenMove = await win.onMoved(({ payload }) => {
        if (typeof payload?.x !== "number" || typeof payload?.y !== "number") return;
        scheduleGeoSave(() => savePosition(payload.x, payload.y));
      });
      unlistenResize = await win.onResized(({ payload }) => {
        const w = payload?.width ?? payload?.size?.width;
        const h = payload?.height ?? payload?.size?.height;
        if (typeof w !== "number" || typeof h !== "number") return;
        scheduleGeoSave(() => saveSize(w, h));
      });
    } catch (err) {
      pushLog("geometry listeners failed: " + err);
    }
  });

  onDestroy(() => {
    if (geoTimer) clearTimeout(geoTimer);
    if (unsubscribeState) unsubscribeState();
    if (unlistenMove) unlistenMove();
    if (unlistenResize) unlistenResize();
  });

  function scheduleGeoSave(fn) {
    if (geoTimer) clearTimeout(geoTimer);
    geoTimer = setTimeout(fn, 400);
  }

  // Explicitly re-fit when the user toggles compact <-> expanded. Driving this
  // from the handler (after tick()) is reliable, unlike a reactive statement
  // that also depends on the transient render state.
  async function toggleExpanded() {
    expanded = !expanded;
    await tick();
    fitter.fit();
  }

  $: if ($ui) {
    applyOptions($ui);
  }

  $: if ($ui && $ui.theme) {
    document.documentElement.setAttribute("data-theme", $ui.theme);
    // Section paddings and the number of visible rows change with the theme's
    // font metrics, so the height has to be re-measured.
    fitter.schedule();
  }

  // Only the two options the window itself owns. `compact` is local state and
  // `opacity` is applied as CSS below — sending them to the backend used to be
  // a silent no-op (they were logged and dropped).
  function applyOptions(u) {
    setOptions({ always_on_top: u.alwaysOnTop, theme: u.theme });
  }

  async function hideToTray() {
    try {
      await getCurrentWindow().hide();
    } catch (err) {
      pushLog("hide failed: " + err);
    }
  }
</script>

<svelte:window
  on:keydown={(e) => {
    if (e.key === "Escape") showSettings = false;
  }}
/>

<div class="widget" class:expanded style="opacity: {$ui.opacity}" bind:this={widgetEl}>
  <Header {expanded} onToggleExpanded={toggleExpanded} onHideToTray={hideToTray} />

  <div class="body">
    <ContextBar {expanded} />
    <SpeedBlock {expanded} />
    <ModelBlock {expanded} />
    <MetricsBlock {expanded} />
  </div>

  <Footer
    {expanded}
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
  <SettingsPanel onClose={() => (showSettings = false)} showLogsInitially={showLogsTab} />
{/if}

<style>
  .widget {
    width: 100%;
    height: 100%;
    background: var(--surface);
    backdrop-filter: blur(40px) saturate(160%);
    border-radius: var(--radius-lg);
    border: 1px solid var(--surface-border);
    box-shadow: var(--shadow-widget);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition:
      background 0.25s,
      border-color 0.25s;
    user-select: none;
  }

  .body {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
</style>
