<script>
  import { settings, ui, logStore } from "../store";
  import { saveSettings, resetSettings, clearLogs, readLogs, scanLan } from "../tauriApi";
  import { t, setLocale } from "../i18n";

  export let onClose;
  export let showLogsInitially = false;
  let showLogs = false;
  $: if (showLogsInitially) showLogs = true;

  // `settings` is `writable(null)` until loadSettings() resolves. Only bind once
  // the settings actually exist (a null store value throws on `bind:value`).
  $: ready = $settings != null;

  // --- server profile helpers --------------------------------------------
  const KIND_LABELS = {
    local: "llama.cpp",
    vllm: "vLLM",
    ollama: "Ollama",
    cloud: "kind_cloud",
  };

  $: servers = ($settings && $settings.servers) || [];
  $: activeId =
    $settings && ($settings.active_server_id || (servers[0] && servers[0].id));
  $: activeProfile = servers.find((p) => p.id === activeId) || null;

  function genId() {
    return "srv_" + Math.random().toString(36).slice(2, 9);
  }

  async function refreshLogs() {
    const lines = await readLogs(300);
    logStore.set(lines.length ? lines : [$t("log_empty")]);
  }

  // Copy the current log to the clipboard (with a legacy fallback for webviews
  // that block the async Clipboard API).
  let copyMsg = "";
  let copyTimer = null;

  function flashCopy(msg) {
    copyMsg = msg;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copyMsg = ""), 2500);
  }

  function fallbackCopy(text) {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.top = "-1000px";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    try {
      document.execCommand("copy");
    } catch (e) {
      throw e;
    } finally {
      document.body.removeChild(ta);
    }
  }

  async function copyLogs() {
    const text = ($logStore && $logStore.length ? $logStore.join("\n") : "").trim();
    if (!text) {
      flashCopy($t("copy_empty"));
      return;
    }
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
      } else {
        fallbackCopy(text);
      }
      flashCopy($t("copied"));
    } catch (e) {
      try {
        fallbackCopy(text);
        flashCopy($t("copied"));
      } catch (e2) {
        flashCopy($t("copy_failed", { e: e2 }));
      }
    }
  }

  function apply(patch) {
    saveSettings(patch);
  }

  function setActive(id) {
    apply({ active_server_id: id });
  }

  function setLanguage(lang) {
    setLocale(lang);
    apply({ language: lang });
  }

  function addServer() {
    const id = genId();
    const list = servers.concat([
      {
        id,
        label: $t("server_new"),
        url: "http://127.0.0.1:8080",
        kind: "local",
        api_key: null,
      },
    ]);
    apply({ servers: list });
  }

  function removeServer(id) {
    let list = servers.filter((p) => p.id !== id);
    if (list.length === 0) {
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
    apply({ servers: list, active_server_id: active });
  }

  function updateServer(id, patch) {
    const list = servers.map((p) => (p.id === id ? { ...p, ...patch } : p));
    apply({ servers: list });
  }

  // API keys arrive masked ("***") from the backend. We only overwrite the
  // stored key when the user types a real value; clearing the field sends null
  // (which the backend treats as "no key").
  function onApiKey(id, value) {
    updateServer(id, { api_key: value || null });
  }

  function apiKeyDisplay(key) {
    return key == null ? "" : key;
  }

  function themeClass(t) {
    if (t === "dark") return "dark";
    if (t === "light") return "light";
    return "auto";
  }

  // --- LAN scan ----------------------------------------------------------
  let scanning = false;
  let found = [];
  let scanError = "";

  async function runScan() {
    scanning = true;
    found = [];
    scanError = "";
    try {
      const res = await scanLan(9000);
      found = res || [];
      if (!found.length) scanError = $t("scan_none");
    } catch (e) {
      scanError = $t("scan_error", { e });
    } finally {
      scanning = false;
    }
  }

  function addDiscovered(f) {
    // Skip a server whose URL is already in the list.
    if (servers.some((p) => p.url === f.url)) {
      found = found.filter((x) => x.url !== f.url);
      return;
    }
    const id = genId();
    const list = servers.concat([
      {
        id,
        label: f.label,
        url: f.url,
        kind: f.kind,
        api_key: null,
      },
    ]);
    apply({ servers: list });
    found = found.filter((x) => x.url !== f.url);
  }
</script>

<div class="panel">
  <div class="panel-head">
    <span>{$t("word_settings")}</span>
    <button class="close" title={$t("title_close")} aria-label={$t("title_close")}
            on:click={() => (onClose ? onClose() : null)}>✕</button>
  </div>

  {#if !ready}
    <div class="grid">
      <p class="hint" style="padding:12px 0">{$t("loading")}</p>
    </div>
  {:else}
  <div class="scroll">
    <!-- Server profiles -->
    <section class="section">
      <div class="section-title">{$t("sec_servers")}</div>
      <div class="server-list">
        {#each servers as p (p.id)}
          <div class="server-row" class:active={p.id === activeId}>
            <button class="server-select" on:click={() => setActive(p.id)} title={$t("title_make_active")}>
              <span class="server-name">{p.label || p.url}</span>
              <span class="server-kind">{$t(KIND_LABELS[p.kind] || p.kind)}</span>
            </button>
            <button class="server-del" title={$t("title_delete_server")}
                    on:click={() => removeServer(p.id)}>✕</button>
          </div>
        {/each}
      </div>
      <button class="btn add" on:click={addServer}>+ {$t("add_server")}</button>

      <div class="lan-scan">
        <button class="btn scan" on:click={runScan} disabled={scanning}>
          {scanning ? $t("scanning") : $t("scan_lan")}
        </button>
        {#if scanError}
          <p class="hint scan-msg">{scanError}</p>
        {/if}
        {#if found.length}
          <div class="section-subtitle">{$t("found_in_net")}</div>
          <div class="found-list">
            {#each found as f (f.url)}
              <div class="found-row">
                <span class="found-name" title={f.url}>{f.label}</span>
                <span class="server-kind">{$t(KIND_LABELS[f.kind] || f.kind)}</span>
                <button class="found-add" title={$t("title_add_server")}
                        on:click={() => addDiscovered(f)}>＋</button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>

    <!-- Active profile editor -->
    {#if activeProfile}
      <section class="section">
        <div class="section-title">{$t("sec_active_server")}</div>
        <div class="grid">
          <label>
            <span>{$t("lbl_server_name")}</span>
            <input value={activeProfile.label}
                   on:input={(e) => updateServer(activeProfile.id, { label: e.target.value })}
                   placeholder={$t("ph_default")} />
          </label>

          <label>
            <span>{$t("lbl_type")}</span>
            <select value={activeProfile.kind}
                    on:change={(e) => updateServer(activeProfile.id, { kind: e.target.value })}>
              <option value="local">{$t("opt_local")}</option>
              <option value="vllm">vLLM</option>
              <option value="ollama">Ollama</option>
              <option value="cloud">{$t("kind_cloud")}</option>
            </select>
          </label>

          <label class="wide">
            <span>{$t("lbl_url")}</span>
            <input value={activeProfile.url}
                   on:input={(e) => updateServer(activeProfile.id, { url: e.target.value })}
                   placeholder="http://127.0.0.1:8080" />
            <small class="hint">
              {$t("hint_url")}
            </small>
          </label>

          <label class="wide">
            <span>{$t("lbl_api_key")}</span>
            <input type="password" value={apiKeyDisplay(activeProfile.api_key)}
                   on:input={(e) => onApiKey(activeProfile.id, e.target.value)}
                   placeholder={$t("ph_optional")} />
            <small class="hint">{$t("hint_api_key")}</small>
          </label>
        </div>
      </section>
    {/if}

    <!-- Shared monitoring settings -->
    <section class="section">
      <div class="section-title">{$t("sec_polling")}</div>
      <div class="grid">
        <label>
          <span>{$t("lbl_poll_interval")}</span>
          <input type="number" bind:value={$settings.poll_interval_ms}
                 on:input={() => apply({ poll_interval_ms: $settings.poll_interval_ms })} />
        </label>

        <label>
          <span>{$t("lbl_timeout")}</span>
          <input type="number" bind:value={$settings.request_timeout_ms}
                 on:input={() => apply({ request_timeout_ms: $settings.request_timeout_ms })} />
        </label>

        <label>
          <span>{$t("lbl_language")}</span>
          <select value={$settings.language}
                  on:change={(e) => setLanguage(e.target.value)}>
            <option value="ru">{$t("opt_lang_ru")}</option>
            <option value="en">{$t("opt_lang_en")}</option>
          </select>
        </label>

        <label>
          <span>{$t("lbl_theme")}</span>
          <select bind:value={$settings.theme} on:change={() => apply({ theme: $settings.theme })}>
            <option value="auto">{$t("opt_auto")}</option>
            <option value="light">{$t("opt_light")}</option>
            <option value="dark">{$t("opt_dark")}</option>
          </select>
        </label>

        <label>
          <span>{$t("lbl_opacity")}</span>
          <input type="range" min="0.5" max="1" step="0.05" bind:value={$settings.window_opacity}
                 on:input={() => apply({ window_opacity: $settings.window_opacity })} />
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.default_compact}
                 on:change={() => apply({ default_compact: $settings.default_compact })} />
          <span>{$t("lbl_default_compact")}</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.always_on_top}
                 on:change={() => apply({ always_on_top: $settings.always_on_top })} />
          <span>{$t("lbl_always_on_top")}</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.minimize_to_tray}
                 on:change={() => apply({ minimize_to_tray: $settings.minimize_to_tray })} />
          <span>{$t("lbl_minimize_tray")}</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.autorun}
                 on:change={() => apply({ autorun: $settings.autorun })} />
          <span>{$t("lbl_autorun")}</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.show_last_update}
                 on:change={() => apply({ show_last_update: $settings.show_last_update })} />
          <span>{$t("lbl_show_last_update")}</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.verbose_logging}
                 on:change={() => apply({ verbose_logging: $settings.verbose_logging })} />
          <span>{$t("lbl_verbose_log")}</span>
        </label>

        <label class="wide">
          <span>{$t("lbl_hotkey")}</span>
          <input bind:value={$settings.hotkey} placeholder="CmdOrCtrl+Shift+M"
                 on:input={() => apply({ hotkey: $settings.hotkey })} />
          <span class="hint">{$t("hint_hotkey")}</span>
        </label>
      </div>
    </section>
  </div>
  {/if}

  <div class="panel-actions">
    <button class="btn" on:click={() => { showLogs = !showLogs; if (showLogs) refreshLogs(); }}>{$t("btn_logs")}</button>
    <button class="btn danger" on:click={resetSettings}>{$t("btn_reset")}</button>
    {#if showLogs}
      <button class="btn" on:click={refreshLogs}>{$t("btn_refresh")}</button>
      <button class="btn" on:click={copyLogs}>{$t("btn_copy")}</button>
      <button class="btn" on:click={async () => { await clearLogs(); logStore.set([$t("log_cleared")]); }}>{$t("btn_clear_logs")}</button>
    {/if}
  </div>

  {#if showLogs}
    <div class="logs">
      {#each $logStore as line}
        <div class="log-line">{line}</div>
      {/each}
    </div>
    {#if copyMsg}
      <p class="copy-msg">{copyMsg}</p>
    {/if}
  {/if}
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
    border-radius: 6px;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .section {
    margin-bottom: 14px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-hint);
    margin-bottom: 8px;
  }

  .server-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 8px;
  }

  .server-row {
    display: flex;
    align-items: stretch;
    gap: 6px;
  }

  .server-select {
    flex: 1;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }

  .server-row.active .server-select {
    border-color: var(--accent, #0078d4);
    background: color-mix(in srgb, var(--accent, #0078d4) 12%, var(--bg-primary));
  }

  .server-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .server-kind {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-hint);
    background: var(--bg-tertiary);
    border-radius: 4px;
    padding: 1px 6px;
  }

  .server-del {
    flex-shrink: 0;
    width: 30px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .server-del:hover {
    border-color: var(--bad);
    color: var(--bad);
  }

  .btn.add {
    width: 100%;
    border-style: dashed;
  }

  .lan-scan {
    margin-top: 10px;
    border-top: 1px dashed var(--border);
    padding-top: 10px;
  }

  .btn.scan {
    width: 100%;
  }

  .btn.scan:disabled {
    opacity: 0.6;
    cursor: progress;
  }

  .scan-msg {
    margin-top: 6px;
    color: var(--text-hint);
  }

  .section-subtitle {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-hint);
    margin: 10px 0 6px;
  }

  .found-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .found-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .found-name {
    flex: 1;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .found-add {
    flex-shrink: 0;
    width: 30px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .found-add:hover {
    border-color: var(--accent, #0078d4);
    color: var(--accent, #0078d4);
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-secondary);
  }

  label.wide {
    grid-column: 1 / -1;
  }

  label.switch {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  input,
  select {
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 12px;
  }

  input[type="range"] {
    width: 100%;
  }

  .hint {
    font-size: 10px;
    color: var(--text-hint);
  }

  .hint code {
    font-family: monospace;
    background: var(--bg-tertiary);
    padding: 0 3px;
    border-radius: 3px;
  }

  .panel-actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }

  .btn {
    padding: 6px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-tertiary);
    font-size: 12px;
  }

  .btn.danger {
    border-color: var(--bad);
    color: var(--bad);
  }

  .logs {
    margin-top: 12px;
    max-height: 160px;
    overflow-y: auto;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 8px;
    font-family: monospace;
    font-size: 11px;
    color: var(--text-secondary);
    /* Counter the global `user-select: none` so log text can be selected. */
    -webkit-user-select: text;
    user-select: text;
  }

  .log-line {
    padding: 1px 0;
    white-space: pre-wrap;
  }

  .copy-msg {
    margin-top: 6px;
    font-size: 10px;
    color: var(--good);
  }
</style>
