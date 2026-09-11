<script>
  import { state } from "../store";

  export let expanded = false;

  $: s = $state;
  $: ctx = s.context;

  let showTip = false;

  function fmt(n) {
    if (n == null) return "N/A";
    return n.toLocaleString("ru-RU");
  }

  $: pct = ctx.available && ctx.percent != null ? Math.round(ctx.percent * 100) : 0;
  $: label = ctx.available
    ? ctx.used != null
      ? `Всего ${fmt(ctx.total)} · Потрачено ${fmt(ctx.used)} · Осталось ${fmt(ctx.remaining)}`
      : `${fmt(ctx.total)} · потрачено неизвестно`
    : "нет данных";
</script>

<div class="section" class:warn={ctx.available && ctx.percent >= 0.75}
     on:mouseenter={() => (showTip = true)} on:mouseleave={() => (showTip = false)}>
  <div class="section-title">Контекст</div>
  <div class="progress-row">
    <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100"
         aria-valuetext={ctx.available ? `${pct}%` : "нет данных"}>
      <div class="progress-fill" style="width: {pct}%"></div>
    </div>
    <div class="progress-pct">{ctx.available ? pct + "%" : "—"}</div>
  </div>

  <div class="context-text compact-line">{label}</div>

  {#if expanded && ctx.available}
    <div class="expanded-rows">
      <div class="ctx-row"><span class="label">Всего</span><span class="value">{fmt(ctx.total)}</span></div>
      <div class="ctx-row"><span class="label">Потрачено</span><span class="value">{fmt(ctx.used)}</span></div>
      <div class="ctx-row"><span class="label">Осталось</span><span class="value">{fmt(ctx.remaining)}</span></div>
    </div>
  {/if}

  {#if showTip && ctx.available}
    <div class="tip">
      Всего: {fmt(ctx.total)} · Потрачено: {fmt(ctx.used)} · Осталось: {fmt(ctx.remaining)}
      · Заполнено: {(ctx.percent * 100).toFixed(1)}%
    </div>
  {/if}
</div>

<style>
  .section {
    padding: 10px 12px 12px;
    border-top: 1px solid rgba(255, 255, 255, .06);
    position: relative;
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
    background: rgba(255, 255, 255, .08);
    overflow: hidden;
  }
  :global(:root[data-theme="light"]) .progress {
    background: rgba(0, 0, 0, .08);
  }

  .progress-fill {
    height: 100%;
    border-radius: 4px;
    background: linear-gradient(90deg, #22C55E 0%, #EAB308 55%, #F97316 80%, #DC2626 100%);
    background-size: 420px 100%;
    transition: width .4s ease;
  }

  .progress-pct {
    font-family: var(--font);
    font-size: 13px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    min-width: 38px;
    text-align: right;
    color: #fff;
  }
  :global(:root[data-theme="light"]) .progress-pct {
    color: #1a1a1a;
  }

  .context-text {
    font-size: 12px;
    color: rgba(255, 255, 255, .65);
    font-variant-numeric: tabular-nums;
    line-height: 1.5;
  }
  :global(:root[data-theme="light"]) .context-text {
    color: rgba(0, 0, 0, .6);
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
    color: rgba(255, 255, 255, .55);
  }
  :global(:root[data-theme="light"]) .ctx-row .label {
    color: rgba(0, 0, 0, .5);
  }
  .ctx-row .value {
    font-variant-numeric: tabular-nums;
    color: #fff;
  }
  :global(:root[data-theme="light"]) .ctx-row .value {
    color: #1a1a1a;
  }

  .warn .progress-pct,
  .warn .context-text {
    color: #F87171;
  }
  :global(:root[data-theme="light"]) .warn .progress-pct,
  :global(:root[data-theme="light"]) .warn .context-text {
    color: #DC2626;
  }

  .tip {
    margin-top: 6px;
    font-size: 11px;
    color: rgba(255, 255, 255, .65);
    background: rgba(255, 255, 255, .08);
    border-radius: 6px;
    padding: 6px 8px;
  }
  :global(:root[data-theme="light"]) .tip {
    color: rgba(0, 0, 0, .6);
    background: rgba(0, 0, 0, .06);
  }
</style>
