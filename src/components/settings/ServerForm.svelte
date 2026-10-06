<script>
  import { t } from "../../i18n";

  /** The profile being edited. */
  export let profile = null;
  /** Debounced edit — used for free-text fields (saved once typing stops). */
  export let onEdit;
  /** Immediate edit — used for dropdowns (a single click is a decision). */
  export let onCommit;

  // API keys arrive masked ("***") from the backend. The field shows whatever
  // was stored; clearing it sends null, which the backend treats as "no key".
  function apiKeyDisplay(key) {
    return key == null ? "" : key;
  }

  function onApiKey(value) {
    onEdit({ api_key: value || null });
  }
</script>

{#if profile}
  <section class="section">
    <div class="section-title">{$t("sec_active_server")}</div>
    <div class="field-grid">
      <label class="field">
        <span>{$t("lbl_server_name")}</span>
        <input
          value={profile.label}
          on:input={(e) => onEdit({ label: e.target.value })}
          placeholder={$t("ph_default")}
        />
      </label>

      <label class="field">
        <span>{$t("lbl_type")}</span>
        <select value={profile.kind} on:change={(e) => onCommit({ kind: e.target.value })}>
          <option value="local">{$t("opt_local")}</option>
          <option value="vllm">vLLM</option>
          <option value="ollama">Ollama</option>
          <option value="cloud">{$t("kind_cloud")}</option>
        </select>
      </label>

      <label class="field wide">
        <span>{$t("lbl_url")}</span>
        <input
          value={profile.url}
          on:input={(e) => onEdit({ url: e.target.value })}
          placeholder="http://127.0.0.1:8080"
        />
        <small class="hint">{$t("hint_url")}</small>
      </label>

      <label class="field wide">
        <span>{$t("lbl_api_key")}</span>
        <input
          type="password"
          value={apiKeyDisplay(profile.api_key)}
          on:input={(e) => onApiKey(e.target.value)}
          placeholder={$t("ph_optional")}
        />
        <small class="hint">{$t("hint_api_key")}</small>
      </label>
    </div>
  </section>
{/if}

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
</style>
