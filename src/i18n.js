import { writable, derived, get } from "svelte/store";

// Supported UI locales. The widget ships with Russian (the original UI) and
// English. The active locale is persisted via Settings.language (see tauriApi.js
// loadSettings/resetSettings -> syncLocaleFromSettings).
export const SUPPORTED_LOCALES = ["ru", "en"];

// Active locale store. Defaults to Russian to preserve the original behaviour
// for existing users; the Settings panel lets the user switch to English.
export const locale = writable("ru");

// Translation tables. Key sets are identical across locales. The Russian table
// MUST stay byte-for-byte equal to the original hardcoded strings so existing
// users see no regression. `t()` falls back to Russian, then to the raw key, if
// a key is missing in the active locale.
export const dictionaries = {
  ru: {
    // --- SpeedBlock ---
    reason_no_metrics: "нет --metrics",
    reason_no_generation: "нет активной генерации",
    reason_no_data: "нет данных",
    speed_avg: "ср. {v}",
    speed_avg30: "ср. 30с {v}",
    speed_split_unavailable: "Разделение prefill/generation недоступно",

    // --- ContextBar ---
    ctx_title: "Контекст",
    ctx_summary: "Всего {total} · Потрачено {used} · Осталось {remaining}",
    ctx_unknown_used: "{total} · потрачено неизвестно",
    no_data: "нет данных",
    ctx_total: "Всего",
    ctx_used: "Потрачено",
    ctx_remaining: "Осталось",
    ctx_tip: "Всего: {total} · Потрачено: {used} · Осталось: {remaining} · Заполнено: {pct}%",

    // --- Footer ---
    time_just_now: "только что",
    time_sec_ago: "{sec} сек назад",
    time_min_ago: "{min} мин назад",
    time_long_ago: "давно",
    updated: "Обновлено {t}",
    diag_source: "Источник: /metrics + /props · Ошибок: 0",
    btn_settings: "Настройки",
    btn_logs: "Логи",
    btn_minimize: "Свернуть",

    // --- Header ---
    theme_light: "Тема: светлая",
    theme_dark: "Тема: тёмная",
    mode_expanded: "Развёрнутый режим",
    mode_compact: "Компактный режим",
    title_always_on_top: "Поверх окон",
    title_to_tray: "В трей",
    title_close: "Закрыть",

    // --- ModelBlock ---
    model_none_loaded: "Нет загруженной модели",
    model_unknown: "Модель не определена",
    model_ready: "готов",
    model_ctx: "ctx {n}",

    // --- SettingsPanel ---
    kind_cloud: "Облако (OpenAI)",
    log_empty: "(лог пуст)",
    copy_empty: "Лог пуст — копировать нечего",
    copied: "Скопировано в буфер обмена",
    copy_failed: "Не удалось скопировать: {e}",
    server_new: "Новый сервер",
    server_local_default: "Локальный llama.cpp",
    scan_none: "Серверы не найдены в локальной сети.",
    scan_error: "Ошибка сканирования: {e}",
    word_settings: "Настройки",
    loading: "Загрузка настроек…",
    sec_servers: "Серверы",
    title_make_active: "Сделать активным",
    title_delete_server: "Удалить сервер",
    add_server: "+ Добавить сервер",
    scanning: "Сканирование…",
    scan_lan: "🔍 Сканировать LAN",
    found_in_net: "Найдено в сети",
    title_add_server: "Добавить сервер",
    sec_active_server: "Активный сервер",
    lbl_server_name: "Имя сервера",
    ph_default: "(по умолчанию)",
    lbl_type: "Тип",
    opt_local: "llama.cpp (локальный/удалённый)",
    lbl_url: "URL",
    hint_url: "Без суффикса /v1: нативные эндпоинты (для llama.cpp — корень; для Ollama — /api) и OpenAI-совместимый /v1 выводятся автоматически.",
    lbl_api_key: "API-ключ (опционально)",
    ph_optional: "(необязательно)",
    hint_api_key: "Не попадает в логи. Пустое поле — ключ удаляется.",
    sec_polling: "Опрос и подключение",
    lbl_poll_interval: "Интервал опроса, мс",
    lbl_timeout: "Таймаут запроса, мс",
    lbl_theme: "Тема",
    opt_auto: "Авто",
    opt_light: "Светлая",
    opt_dark: "Тёмная",
    lbl_opacity: "Прозрачность окна",
    lbl_default_compact: "Компактный режим по умолчанию",
    lbl_always_on_top: "Закрепить поверх всех окон",
    lbl_minimize_tray: "Сворачивать в трей при закрытии",
    lbl_autorun: "Запуск при старте Windows",
    lbl_show_last_update: "Показывать время последнего обновления",
    lbl_verbose_log: "Расширенное логирование",
    lbl_hotkey: "Горячая клавиша (показать/скрыть)",
    hint_hotkey: "Например: CmdOrCtrl+Shift+M. Пустое поле — отключить.",
    btn_reset: "Сбросить настройки",
    btn_refresh: "Обновить",
    btn_copy: "Копировать",
    btn_clear_logs: "Очистить логи",
    log_cleared: "(лог очищен)",
    lbl_language: "Язык",
    opt_lang_ru: "Русский",
    opt_lang_en: "English",

    // --- main.js (fatal / banner) ---
    fatal_title: "Ошибка запуска интерфейса",
    banner_dismiss: "Клик — скрыть",
    err_event: "событие error",
  },

  en: {
    // --- SpeedBlock ---
    reason_no_metrics: "no --metrics",
    reason_no_generation: "no active generation",
    reason_no_data: "no data",
    speed_avg: "avg {v}",
    speed_avg30: "avg 30s {v}",
    speed_split_unavailable: "prefill/generation split unavailable",

    // --- ContextBar ---
    ctx_title: "Context",
    ctx_summary: "Total {total} · Used {used} · Free {remaining}",
    ctx_unknown_used: "{total} · used unknown",
    no_data: "no data",
    ctx_total: "Total",
    ctx_used: "Used",
    ctx_remaining: "Remaining",
    ctx_tip: "Total: {total} · Used: {used} · Remaining: {remaining} · Filled: {pct}%",

    // --- Footer ---
    time_just_now: "just now",
    time_sec_ago: "{sec}s ago",
    time_min_ago: "{min}m ago",
    time_long_ago: "long ago",
    updated: "Updated {t}",
    diag_source: "Source: /metrics + /props · Errors: 0",
    btn_settings: "Settings",
    btn_logs: "Logs",
    btn_minimize: "Minimize",

    // --- Header ---
    theme_light: "Theme: light",
    theme_dark: "Theme: dark",
    mode_expanded: "Expanded mode",
    mode_compact: "Compact mode",
    title_always_on_top: "Always on top",
    title_to_tray: "To tray",
    title_close: "Close",

    // --- ModelBlock ---
    model_none_loaded: "No model loaded",
    model_unknown: "Model not detected",
    model_ready: "ready",
    model_ctx: "ctx {n}",

    // --- SettingsPanel ---
    kind_cloud: "Cloud (OpenAI)",
    log_empty: "(log empty)",
    copy_empty: "Log empty — nothing to copy",
    copied: "Copied to clipboard",
    copy_failed: "Copy failed: {e}",
    server_new: "New server",
    server_local_default: "Local llama.cpp",
    scan_none: "No servers found on the local network.",
    scan_error: "Scan error: {e}",
    word_settings: "Settings",
    loading: "Loading settings…",
    sec_servers: "Servers",
    title_make_active: "Make active",
    title_delete_server: "Delete server",
    add_server: "+ Add server",
    scanning: "Scanning…",
    scan_lan: "🔍 Scan LAN",
    found_in_net: "Found on network",
    title_add_server: "Add server",
    sec_active_server: "Active server",
    lbl_server_name: "Server name",
    ph_default: "(default)",
    lbl_type: "Type",
    opt_local: "llama.cpp (local/remote)",
    lbl_url: "URL",
    hint_url: "No /v1 suffix: native endpoints (llama.cpp root; Ollama /api) and the OpenAI-compatible /v1 are detected automatically.",
    lbl_api_key: "API key (optional)",
    ph_optional: "(optional)",
    hint_api_key: "Not written to logs. Empty field removes the key.",
    sec_polling: "Polling & connection",
    lbl_poll_interval: "Poll interval, ms",
    lbl_timeout: "Request timeout, ms",
    lbl_theme: "Theme",
    opt_auto: "Auto",
    opt_light: "Light",
    opt_dark: "Dark",
    lbl_opacity: "Window opacity",
    lbl_default_compact: "Compact mode by default",
    lbl_always_on_top: "Always on top",
    lbl_minimize_tray: "Minimize to tray on close",
    lbl_autorun: "Start with Windows",
    lbl_show_last_update: "Show last update time",
    lbl_verbose_log: "Verbose logging",
    lbl_hotkey: "Hotkey (show/hide)",
    hint_hotkey: "Example: CmdOrCtrl+Shift+M. Empty field disables it.",
    btn_reset: "Reset settings",
    btn_refresh: "Refresh",
    btn_copy: "Copy",
    btn_clear_logs: "Clear logs",
    log_cleared: "(log cleared)",
    lbl_language: "Language",
    opt_lang_ru: "Русский",
    opt_lang_en: "English",

    // --- main.js (fatal / banner) ---
    fatal_title: "Interface startup error",
    banner_dismiss: "Click to dismiss",
    err_event: "error event",
  },
};

