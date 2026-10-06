<script>
  import { state, settings } from "../store";
  import { t, formatInt } from "../i18n";
  import Section from "./Section.svelte";

  export let expanded = false;

  $: ctx = $state.context;

  let showTip = false;

  const fmt = (n) => formatInt(n);

  // Fill ratios come from Settings (0..1). The fallbacks match
  // `Settings::default()` so the bar behaves sensibly before the settings load.
  $: warnAt = $settings?.warning_threshold ?? 0.75;
  $: criticalAt = $settings?.critical_threshold ?? 0.9;

  // `percent` is `null` when the total is known but the used amount is not.
  // Multiplying null by 100 yields 0 — which would claim "filled: 0.0%" for a
  // server whose usage is simply unknown. So the ratio is only defined when
  // `percent` is actually present.
  $: hasPercent = ctx.available && ctx.percent != null;
  $: ratio = hasPercent ? ctx.percent : 0;
  $: pct = hasPercent ? Math.round(ratio * 100) : 0;
  $: warn = hasPercent && ratio >= warnAt;
  $: critical = hasPercent && ratio >= criticalAt;

  $: label = ctx.available
    ? ctx.used != null
      ? $t("ctx_summary", {
          total: fmt(ctx.total),
          used: fmt(ctx.used),
          remaining: fmt(ctx.remaining),
        })
      : $t("ctx_unknown_used", { total: fmt(ctx.total) })
    : $t("no_data");
</script>

<Section title={$t("ctx_title")}>
  <!-- The warn/critical classes live on a wrapper inside this component so the
       scoped styles below can reach the elements they colour. -->
  <div
    class="ctx-body"
    class:warn
    class:critical
    role="group"
    aria-label={$t("ctx_title")}
    on:mouseenter={() => (showTip = true)}
    on:mouseleave={() => (showTip = false)}
  >
    <div class="progress-row">
      <div
        class="progress"
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuetext={hasPercent ? `${pct}%` : $t("no_data")}
      >
        <div class="progress-fill" style="width: {pct}%"></div>
      </div>
      <div class="progress-pct">{hasPercent ? pct + "%" : "—"}</div>
    </div>

    <div class="context-text compact-line">{label}</div>

    {#if expanded && ctx.available}
      <div class="expanded-rows">
        <div class="ctx-row">
          <span class="label">{$t("ctx_total")}</span><span class="value">{fmt(ctx.total)}</span>
        </div>
        <div class="ctx-row">
          <span class="label">{$t("ctx_used")}</span><span class="value">{fmt(ctx.used)}</span>
        </div>
        <div class="ctx-row">
          <span class="label">{$t("ctx_remaining")}</span><span class="value"
            >{fmt(ctx.remaining)}</span
          >
        </div>
      </div>
    {/if}

    {#if showTip && ctx.available}
      <div class="tip">
        {hasPercent
          ? $t("ctx_tip", {
              total: fmt(ctx.total),
              used: fmt(ctx.used),
              remaining: fmt(ctx.remaining),
              pct: (ratio * 100).toFixed(1),
            })
          : $t("ctx_tip_unknown", { total: fmt(ctx.total) })}
      </div>
    {/if}
  </div>
</Section>

<style>
  .progress-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .progress {
    flex: 1;
    height: 8px;
    border-radius: 4px;
    background: var(--inset);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    border-radius: 4px;
    background: linear-gradient(90deg, #22c55e 0%, #eab308 55%, #f97316 80%, #dc2626 100%);
    background-size: 420px 100%;
    transition: width 0.4s ease;
  }

  .progress-pct {
    font-family: var(--font);
    font-size: 13px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    min-width: 38px;
    text-align: right;
    color: var(--text-strong);
  }

  .context-text {
    font-size: 12px;
    color: var(--text-body);
    font-variant-numeric: tabular-nums;
    line-height: 1.5;
  }

  .expanded-rows {
    display: none;
  }
  :global(.widget.expanded) .expanded-rows {
    display: block;
  }
  :global(.widget.expanded) .compact-line {
    display: none;
  }

  .ctx-row {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    line-height: 1.7;
  }
  .ctx-row .label {
    color: var(--text-label);
  }
  .ctx-row .value {
    font-variant-numeric: tabular-nums;
    color: var(--text-strong);
  }

  .warn .progress-pct,
  .warn .context-text {
    color: var(--warn-text);
  }

  .critical .progress-pct,
  .critical .context-text {
    color: var(--bad);
  }

  .tip {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-body);
    background: var(--inset);
    border-radius: var(--radius-sm);
    padding: 6px 8px;
  }
</style>
