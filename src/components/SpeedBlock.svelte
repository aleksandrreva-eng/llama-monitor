<script>
  import { state } from "../store";

  export let expanded = false;

  $: s = $state;
  $: pf = s.prefillSpeed;
  $: gn = s.generationSpeed;

  let pfHistory = [];
  let gnHistory = [];

  $: {
    const v = $state.prefillSpeed.current;
    if (v != null) pfHistory = [...pfHistory.slice(-29), v];
  }
  $: {
    const v = $state.generationSpeed.current;
    if (v != null) gnHistory = [...gnHistory.slice(-29), v];
  }

  function sparkPoints(arr, w, h) {
    if (arr.length < 2) return "";
    const min = Math.min(...arr);
    const max = Math.max(...arr);
    const range = max - min || 1;
    const step = w / (arr.length - 1);
    return arr.map((v, i) => {
      const x = i * step;
      const y = h - ((v - min) / range) * h;
      return `${x},${y}`;
    }).join(" ");
  }

  function fmt(v) {
    if (v == null) return null;
    return v.toFixed(1);
  }

  // Why a speed card has no number. A bare "—" is useless: the usual cause is a
  // server started without `--metrics` (the endpoint then answers 501 and this
  // build's /slots carries no timings either), and the user only sees the
  // diagnostics block in expanded mode. Derive a short reason from the
  // diagnostics the backend already sends so the compact view explains itself.
  $: diags = $state.diagnostics || [];
  $: hasDiag = (needle) =>
    diags.some((d) => typeof d === "string" && d.includes(needle));
  $: reason = hasDiag("нет --metrics")
    ? "нет --metrics"
    : hasDiag("простаивает")
      ? "нет активной генерации"
      : "нет данных";
</script>

