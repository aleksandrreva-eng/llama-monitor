<script>
  import { state } from "../store";
  import { t, formatInt } from "../i18n";
  import Section from "./Section.svelte";

  export let expanded = false;

  $: metrics = $state.otherMetrics || [];

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
  <Section title={$t("metrics_title")}>
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
  </Section>
{/if}

<style>
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
    font-family: var(--font-mono);
    color: var(--text-label);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .metric-value {
    font-family: var(--font);
    font-variant-numeric: tabular-nums;
    color: var(--text-strong);
  }

  .metric-empty {
    font-size: 12px;
    color: var(--text-path);
    line-height: 1.7;
  }
</style>
