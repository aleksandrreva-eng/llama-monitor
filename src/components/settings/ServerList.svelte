<script>
  import { t } from "../../i18n";

  /** Stored server profiles. */
  export let servers = [];
  /** Id of the profile the widget currently monitors. */
  export let activeId = null;
  export let onSelect;
  export let onRemove;
  export let onAdd;

  const KIND_LABELS = { local: "llama.cpp", vllm: "vLLM", ollama: "Ollama" };

  function kindLabel(kind) {
    if (kind === "cloud") return $t("kind_cloud");
    return KIND_LABELS[kind] || kind;
  }
</script>

<section class="section">
  <div class="section-title">{$t("sec_servers")}</div>
  <div class="server-list">
    {#each servers as p (p.id)}
      <div class="server-row" class:active={p.id === activeId}>
        <button
          class="server-select"
          on:click={() => onSelect(p.id)}
          title={$t("title_make_active")}
        >
          <span class="server-name">{p.label || p.url}</span>
          <span class="server-kind">{kindLabel(p.kind)}</span>
        </button>
        <button class="server-del" title={$t("title_delete_server")} on:click={() => onRemove(p.id)}
          >✕</button
        >
      </div>
    {/each}
  </div>
  <button class="btn block dashed" on:click={onAdd}>+ {$t("add_server")}</button>
</section>

<style>
  .section {
    margin-bottom: 14px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-hint);
    margin-bottom: 8px;
  }

  .server-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 8px;
  }

  .server-row {
    display: flex;
    align-items: stretch;
    gap: 6px;
  }

  .server-select {
    flex: 1;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    color: var(--text-primary);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }

  .server-row.active .server-select {
    border-color: var(--accent, #0078d4);
    background: color-mix(in srgb, var(--accent, #0078d4) 12%, var(--bg-primary));
  }

  .server-name {
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

  .server-del {
    flex-shrink: 0;
    width: 30px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .server-del:hover {
    border-color: var(--bad);
    color: var(--bad);
  }
</style>