<div class="section">
  <div class="speed-grid">
    <div class="speed-card prefill">
      <div class="speed-title">
        <span>Prefill</span>
        {#if pf.available}<span class="live-dot"></span>{/if}
      </div>
      <div class="speed-value-row">
        <div class="speed-value" class:na={!pf.available}>{pf.available ? fmt(pf.current) : "—"}</div>
        {#if pf.available}<div class="speed-unit">tok/s</div>{/if}
      </div>
      {#if !pf.available}
        <div class="speed-sub">{reason}</div>
      {/if}
      {#if pf.available && pf.avg30s != null}
        <div class="speed-avg">ср. {fmt(pf.avg30s)}</div>
      {/if}
      {#if expanded && pfHistory.length > 1}
        <svg class="sparkline" width="100%" height="24" viewBox="0 0 300 24" preserveAspectRatio="none">
          <polyline fill="none" stroke="#FB923C" stroke-width="1.5"
            points={sparkPoints(pfHistory, 300, 24)}/>
        </svg>
        {#if pf.avg30s != null}
          <div class="speed-expanded-meta">ср. 30с {fmt(pf.avg30s)}</div>
        {/if}
      {/if}
    </div>

    <div class="speed-card generation">
      <div class="speed-title">
        <span>Generation</span>
        {#if gn.available}<span class="live-dot"></span>{/if}
      </div>
      <div class="speed-value-row">
        <div class="speed-value" class:na={!gn.available}>{gn.available ? fmt(gn.current) : "—"}</div>
        {#if gn.available}<div class="speed-unit">tok/s</div>{/if}
      </div>
      {#if !gn.available}
        <div class="speed-sub">{reason}</div>
      {/if}
      {#if gn.available && gn.avg30s != null}
        <div class="speed-avg">ср. {fmt(gn.avg30s)}</div>
      {/if}
      {#if expanded && gnHistory.length > 1}
        <svg class="sparkline" width="100%" height="24" viewBox="0 0 300 24" preserveAspectRatio="none">
          <polyline fill="none" stroke="#4ADE80" stroke-width="1.5"
            points={sparkPoints(gnHistory, 300, 24)}/>
        </svg>
        {#if gn.avg30s != null}
          <div class="speed-expanded-meta">ср. 30с {fmt(gn.avg30s)}</div>
        {/if}
      {/if}
    </div>
  </div>

  {#if !pf.splitAvailable && (pf.available || gn.available)}
    <div class="note">Разделение prefill/generation недоступно</div>
  {/if}
</div>

<style>
  .section {
    padding: 10px 12px 12px;
    border-top: 1px solid rgba(255, 255, 255, .06);
  }
  :global(:root[data-theme="light"]) .section {
    border-top-color: rgba(0, 0, 0, .06);
  }

  .speed-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  :global(.widget.expanded) .speed-grid {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .speed-card {
    border-radius: 10px;
    padding: 10px;
    border: 2px solid;
    position: relative;
  }
  :global(.widget.expanded) .speed-card {
    padding: 14px;
  }

  .speed-card.prefill {
    border-color: #FB923C;
    background: rgba(251, 146, 60, .10);
  }
  :global(:root[data-theme="light"]) .speed-card.prefill {
    border-color: #EA580C;
    background: rgba(234, 88, 12, .06);
  }

  .speed-card.generation {
    border-color: #4ADE80;
    background: rgba(74, 222, 128, .10);
  }
  :global(:root[data-theme="light"]) .speed-card.generation {
    border-color: #16A34A;
    background: rgba(22, 163, 74, .06);
  }

  .speed-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: .6px;
    text-transform: uppercase;
    margin-bottom: 6px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .speed-card.prefill .speed-title { color: #FDBA74; }
  :global(:root[data-theme="light"]) .speed-card.prefill .speed-title { color: #C2410C; }

  .speed-card.generation .speed-title { color: #86EFAC; }
  :global(:root[data-theme="light"]) .speed-card.generation .speed-title { color: #15803D; }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    animation: live 1.2s infinite ease-in-out;
  }
  .speed-card.prefill .live-dot { background: #FB923C; }
  .speed-card.generation .live-dot { background: #4ADE80; }

  @keyframes live {
    0%, 100% { opacity: .3; }
    50%      { opacity: 1; }
  }

  .speed-value-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }
  :global(.widget.expanded) .speed-value-row {
    flex-direction: row;
    align-items: baseline;
    gap: 8px;
  }

  .speed-value {
    font-family: var(--font);
    font-size: 40px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    letter-spacing: -1.8px;
    line-height: 1;
    color: #F8FAFC;
    white-space: nowrap;
  }
  :global(:root[data-theme="light"]) .speed-value {
    color: #0F172A;
  }
  :global(.widget.expanded) .speed-value {
    font-size: 44px;
    letter-spacing: -2px;
  }

  /* No reading yet: keep the dash visible but clearly inactive, so "—" reads as
     "nothing to measure" rather than "broken / still loading". */
  .speed-value.na {
    color: rgba(255, 255, 255, .25);
  }
  :global(:root[data-theme="light"]) .speed-value.na {
    color: rgba(0, 0, 0, .22);
  }

  .speed-sub {
    font-size: 10px;
    line-height: 1.3;
    margin-top: 6px;
    font-style: italic;
    color: rgba(255, 255, 255, .45);
  }
  :global(:root[data-theme="light"]) .speed-sub {
    color: rgba(0, 0, 0, .5);
  }

  .speed-unit {
    font-size: 13px;
    font-weight: 500;
    line-height: 1;
  }
  :global(.widget.expanded) .speed-unit {
    font-size: 15px;
  }

  .speed-card.prefill .speed-unit { color: #FDBA74; }
  :global(:root[data-theme="light"]) .speed-card.prefill .speed-unit { color: #F97316; }
  .speed-card.generation .speed-unit { color: #86EFAC; }
  :global(:root[data-theme="light"]) .speed-card.generation .speed-unit { color: #22C55E; }

  .speed-avg {
    font-size: 11px;
    color: rgba(255, 255, 255, .45);
    margin-top: 6px;
    font-variant-numeric: tabular-nums;
  }
  :global(:root[data-theme="light"]) .speed-avg {
    color: rgba(0, 0, 0, .5);
  }

  .sparkline {
    display: none;
    margin-top: 4px;
  }
  :global(.widget.expanded) .sparkline {
    display: block;
  }

  .speed-expanded-meta {
    display: none;
    font-size: 12px;
    color: rgba(255, 255, 255, .5);
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }
  :global(.widget.expanded) .speed-expanded-meta {
    display: block;
  }
  :global(:root[data-theme="light"]) .speed-expanded-meta {
    color: rgba(0, 0, 0, .55);
  }

  .note {
    margin-top: 8px;
    font-size: 10px;
    color: rgba(255, 255, 255, .45);
    font-style: italic;
    text-align: center;
  }
  :global(:root[data-theme="light"]) .note {
    color: rgba(0, 0, 0, .5);
  }
</style>
