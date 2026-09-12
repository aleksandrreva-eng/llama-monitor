<script>
  import { state } from "../store";
  import { t, formatInt } from "../i18n";

  export let expanded = false;

  $: s = $state;
  $: m = s.model;

  // Show only the model's own name — strip any path prefix so a full path
  // (e.g. when name falls back to model_path) renders as just the filename.
  $: displayName = m.name
    ? m.name.replace(/.*[/\\]/, "")
    : m.loaded
      ? $t("model_none_loaded")
      : $t("model_unknown");

  // The full path, kept for reference below the name.
  $: displayPath = m.path || (m.name && (m.name.match(/^(.*[/\\])/) || ["", ""])[1]);

  $: metaParts = [
    m.contextSize ? $t("model_ctx", { n: formatInt(m.contextSize) }) : null,
    m.quantization ? m.quantization : null,
    m.loaded ? $t("model_ready") : null,
  ].filter(Boolean);
</script>

<div class="section">
  <div class="model-name">{displayName}</div>
  {#if expanded}
    <div class="model-meta">{metaParts.join(" · ") || "—"}</div>
    {#if displayPath}
      <div class="model-path">{displayPath}</div>
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
