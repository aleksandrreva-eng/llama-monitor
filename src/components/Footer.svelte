<script>
  import { state } from "../store";

  export let expanded = false;
  export let diagnostics = [];
  export let onToggleSettings;
  export let onShowLogs;
  export let onClickTray;

  $: s = $state;

  let now = Date.now();
  const timer = setInterval(() => { now = Date.now(); }, 1000);

  function relativeTime(ts) {
    if (!ts) return "—";
    const sec = Math.round((now - ts) / 1000);
    if (sec < 5) return "только что";
    if (sec < 60) return `${sec} сек назад`;
    const min = Math.round(sec / 60);
    if (min < 60) return `${min} мин назад`;
    return "давно";
  }

  $: updateText = s.lastUpdate ? `Обновлено ${relativeTime(s.lastUpdate)}` : "—";
</script>

<div class="footer">
  <span>{updateText}</span>
</div>

{#if expanded}
  <div class="footer-diag">
    {#each diagnostics as d}
      <div>{d}</div>
    {:else}
      <div class="hint">Источник: /metrics + /props · Ошибок: 0</div>
    {/each}
  </div>

  <div class="footer-actions">
    <button on:click={onToggleSettings}>Настройки</button>
    <button on:click={onShowLogs}>Логи</button>
    <button on:click={onClickTray}>Свернуть</button>
  </div>
{/if}

<style>
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-top: 1px solid rgba(255, 255, 255, .06);
    font-size: 11px;
    color: rgba(255, 255, 255, .45);
  }
  :global(:root[data-theme="light"]) .footer {
    border-top-color: rgba(0, 0, 0, .06);
    color: rgba(0, 0, 0, .5);
  }

  .footer-diag {
    font-size: 11px;
    padding: 0 12px 10px;
    color: rgba(255, 255, 255, .4);
  }
  :global(:root[data-theme="light"]) .footer-diag {
    color: rgba(0, 0, 0, .5);
  }

  .footer-diag .hint {
    color: rgba(255, 255, 255, .35);
  }
  :global(:root[data-theme="light"]) .footer-diag .hint {
    color: rgba(0, 0, 0, .4);
  }

  .footer-actions {
    display: flex;
    gap: 6px;
    padding: 0 12px 12px;
  }

  .footer-actions button {
    font-size: 11px;
    padding: 5px 10px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, .12);
    background: rgba(255, 255, 255, .04);
    color: inherit;
    cursor: pointer;
    font-family: var(--font-text);
  }
  .footer-actions button:hover {
    background: rgba(255, 255, 255, .1);
  }
  :global(:root[data-theme="light"]) .footer-actions button {
    border-color: rgba(0, 0, 0, .12);
    background: rgba(0, 0, 0, .03);
    color: #1a1a1a;
  }
  :global(:root[data-theme="light"]) .footer-actions button:hover {
    background: rgba(0, 0, 0, .07);
  }
</style>
