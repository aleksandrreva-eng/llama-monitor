<script>
  import { onDestroy } from "svelte";
  import { logStore } from "../../store";
  import { t } from "../../i18n";
  import { clearLogs, readLogs } from "../../tauriApi";

  /** Whether the log pane is expanded. Refreshes on the rising edge. */
  export let visible = false;

  let copyMsg = "";
  let copyTimer = null;
  onDestroy(() => {
    if (copyTimer) clearTimeout(copyTimer);
  });

  // Refresh on the rising edge of `visible`. The previous value lives in a
  // plain container so the block does not read and write the same tracked
  // variable (Svelte calls that a cyclical dependency).
  const seen = { visible: false };
  $: if (visible !== seen.visible) {
    seen.visible = visible;
    if (visible) refresh();
  }

  async function refresh() {
    const lines = await readLogs(300);
    logStore.set(lines.length ? lines : [$t("log_empty")]);
  }

  function flashCopy(msg) {
    copyMsg = msg;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copyMsg = ""), 2500);
  }

  // Legacy fallback for webviews that block the async Clipboard API.
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

  async function onClear() {
    await clearLogs();
    logStore.set([$t("log_cleared")]);
  }
</script>

{#if visible}
  <div class="log-actions">
    <button class="btn" on:click={refresh}>{$t("btn_refresh")}</button>
    <button class="btn" on:click={copyLogs}>{$t("btn_copy")}</button>
    <button class="btn" on:click={onClear}>{$t("btn_clear_logs")}</button>
  </div>

  <div class="logs">
    {#each $logStore as line}
      <div class="log-line">{line}</div>
    {/each}
  </div>

  {#if copyMsg}
    <p class="copy-msg">{copyMsg}</p>
  {/if}
{/if}

<style>
  .log-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .logs {
    margin-top: 8px;
    max-height: 160px;
    overflow-y: auto;
    background: var(--bg-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px;
    font-family: var(--font-mono);
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
