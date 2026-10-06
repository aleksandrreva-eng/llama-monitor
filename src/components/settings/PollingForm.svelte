<script>
  import { settings } from "../../store";
  import { t } from "../../i18n";

  /** Debounced edit — text/number/range fields. */
  export let onEdit;
  /** Immediate edit — checkboxes and dropdowns. */
  export let onCommit;
  /** Language change needs the locale switched before the save round-trips. */
  export let onLanguage;

  // Thresholds are stored as 0..1 ratios but shown as percentages, so they need
  // a local mirror. Re-synced whenever a new settings object arrives (i.e. after
  // each save), which also pulls back whatever the backend clamped. The last
  // seen object is held in a plain container so this block has no self-cycle.
  const seen = { settings: null };
  let warnPct = 75;
  let critPct = 90;
  $: if ($settings && $settings !== seen.settings) {
    seen.settings = $settings;
    warnPct = Math.round(($settings.warning_threshold ?? 0.75) * 100);
    critPct = Math.round(($settings.critical_threshold ?? 0.9) * 100);
  }
</script>

<section class="section">
  <div class="section-title">{$t("sec_polling")}</div>
  <div class="field-grid">
    <label class="field">
      <span>{$t("lbl_poll_interval")}</span>
      <input
        type="number"
        bind:value={$settings.poll_interval_ms}
        on:input={() => onEdit({ poll_interval_ms: $settings.poll_interval_ms })}
      />
    </label>

    <label class="field">
      <span>{$t("lbl_timeout")}</span>
      <input
        type="number"
        bind:value={$settings.request_timeout_ms}
        on:input={() => onEdit({ request_timeout_ms: $settings.request_timeout_ms })}
      />
    </label>

    <label class="field">
      <span>{$t("lbl_smoothing")}</span>
      <input
        type="number"
        bind:value={$settings.smoothing_window_secs}
        on:input={() => onEdit({ smoothing_window_secs: $settings.smoothing_window_secs })}
      />
    </label>

    <label class="field">
      <span>{$t("lbl_language")}</span>
      <select value={$settings.language} on:change={(e) => onLanguage(e.target.value)}>
        <option value="ru">{$t("opt_lang_ru")}</option>
        <option value="en">{$t("opt_lang_en")}</option>
      </select>
    </label>

    <label class="field">
      <span>{$t("lbl_warn_threshold")}</span>
      <input
        type="number"
        min="0"
        max="100"
        bind:value={warnPct}
        on:input={() => onEdit({ warning_threshold: (Number(warnPct) || 0) / 100 })}
      />
    </label>

    <label class="field">
      <span>{$t("lbl_critical_threshold")}</span>
      <input
        type="number"
        min="0"
        max="100"
        bind:value={critPct}
        on:input={() => onEdit({ critical_threshold: (Number(critPct) || 0) / 100 })}
      />
    </label>

    <label class="field">
      <span>{$t("lbl_theme")}</span>
      <select bind:value={$settings.theme} on:change={() => onCommit({ theme: $settings.theme })}>
        <option value="auto">{$t("opt_auto")}</option>
        <option value="light">{$t("opt_light")}</option>
        <option value="dark">{$t("opt_dark")}</option>
      </select>
    </label>

    <label class="field">
      <span>{$t("lbl_opacity")}</span>
      <input
        type="range"
        min="0.5"
        max="1"
        step="0.05"
        bind:value={$settings.window_opacity}
        on:input={() => onEdit({ window_opacity: $settings.window_opacity })}
      />
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.default_compact}
        on:change={() => onCommit({ default_compact: $settings.default_compact })}
      />
      <span>{$t("lbl_default_compact")}</span>
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.always_on_top}
        on:change={() => onCommit({ always_on_top: $settings.always_on_top })}
      />
      <span>{$t("lbl_always_on_top")}</span>
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.minimize_to_tray}
        on:change={() => onCommit({ minimize_to_tray: $settings.minimize_to_tray })}
      />
      <span>{$t("lbl_minimize_tray")}</span>
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.autorun}
        on:change={() => onCommit({ autorun: $settings.autorun })}
      />
      <span>{$t("lbl_autorun")}</span>
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.show_last_update}
        on:change={() => onCommit({ show_last_update: $settings.show_last_update })}
      />
      <span>{$t("lbl_show_last_update")}</span>
    </label>

    <label class="field switch">
      <input
        type="checkbox"
        bind:checked={$settings.verbose_logging}
        on:change={() => onCommit({ verbose_logging: $settings.verbose_logging })}
      />
      <span>{$t("lbl_verbose_log")}</span>
    </label>

    <label class="field wide">
      <span>{$t("lbl_hotkey")}</span>
      <!-- Saved on `change` (blur / Enter), never per keystroke: registering a
           global shortcut per character failed on every intermediate value and
           buried the log in warnings. -->
      <input
        value={$settings.hotkey || ""}
        placeholder="CmdOrCtrl+Shift+M"
        on:change={(e) => onCommit({ hotkey: e.target.value || null })}
      />
      <span class="hint">{$t("hint_hotkey")}</span>
    </label>
  </div>
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
</style>
