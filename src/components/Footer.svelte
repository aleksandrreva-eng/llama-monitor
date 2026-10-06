<script>
  import { onDestroy } from "svelte";
  import { state, settings } from "../store";
  import { t } from "../i18n";
  import { diagText } from "../lib/diagnostics";

  export let expanded = false;
  export let onToggleSettings;
  export let onShowLogs;
  export let onClickTray;

  $: s = $state;

  let now = Date.now();
  const timer = setInterval(() => {
    now = Date.now();
  }, 1000);
  // Without this the interval kept ticking after the component was destroyed
  // and wrote to a dead component's state.
  onDestroy(() => clearInterval(timer));

  function relativeTime(ts) {
    if (!ts) return "—";
    const sec = Math.round((now - ts) / 1000);
    if (sec < 5) return $t("time_just_now");
    if (sec < 60) return $t("time_sec_ago", { sec });
    const min = Math.round(sec / 60);
    if (min < 60) return $t("time_min_ago", { min });
    return $t("time_long_ago");
  }

  // The "last update" line is optional; the setting is persisted and gated here
  // (it used to be saved and then ignored).
  $: showUpdated = $settings ? $settings.show_last_update : true;
  $: updateText = s.lastUpdate ? $t("updated", { t: relativeTime(s.lastUpdate) }) : "—";
</script>

<div class="footer">
  <span>{showUpdated ? updateText : ""}</span>
</div>

{#if expanded}
  <div class="footer-diag">
    {#each s.diagnostics as d}
      <div>{diagText(d, $t)}</div>
    {:else}
      <div class="hint">{$t("diag_none")}</div>
    {/each}
  </div>

  <div class="footer-actions">
    <button on:click={onToggleSettings}>{$t("btn_settings")}</button>
    <button on:click={onShowLogs}>{$t("btn_logs")}</button>
    <button on:click={onClickTray}>{$t("btn_minimize")}</button>
  </div>
{/if}

<style>
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-top: 1px solid var(--divider);
    font-size: 11px;
    color: var(--text-faint);
  }

  .footer-diag {
    font-size: 11px;
    padding: 0 12px 10px;
    color: var(--text-path);
  }

  .footer-diag .hint {
    color: var(--text-path);
  }

  .footer-actions {
    display: flex;
    gap: 6px;
    padding: 0 12px 12px;
  }

  .footer-actions button {
    font-size: 11px;
    padding: 5px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--control-border);
    background: var(--control-bg);
    color: inherit;
    cursor: pointer;
    font-family: var(--font-text);
  }
  .footer-actions button:hover {
    background: var(--control-bg-hover);
  }
</style>
