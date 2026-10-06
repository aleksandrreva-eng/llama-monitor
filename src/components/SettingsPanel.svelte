<script>
  import { onDestroy } from "svelte";
  import { settings, get } from "../store";
  import { saveSettings, resetSettings } from "../tauriApi";
  import { t, setLocale } from "../i18n";
  import { createDebouncedApply } from "../lib/debounce";
  import ServerList from "./settings/ServerList.svelte";
  import LanScan from "./settings/LanScan.svelte";
  import ServerForm from "./settings/ServerForm.svelte";
  import PollingForm from "./settings/PollingForm.svelte";
  import LogsViewer from "./settings/LogsViewer.svelte";
  // Global form styling shared by the four settings sub-components.
  import "./settings/form.css";

  export let onClose;
  export let showLogsInitially = false;

  let showLogs = showLogsInitially;

  // `settings` is `writable(null)` until loadSettings() resolves. Only bind once
  // the settings actually exist (a null store value throws on `bind:value`).
  $: ready = $settings != null;

  $: servers = ($settings && $settings.servers) || [];
  $: activeId = $settings && ($settings.active_server_id || (servers[0] && servers[0].id));
  $: activeProfile = servers.find((p) => p.id === activeId) || null;

  // Free-text and numeric fields go through `apply` (one write once the user
  // pauses); toggles, dropdowns and structural changes go through `applyNow`.
  const form = createDebouncedApply(saveSettings);
  onDestroy(form.cancel);

  function genId() {
    return "srv_" + Math.random().toString(36).slice(2, 9);
  }

  function setActive(id) {
    form.applyNow({ active_server_id: id });
  }

  function setLanguage(lang) {
    // Switch the UI first so the panel re-renders immediately; the save is what
    // makes the choice survive a restart.
    setLocale(lang);
    form.applyNow({ language: lang });
  }

  function addServer() {
    const list = servers.concat([
      {
        id: genId(),
        label: $t("server_new"),
        url: "http://127.0.0.1:8080",
        kind: "local",
        api_key: null,
      },
    ]);
    form.applyNow({ servers: list });
  }

  function removeServer(id) {
    let list = servers.filter((p) => p.id !== id);
    if (list.length === 0) {
      // Never leave the list empty: the widget needs something to monitor.
      list = [
        {
          id: genId(),
          label: $t("server_local_default"),
          url: "http://127.0.0.1:8080",
          kind: "local",
          api_key: null,
        },
      ];
    }
    let active = activeId;
    if (active === id) active = list[0].id;
    form.applyNow({ servers: list, active_server_id: active });
  }

  /**
   * Apply a field edit to one profile.
   *
   * The store is updated **synchronously** and only the disk write is deferred.
   * Otherwise two edits inside one debounce window would each be computed from
   * the same stale `servers` list, and the second would silently drop the first.
   */
  function editServer(id, patch, immediate) {
    const current = get(settings);
    if (!current) return;
    const list = (current.servers || []).map((p) => (p.id === id ? { ...p, ...patch } : p));
    settings.set({ ...current, servers: list });
    if (immediate) form.applyNow({ servers: list });
    else form.apply({ servers: list });
  }

  function addDiscovered(discovered) {
    const list = servers.concat([
      {
        id: genId(),
        label: discovered.label,
        url: discovered.url,
        kind: discovered.kind,
        api_key: null,
      },
    ]);
    form.applyNow({ servers: list });
  }

  async function onReset() {
    // Drop anything still queued, or a stale patch would land right after the
    // reset and partially undo it.
    form.cancel();
    await resetSettings();
  }
</script>

<div class="panel">
  <div class="panel-head">
    <span>{$t("word_settings")}</span>
    <button
      class="close"
      title={$t("title_close")}
      aria-label={$t("title_close")}
      on:click={() => (onClose ? onClose() : null)}>✕</button
    >
  </div>

  {#if !ready}
    <div class="grid">
      <p class="hint loading">{$t("loading")}</p>
    </div>
  {:else}
    <div class="scroll">
      <section class="section">
        <ServerList
          {servers}
          {activeId}
          onSelect={setActive}
          onRemove={removeServer}
          onAdd={addServer}
        />
        <LanScan {servers} onAdd={addDiscovered} />
      </section>

      {#if activeProfile}
        <ServerForm
          profile={activeProfile}
          onEdit={(patch) => editServer(activeProfile.id, patch, false)}
          onCommit={(patch) => editServer(activeProfile.id, patch, true)}
        />
      {/if}

      <PollingForm onEdit={form.apply} onCommit={form.applyNow} onLanguage={setLanguage} />
    </div>
  {/if}

  <div class="panel-actions">
    <button class="btn" on:click={() => (showLogs = !showLogs)}>{$t("btn_logs")}</button>
    <button class="btn danger" on:click={onReset}>{$t("btn_reset")}</button>
  </div>

  <LogsViewer visible={showLogs} />
</div>

<style>
  .panel {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: var(--bg-secondary);
    border-right: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    z-index: 10;
    box-shadow: -8px 0 24px rgba(0, 0, 0, 0.14);
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    margin-bottom: 12px;
  }

  .close {
    padding: 2px 8px;
    border-radius: var(--radius-sm);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .loading {
    padding: 12px 0;
  }

  .panel-actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
</style>
