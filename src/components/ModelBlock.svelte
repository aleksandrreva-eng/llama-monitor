<script>
  import { state } from "../store";
  import { t, formatInt } from "../i18n";
  import Section from "./Section.svelte";

  export let expanded = false;

  $: m = $state.model;

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

<Section>
  <div class="model-name">{displayName}</div>
  {#if expanded}
    <div class="model-meta">{metaParts.join(" · ") || "—"}</div>
    {#if displayPath}
      <div class="model-path">{displayPath}</div>
    {/if}
  {/if}
</Section>

<style>
  .model-name {
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-strong);
  }

  .model-meta {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }

  .model-path {
    font-size: 11px;
    color: var(--text-path);
    margin-top: 2px;
    font-family: var(--font-mono);
  }
</style>