function resolve(key) {
  const dict = dictionaries[get(locale)] || dictionaries.ru;
  if (Object.prototype.hasOwnProperty.call(dict, key)) return dict[key];
  if (Object.prototype.hasOwnProperty.call(dictionaries.ru, key)) return dictionaries.ru[key];
  return key;
}

// `t` is a derived store: `{$t('key')}` in markup and `$t('key')` in script both
// re-render whenever `locale` changes. The stored value is a translator
// function so it can take interpolation vars: `$t('ctx_summary', {total, used})`.
export const t = derived(locale, () => (key, vars) => {
  let s = resolve(key);
  if (vars && typeof s === "string") {
    s = s.replace(/\{(\w+)\}/g, (_, name) => (vars[name] != null ? String(vars[name]) : `{${name}}`));
  }
  return s;
});

export function setLocale(lang) {
  if (SUPPORTED_LOCALES.includes(lang)) locale.set(lang);
}

// Keep the UI locale in sync with the persisted Settings.language field. Called
// from tauriApi.js after loadSettings()/resetSettings() resolve.
export function syncLocaleFromSettings(lang) {
  if (lang && SUPPORTED_LOCALES.includes(lang)) locale.set(lang);
}

// Locale-aware integer formatting (thousands separators). Reads the current
// locale; components re-render on locale change because they also use `$t`.
export function formatInt(n) {
  if (n == null) return "N/A";
  const l = get(locale) === "en" ? "en-US" : "ru-RU";
  return n.toLocaleString(l);
}
