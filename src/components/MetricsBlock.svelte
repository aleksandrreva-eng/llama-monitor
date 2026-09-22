<script>
  import { state } from "../store";
  import { t, formatInt } from "../i18n";

  export let expanded = false;

  $: s = $state;
  $: metrics = s.otherMetrics || [];

  function fmt(v) {
    if (v == null || Number.isNaN(v)) return "—";
    // Cumulative counters and gauges are whole numbers in practice; keep
    // sub-integer gauges readable with two decimals instead of a long float.
    if (Math.abs(v) >= 1 && Number.isInteger(v)) return formatInt(Math.round(v));
    return Number(v).toFixed(2);
  }

  $: hasMetrics = metrics.length > 0;
</script>

{#if expanded}
  <div class="section">
    <div class="section-title">{$t("metrics_title")}</div>
    {#if hasMetrics}
      <div class="metrics-grid">
        {#each metrics as m (m.name)}
          <div class="metric-row">
            <span class="metric-name">{m.name}</span>
            <span class="metric-value">{fmt(m.value)}</span>
          </div>
        {/each}
      </div>
    {:else}
      <div class="metric-empty">{$t("metrics_empty")}</div>
    {/if}
  </div>
{/if}

<style>
  .section {
    padding: 10px 12px 12px;
    border-top: 1px solid rgba(255, 255, 255, .06);
  }
  :global(:root[data-theme="light"]) .section {
    border-top-color: rgba(0, 0, 0, .06);
  }

  .section-title {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: .6px;
    text-transform: uppercase;
    color: rgba(255, 255, 255, .5);
    margin-bottom: 8px;
  }
  :global(:root[data-theme="light"]) .section-title {
    color: rgba(0, 0, 0, .45);
  }

  .metrics-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 16px;
  }

  .metric-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 12px;
    line-height: 1.7;
  }
  .metric-name {
    font-family: "Cascadia Code", Consolas, monospace;
    color: rgba(255, 255, 255, .55);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(:root[data-theme="light"]) .metric-name {
    color: rgba(0, 0, 0, .5);
  }
  .metric-value {
    font-family: var(--font);
    font-variant-numeric: tabular-nums;
    color: #fff;
  }
  :global(:root[data-theme="light"]) .metric-value {
    color: #1a1a1a;
  }

  .metric-empty {
    font-size: 12px;
    color: rgba(255, 255, 255, .4);
    line-height: 1.7;
  }
  :global(:root[data-theme="light"]) .metric-empty {
    color: rgba(0, 0, 0, .45);
  }
</style>
