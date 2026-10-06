<script>
  import { t } from "../../i18n";
  import { scanLan } from "../../tauriApi";

  /** Stored server profiles, used to skip URLs that are already known. */
  export let servers = [];
  /** Called with a discovered profile `{ label, url, kind }`. */
  export let onAdd;

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

  function add(discovered) {
    if (servers.some((p) => p.url === discovered.url)) {
      // Already configured — just drop it from the results.
      found = found.filter((x) => x.url !== discovered.url);
      return;
    }
    onAdd(discovered);
    found = found.filter((x) => x.url !== discovered.url);
  }
</script>

<div class="lan-scan">
  <button class="btn block" on:click={runScan} disabled={scanning}>
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
          <span class="server-kind">{$t(f.kind === "cloud" ? "kind_cloud" : f.kind)}</span>
          <button class="found-add" title={$t("title_add_server")} on:click={() => add(f)}
            >＋</button
          >
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .lan-scan {
    margin-top: 10px;
    border-top: 1px dashed var(--border);
    padding-top: 10px;
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

  .server-kind {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-hint);
    background: var(--bg-tertiary);
    border-radius: 4px;
    padding: 1px 6px;
  }

  .found-add {
    flex-shrink: 0;
    width: 30px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .found-add:hover {
    border-color: var(--accent, #0078d4);
    color: var(--accent, #0078d4);
  }
</style>
