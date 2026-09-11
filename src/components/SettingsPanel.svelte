<script>
  import { settings, ui, logStore } from "../store";
  import { saveSettings, resetSettings, clearLogs, readLogs } from "../tauriApi";

  export let onClose;
  export let showLogsInitially = false;
  let showLogs = false;
  $: if (showLogsInitially) showLogs = true;

  // `settings` is `writable(null)` until loadSettings() resolves. The template
  // below dereferences `$settings.server_host` etc., and `bind:value` on a null
  // store value throws — so only bind once the settings actually exist.
  $: ready = $settings != null;

  async function refreshLogs() {
    const lines = await readLogs(300);
    logStore.set(lines.length ? lines : ["(лог пуст)"]);
  }

  async function apply(patch) {
    await saveSettings(patch);
  }

  function themeClass(t) {
    if (t === "dark") return "dark";
    if (t === "light") return "light";
    return "auto";
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
  <div class="grid">
    <label>
      <span>Адрес сервера</span>
      <input bind:value={$settings.server_host} placeholder="127.0.0.1"
             on:input={() => apply({ server_host: $settings.server_host })} />
    </label>

    <label>
      <span>Порт</span>
      <input type="number" bind:value={$settings.server_port}
             on:input={() => apply({ server_port: $settings.server_port })} />
    </label>

    <label>
      <span>Префикс пути</span>
      <input bind:value={$settings.base_path} placeholder="(корень сервера)"
             on:input={() => apply({ base_path: $settings.base_path })} />
      <small class="hint">Обычно пусто. Для llama.cpp все метрики (/health, /props, /slots) живут в корне; /v1 добавляется автоматически для OpenAI-совместимых запросов.</small>
    </label>

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
      <span>Имя сервера</span>
      <input bind:value={$settings.server_label} placeholder="(по умолчанию)"
             on:input={() => apply({ server_label: $settings.server_label })} />
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
      <span>Комактный режим по умолчанию</span>
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

    <label>
      <span>Горячая клавиша (показать/скрыть)</span>
      <input bind:value={$settings.hotkey} placeholder="CmdOrCtrl+Shift+M"
             on:input={() => apply({ hotkey: $settings.hotkey })} />
      <span class="hint">Например: CmdOrCtrl+Shift+M. Пустое поле — отключить.</span>
    </label>

    <label>
      <span>API-ключ</span>
      <input type="password" bind:value={$settings.api_key} placeholder="(необязательно)"
             on:input={() => apply({ api_key: $settings.api_key })} />
    </label>
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
    overflow-y: auto;
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
