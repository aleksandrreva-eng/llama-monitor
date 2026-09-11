<script>
  import { settings, ui, logStore } from "../store";
  import { saveSettings, resetSettings, clearLogs, readLogs, scanLan } from "../tauriApi";

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
    cloud: "Облако (OpenAI)",
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
    logStore.set(lines.length ? lines : ["(лог пуст)"]);
  }

  function apply(patch) {
    saveSettings(patch);
  }

  function setActive(id) {
    apply({ active_server_id: id });
  }

  function addServer() {
    const id = genId();
    const list = servers.concat([
      {
        id,
        label: "Новый сервер",
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
          label: "Локальный llama.cpp",
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
      if (!found.length) scanError = "Серверы не найдены в локальной сети.";
    } catch (e) {
      scanError = "Ошибка сканирования: " + e;
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
    <span>Настройки</span>
    <button class="close" title="Закрыть" aria-label="Закрыть"
            on:click={() => (onClose ? onClose() : null)}>✕</button>
  </div>

  {#if !ready}
    <div class="grid">
      <p class="hint" style="padding:12px 0">Загрузка настроек…</p>
    </div>
  {:else}
  <div class="scroll">
    <!-- Server profiles -->
    <section class="section">
      <div class="section-title">Серверы</div>
      <div class="server-list">
        {#each servers as p (p.id)}
          <div class="server-row" class:active={p.id === activeId}>
            <button class="server-select" on:click={() => setActive(p.id)} title="Сделать активным">
              <span class="server-name">{p.label || p.url}</span>
              <span class="server-kind">{KIND_LABELS[p.kind] || p.kind}</span>
            </button>
            <button class="server-del" title="Удалить сервер"
                    on:click={() => removeServer(p.id)}>✕</button>
          </div>
        {/each}
      </div>
      <button class="btn add" on:click={addServer}>+ Добавить сервер</button>

      <div class="lan-scan">
        <button class="btn scan" on:click={runScan} disabled={scanning}>
          {scanning ? "Сканирование…" : "🔍 Сканировать LAN"}
        </button>
        {#if scanError}
          <p class="hint scan-msg">{scanError}</p>
        {/if}
        {#if found.length}
          <div class="section-subtitle">Найдено в сети</div>
          <div class="found-list">
            {#each found as f (f.url)}
              <div class="found-row">
                <span class="found-name" title={f.url}>{f.label}</span>
                <span class="server-kind">{KIND_LABELS[f.kind] || f.kind}</span>
                <button class="found-add" title="Добавить сервер"
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
        <div class="section-title">Активный сервер</div>
        <div class="grid">
          <label>
            <span>Имя сервера</span>
            <input value={activeProfile.label}
                   on:input={(e) => updateServer(activeProfile.id, { label: e.target.value })}
                   placeholder="(по умолчанию)" />
          </label>

          <label>
            <span>Тип</span>
            <select value={activeProfile.kind}
                    on:change={(e) => updateServer(activeProfile.id, { kind: e.target.value })}>
              <option value="local">llama.cpp (локальный/удалённый)</option>
              <option value="vllm">vLLM</option>
              <option value="ollama">Ollama</option>
              <option value="cloud">Облако (OpenAI)</option>
            </select>
          </label>

          <label class="wide">
            <span>URL</span>
            <input value={activeProfile.url}
                   on:input={(e) => updateServer(activeProfile.id, { url: e.target.value })}
                   placeholder="http://127.0.0.1:8080" />
            <small class="hint">
              Без суффикса <code>/v1</code>: нативные эндпоинты (для llama.cpp — корень;
              для Ollama — <code>/api</code>) и OpenAI-совместимый <code>/v1</code>
              выводятся автоматически.
            </small>
          </label>

          <label class="wide">
            <span>API-ключ (опционально)</span>
            <input type="password" value={apiKeyDisplay(activeProfile.api_key)}
                   on:input={(e) => onApiKey(activeProfile.id, e.target.value)}
                   placeholder="(необязательно)" />
            <small class="hint">Не попадает в логи. Пустое поле — ключ удаляется.</small>
          </label>
        </div>
      </section>
    {/if}

    <!-- Shared monitoring settings -->
    <section class="section">
      <div class="section-title">Опрос и подключение</div>
      <div class="grid">
        <label>
          <span>Интервал опроса, мс</span>
          <input type="number" bind:value={$settings.poll_interval_ms}
                 on:input={() => apply({ poll_interval_ms: $settings.poll_interval_ms })} />
        </label>

        <label>
          <span>Таймаут запроса, мс</span>
          <input type="number" bind:value={$settings.request_timeout_ms}
                 on:input={() => apply({ request_timeout_ms: $settings.request_timeout_ms })} />
        </label>

        <label>
          <span>Тема</span>
          <select bind:value={$settings.theme} on:change={() => apply({ theme: $settings.theme })}>
            <option value="auto">Авто</option>
            <option value="light">Светлая</option>
            <option value="dark">Тёмная</option>
          </select>
        </label>

        <label>
          <span>Прозрачность окна</span>
          <input type="range" min="0.5" max="1" step="0.05" bind:value={$settings.window_opacity}
                 on:input={() => apply({ window_opacity: $settings.window_opacity })} />
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.default_compact}
                 on:change={() => apply({ default_compact: $settings.default_compact })} />
          <span>Компактный режим по умолчанию</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.always_on_top}
                 on:change={() => apply({ always_on_top: $settings.always_on_top })} />
          <span>Закрепить поверх всех окон</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.minimize_to_tray}
                 on:change={() => apply({ minimize_to_tray: $settings.minimize_to_tray })} />
          <span>Сворачивать в трей при закрытии</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.autorun}
                 on:change={() => apply({ autorun: $settings.autorun })} />
          <span>Запуск при старте Windows</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.show_last_update}
                 on:change={() => apply({ show_last_update: $settings.show_last_update })} />
          <span>Показывать время последнего обновления</span>
        </label>

        <label class="switch">
          <input type="checkbox" bind:checked={$settings.verbose_logging}
                 on:change={() => apply({ verbose_logging: $settings.verbose_logging })} />
          <span>Расширенное логирование</span>
        </label>

        <label class="wide">
          <span>Горячая клавиша (показать/скрыть)</span>
          <input bind:value={$settings.hotkey} placeholder="CmdOrCtrl+Shift+M"
                 on:input={() => apply({ hotkey: $settings.hotkey })} />
          <span class="hint">Например: CmdOrCtrl+Shift+M. Пустое поле — отключить.</span>
        </label>
      </div>
    </section>
  </div>
  {/if}

  <div class="panel-actions">
    <button class="btn" on:click={() => { showLogs = !showLogs; if (showLogs) refreshLogs(); }}>Логи</button>
    <button class="btn danger" on:click={resetSettings}>Сбросить настройки</button>
    {#if showLogs}
      <button class="btn" on:click={refreshLogs}>Обновить</button>
      <button class="btn" on:click={async () => { await clearLogs(); logStore.set(["(лог очищен)"]); }}>Очистить логи</button>
    {/if}
  </div>

  {#if showLogs}
    <div class="logs">
      {#each $logStore as line}
        <div class="log-line">{line}</div>
      {/each}
    </div>
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
  }

  .log-line {
    padding: 1px 0;
    white-space: pre-wrap;
  }
</style>
