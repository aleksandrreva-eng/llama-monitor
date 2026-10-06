<script>
  import { state, settings } from "../store";
  import { t } from "../i18n";
  import { hasDiag } from "../lib/diagnostics";
  import Section from "./Section.svelte";

  export let expanded = false;

  $: s = $state;
  $: pf = s.prefillSpeed;
  $: gn = s.generationSpeed;

  let pfHistory = [];
  let gnHistory = [];

  // Drop the sparkline history when the monitored server changes. The Rust side
  // resets its averaging window on a server switch, but the history lived only
  // in this component — so the graph used to splice the previous upstream's
  // speeds onto the new one's. `active_server_id` flips the moment the user
  // picks a server, i.e. before the first poll of the new one arrives.
  //
  // The previous key is kept in a plain object rather than a `let`: a reactive
  // block that both reads and writes a tracked variable is a self-cycle, which
  // Svelte reports as a "cyclical dependency".
  const tracker = { server: null };
  $: {
    const key = `${$settings?.active_server_id ?? ""}|${$state.serverLabel ?? ""}`;
    if (key !== tracker.server) {
      tracker.server = key;
      pfHistory = [];
      gnHistory = [];
    }
  }

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
    return arr
      .map((v, i) => {
        const x = i * step;
        const y = h - ((v - min) / range) * h;
        return `${x},${y}`;
      })
      .join(" ");
  }

  function fmt(v) {
    if (v == null) return null;
    return v.toFixed(1);
  }

  // Why a speed card has no number. A bare "—" is useless: the usual cause is a
  // server started without `--metrics` (the endpoint then answers 501 and this
  // build's /slots carries no timings either), and the user only sees the
  // diagnostics block in expanded mode. The reason is derived from the
  // diagnostic *codes* the backend sends — never from matching on localized
  // prose, which broke as soon as the locale was switched to English.
  $: diags = $state.diagnostics;
  $: reason = hasDiag(diags, "metrics_disabled")
    ? $t("reason_no_metrics")
    : hasDiag(diags, "server_idle")
      ? $t("reason_no_generation")
      : $t("reason_no_data");
</script>

<Section>
  <div class="speed-grid">
    <div class="speed-card prefill">
      <div class="speed-title">
        <span>Prefill</span>
        {#if pf.available}<span class="live-dot"></span>{/if}
      </div>
      <div class="speed-value-row">
        <div class="speed-value" class:na={!pf.available}>
          {pf.available ? fmt(pf.current) : "—"}
        </div>
        {#if pf.available}<div class="speed-unit">tok/s</div>{/if}
      </div>
      {#if !pf.available}
        <div class="speed-sub">{reason}</div>
      {/if}
      {#if pf.available && pf.avg30s != null}
        <div class="speed-avg">{$t("speed_avg", { v: fmt(pf.avg30s) })}</div>
      {/if}
      {#if expanded && pfHistory.length > 1}
        <svg
          class="sparkline"
          width="100%"
          height="24"
          viewBox="0 0 300 24"
          preserveAspectRatio="none"
        >
          <polyline
            fill="none"
            style="stroke: var(--prefill)"
            stroke-width="1.5"
            points={sparkPoints(pfHistory, 300, 24)}
          />
        </svg>
        {#if pf.avg30s != null}
          <div class="speed-expanded-meta">{$t("speed_avg30", { v: fmt(pf.avg30s) })}</div>
        {/if}
      {/if}
    </div>

    <div class="speed-card generation">
      <div class="speed-title">
        <span>Generation</span>
        {#if gn.available}<span class="live-dot"></span>{/if}
      </div>
      <div class="speed-value-row">
        <div class="speed-value" class:na={!gn.available}>
          {gn.available ? fmt(gn.current) : "—"}
        </div>
        {#if gn.available}<div class="speed-unit">tok/s</div>{/if}
      </div>
      {#if !gn.available}
        <div class="speed-sub">{reason}</div>
      {/if}
      {#if gn.available && gn.avg30s != null}
        <div class="speed-avg">{$t("speed_avg", { v: fmt(gn.avg30s) })}</div>
      {/if}
      {#if expanded && gnHistory.length > 1}
        <svg
          class="sparkline"
          width="100%"
          height="24"
          viewBox="0 0 300 24"
          preserveAspectRatio="none"
        >
          <polyline
            fill="none"
            style="stroke: var(--generation)"
            stroke-width="1.5"
            points={sparkPoints(gnHistory, 300, 24)}
          />
        </svg>
        {#if gn.avg30s != null}
          <div class="speed-expanded-meta">{$t("speed_avg30", { v: fmt(gn.avg30s) })}</div>
        {/if}
      {/if}
    </div>
  </div>

  {#if !pf.splitAvailable && (pf.available || gn.available)}
    <div class="note">{$t("speed_split_unavailable")}</div>
  {/if}
</Section>

<style>
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
    border-radius: var(--radius-md);
    padding: 10px;
    border: 2px solid;
    position: relative;
  }
  :global(.widget.expanded) .speed-card {
    padding: 14px;
  }

  .speed-card.prefill {
    border-color: var(--prefill);
    background: var(--prefill-bg);
  }

  .speed-card.generation {
    border-color: var(--generation);
    background: var(--generation-bg);
  }

  .speed-title {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    margin-bottom: 6px;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .speed-card.prefill .speed-title {
    color: var(--prefill-title);
  }
  .speed-card.generation .speed-title {
    color: var(--generation-title);
  }

  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    animation: live 1.2s infinite ease-in-out;
  }
  .speed-card.prefill .live-dot {
    background: var(--prefill);
  }
  .speed-card.generation .live-dot {
    background: var(--generation);
  }

  @keyframes live {
    0%,
    100% {
      opacity: 0.3;
    }
    50% {
      opacity: 1;
    }
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
    color: var(--text-strong);
    white-space: nowrap;
  }
  :global(.widget.expanded) .speed-value {
    font-size: 44px;
    letter-spacing: -2px;
  }

  /* No reading yet: keep the dash visible but clearly inactive, so "—" reads as
     "nothing to measure" rather than "broken / still loading". */
  .speed-value.na {
    color: var(--text-ghost);
  }

  .speed-sub {
    font-size: 10px;
    line-height: 1.3;
    margin-top: 6px;
    font-style: italic;
    color: var(--text-faint);
  }

  .speed-unit {
    font-size: 13px;
    font-weight: 500;
    line-height: 1;
  }
  :global(.widget.expanded) .speed-unit {
    font-size: 15px;
  }

  .speed-card.prefill .speed-unit {
    color: var(--prefill-unit);
  }
  .speed-card.generation .speed-unit {
    color: var(--generation-unit);
  }

  .speed-avg {
    font-size: 11px;
    color: var(--text-faint);
    margin-top: 6px;
    font-variant-numeric: tabular-nums;
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
    color: var(--text-muted);
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }
  :global(.widget.expanded) .speed-expanded-meta {
    display: block;
  }

  .note {
    margin-top: 8px;
    font-size: 10px;
    color: var(--text-faint);
    font-style: italic;
    text-align: center;
  }
</style>
