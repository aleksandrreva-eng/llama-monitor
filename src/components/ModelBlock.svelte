<script>
  import { state } from "../store";

  export let expanded = false;

  $: s = $state;
  $: m = s.model;

  $: displayName = m.name
    ? m.name
    : m.loaded
      ? "Нет загруженной модели"
      : "Модель не определена";

  $: metaParts = [
    m.contextSize ? `ctx ${m.contextSize.toLocaleString("ru-RU")}` : null,
    m.quantization ? m.quantization : null,
    m.loaded ? "готов" : null,
  ].filter(Boolean);
</script>

<div class="section">
  <div class="model-name">{displayName}</div>
  {#if expanded}
    <div class="model-meta">{metaParts.join(" · ") || "—"}</div>
    {#if m.path}
      <div class="model-path">{m.path}</div>
    {/if}
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

  .model-name {
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: #fff;
  }
  :global(:root[data-theme="light"]) .model-name {
    color: #1a1a1a;
  }

  .model-meta {
    font-size: 11px;
    color: rgba(255, 255, 255, .5);
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }
  :global(:root[data-theme="light"]) .model-meta {
    color: rgba(0, 0, 0, .5);
  }

  .model-path {
    font-size: 11px;
    color: rgba(255, 255, 255, .35);
    margin-top: 2px;
    font-family: "Cascadia Code", Consolas, monospace;
  }
  :global(:root[data-theme="light"]) .model-path {
    color: rgba(0, 0, 0, .4);
  }
</style>
