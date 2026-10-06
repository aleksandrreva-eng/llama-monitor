# llama.cpp Monitor — дизайн и реализация

Плавающее десктопное приложение-виджет для Windows 11, отображающее состояние локального или сетевого сервера llama.cpp: контекст, скорости инференса, загруженную модель и статус подключения.

> Статус: **реализовано**. Документ обновлён под фактический код (`v0.1.x`).
> Разделы 1 и 6 — исходное проектирование, оставлены как есть; разделы 2–5 и 7
> приведены в соответствие с реализацией.
> Контракт обмена с фронтендом вынесен отдельно: `docs/wire-contract.md`.

---

## 1. Выбор технологического стека

### Вариант А — Tauri 2 + Svelte (РЕКОМЕНДОВАН)

| Плюс | Минус |
|------|-------|
| Лёгкий (~5–10 МБ бинарь, без Electron runtime) | Нужен WebView2 (есть на Win11 по умолчанию) |
| Современный UI на веб-технологиях | Сложнее нативный трей/always-on-top, чем в WPF |
| Rust-фонд для сети/парсинга, многопоточность | Зависимость от версии WebView2 |
| Быстрая разработка UI (Svelte) | — |
| Легко парсить JSON в Rust | — |

**Итог:** лучшее соотношение «лёгкий + красивый UI + надёжность». Идеально под требования (плавающее окно, трей, фоновая работа, Win11-стиль).

### Вариант Б — .NET 8 + WPF

| Плюс | Минус |
|------|-------|
| Полностью нативно, отличный трей/always-on-top | Нужно писать UI-компоненты вручную |
| Хорошая производительность | Меньше «вау»-эффекта WinUI без больших усилий |
| Удобный HttpClient, System.Text.Json | — |
| Простое развёртывание (single-file) | WPF-стиль Win11 требует усилий по кастомизации |

**Итог:** самый надёжный для системных фич (трей, автозапуск, DPI). UI чуть «техничнее».

### Вариант В — Python + PySide6 (MVP)

| Плюс | Минус |
|------|-------|
| Быстрый прототип | Python-зависимости у конечного пользователя |
| Qt-трей/окна из коробки | Тяжелее по памяти, сложнее упаковка |

**Итог:** только для быстрого прототипа.

### Рекомендация

**Вариант А (Tauri 2 + Svelte)** как основной. Если критична максимальная нативность трей/автозапуска и не нужны веб-технологии — **Вариант Б (WPF)** как запасной.

---

## 2. Высокоуровневая архитектура

```
┌─────────────────────────────────────────────────────────┐
│                        UI (Svelte)                        │
│  Компактный / Развёрнутый режим · темы · перетаскивание   │
│  Tray · Always-on-top · DPI · настройки                   │
└───────────────────────┬─────────────────────────────────┘
                        │ store.js (`monitoring:update`)
┌───────────────────────▼─────────────────────────────────┐
│                  State Management (store.js)              │
│  MonitoringState (context, speeds, model, diagnostics)    │
│  mergeState() добивает недостающие ключи; UI не ходит     │
│  в сеть напрямую.                                         │
└───────────────────────┬─────────────────────────────────┘
                        │ Tauri event
┌───────────────────────▼─────────────────────────────────┐
│              Monitoring Service (monitoring.rs)           │
│  loop { sleep(backoff_delay) → poll → parse → emit }      │
│  Живые настройки читаются на каждом тике.                 │
└───────────────────────┬─────────────────────────────────┘
                        │
┌───────────────────────▼─────────────────────────────────┐
│              Data Adapters (api/adapters/)                │
│  trait Adapter { probes() · parse() }                     │
│  llamacpp · vllm · ollama · cloud; probes.rs — HTTP,      │
│  api/parse/ — чистые парсеры без I/O                      │
└─────────────────────────────────────────────────────────┘
        │            │            │
        ▼            ▼            ▼
┌────────────┐ ┌───────────┐ ┌──────────┐
│Calculator  ││Config Svc  ││ Logging  │
│ (metrics)  ││ (JSON)     ││ (local)  │
└────────────┘ └───────────┘ └──────────┘
```

### Поток данных
1. `Monitoring Service` по таймеру (`loop { sleep(delay) → poll }`) вызывает HTTP-эндпоинты через `Data Adapters`.
2. Адаптеры парсят JSON → сырые DTO (`ParsedSnapshot`), парсинг чистый и без I/O.
3. `Metrics Calculator` превращает DTO в `MonitoringState` (контекст, скорости со скользящим средним, модель, диагностики).
4. `State` обновляется → UI перерисовывается. UI никогда не знает про сеть.
5. Ошибка не роняет приложение: `degraded_state()` сохраняет последние известные значения, меняет статус на «сервер недоступен»/«устарело» и добавляет диагностику `poll_failed`. Экспоненциальный backoff (×2 за неудачу, потолок 4× интервала) замедляет опрос мёртвого сервера.
6. `Config Service` читает/пишет `settings.json` (атомарно: temp + rename). `Logging Service` пишет локальные логи (без секретов) с лимитом размера файла.

---

## 3. Список модулей

| Модуль | Файл | Ответственность |
|--------|------|-----------------|
| **UI Layer** | `src/App.svelte`, `src/components/` | Отображение состояния. Никакой сети. Компоненты: Header, ContextBar, SpeedBlock, ModelBlock, MetricsBlock, Footer, SettingsPanel (+ `components/settings/*`). |
| **State Management** | `src/store.js` | `MonitoringState` store, `mergeState()`, реактивная передача в UI. |
| **Monitoring Service** | `src-tauri/src/monitoring.rs` | Цикл опроса, живые настройки, backoff, деградация. |
| **Data Adapters** | `src-tauri/src/api/adapters/` | `trait Adapter` + реализации на движок: llamacpp, vllm, ollama, cloud. |
| **HTTP-слой проб** | `src-tauri/src/api/probes.rs` | Клиент, авторизация, классификация исходов (`Ok`/`Status`/`Transport`). |
| **Парсеры** | `src-tauri/src/api/parse/` | Чистые функции без I/O: prometheus, metrics, model, slots. |
| **Metrics Calculator** | `src-tauri/src/api/calculator.rs` | Контекст, prefill/generation speed (скользящее окно), статусы, диагностики. |
| **Configuration Service** | `src-tauri/src/config.rs` | Загрузка/сохранение/применение настроек, миграция, `sanitize()`. |
| **Logging Service** | `src-tauri/src/logging.rs` | Локальные логи диагностики, лимит размера. |
| **Sync helpers** | `src-tauri/src/sync.rs` | Блокировки, устойчивые к poison (паника в критической секции не убивает мониторинг). |
| **Window/OS Service** | `src-tauri/src/main.rs`, `api/mod.rs`, `api/lan.rs` | Tray, always-on-top, drag, DPI, автозапуск, позиция/размер, скан LAN. |

---

## 4. Структура данных состояния

```ts
interface MonitoringState {
  connection: ConnectionStatus;      // 'connected' | 'connecting' | 'disconnected'
                                       // | 'error' | 'stale' | 'server_unavailable'
                                       // | 'metrics_unavailable'
  lastUpdate: Date | null;           // время последнего успешного обновления
  serverLabel: string;               // короткое имя/адрес сервера

  context: ContextMetric;
  prefillSpeed: SpeedMetric;         // tokens/sec
  generationSpeed: SpeedMetric;

  model: ModelMetric;

  otherMetrics: RawMetric[];        // сырые счётчики /metrics для развёрнутого вида
  diagnostics: Diag[];              // { code, detail? } — коды, не текст
}

interface RawMetric {
  name: string;                     // без префикса `llamacpp:`
  value: number;
}

interface Diag {
  code: string;                     // ключ i18n = `diag_<code>`
  detail?: string;                  // только непереводимое (текст ошибки ОС/сети)
}

interface ContextMetric {
  total: number | null;
  used: number | null;             // null = неизвестно (не '0')
  remaining: number | null;
  percent: number | null;          // 0..1
  available: boolean;              // false → «нет данных»
}

interface SpeedMetric {
  current: number | null;          // tokens/sec
  avg30s: number | null;           // скользящее среднее
  available: boolean;
  splitAvailable: boolean;         // true, если prefill≠generation
}

interface ModelMetric {
  name: string | null;             // null → «Модель не определена»
  contextSize: number | null;
  loaded: boolean;                 // false → «Нет загруженной модели»
  quantization: string | null;
  path: string | null;
  version: string | null;
}
```

**Принципы модели:**
- `null` = «неизвестно/недоступно», никогда не показываем как `0`.
- `available: false` → UI рисует нейтральный/серый вид, «N/A».
- Отрицательный remaining → clamp к 0.
- `percent: null` при известном `total` и неизвестном `used` — UI обязан показать «потрачено неизвестно», а не 0 %.

Полное описание полезной нагрузки и правил её именования: `docs/wire-contract.md`.

---

## 5. Endpoints и fallback-источники

Актуальные наборы эндпоинтов — в `src-tauri/src/api/adapters/`.

| Поле | llama.cpp | vLLM | Ollama | Cloud |
|------|-----------|------|--------|-------|
| Доступность (обязательная проба) | `/health` | `/health` | `/api/ps` | `/v1/models` |
| Контекст | `/metrics`, иначе `/slots` | `/metrics` (KV-cache) | `/api/ps` (`context_length`) | — |
| Скорости | `/metrics` или `timings` из `/slots` | `/metrics` | — | — |
| Имя модели | `/props`, `/v1/models` | `/v1/models` | `/api/ps` | `/v1/models` |
| Quantization / ctx | `/props` | `/v1/models` | `/api/ps` | `/v1/models` |

`/metrics` у llama.cpp требует запуска с `--metrics`; без него эндпоинт отвечает
501, и виджет показывает подсказку вместо скоростей. `/slots` работает по
умолчанию и остаётся источником живого контекста.

**Поведение fallback:** отсутствие опционального эндпоинта → поле помечается
`available: false` и добавляется диагностика (только на определённый 404/501 —
транзиентные ошибки молчат). Отказ **обязательной** пробы → весь опрос
считается неуспешным, состояние деградирует.

---

## 6. UX-описание окна

**Заголовок (drag-область):** имя модели + индикатор статуса (цветной dot).
- 🟢 connected · 🟡 connecting/stale · 🔴 disconnected/error · ⚪ no-data

**Блок «Контекст»:**
- Текст: `12 288 / 32 768 · 11 480 ост.`
- Прогресс-бар: градиент зелёный→жёлтый→оранжевый→красный по `percent`, плавный.
- Нет данных → серый бар, «нет данных».
- Tooltip: точные значения.

**Блок «Скорости»:**
- `Prefill: 42 tok/s (avg 38)` · `Generation: 28 tok/s (avg 30)`.
- Нет данных → «N/A» / «нет активной генерации».
- Скользящее среднее за 30 с (настраиваемо).

**Блок «Модель»:**
- `llama-3.1-8B · Q4_K_M · 32k context`.
- Нет модели → «Модель не определена» / «Нет загруженной модели».

**Подвал:** время последнего обновления · кнопки: ⚙️ настройки · ⤓ трей · 📌 always-on-top.

**Режимы:** компактный (только ключевые значения) и развёрнутый (подсказки, статусы, время).

---

## 7. Этапы реализации

Все этапы закрыты (v0.1.x):

1. ~~Настройка проекта~~ — Tauri 2 + Svelte, `cargo`/`tauri.conf`.
2. ~~State + UI-каркас~~ — store, компоненты, темы.
3. ~~Config Service~~ — `settings.json` в `%LOCALAPPDATA%`, дефолты, сброс, миграция.
4. ~~Data Adapters + Calculator~~ — `trait Adapter` на каждый движок, чистые парсеры, расчёт метрик.
5. ~~Monitoring Service~~ — цикл опроса, живые настройки, обновление state.
6. ~~Window/OS~~ — tray, always-on-top, drag, DPI, автозапуск, позиция/размер.
7. ~~Error handling & degradation~~ — статусы, заглушки, экспоненциальный backoff, устойчивые блокировки.
8. ~~Logging~~ — локальные логи, защита секретов, лимит размера.
9. ~~Тестирование~~ — 103 unit-теста в `src-tauri` + два статических чека фронтенда.
10. ~~Документация~~ — этот документ, `wire-contract.md`, `PRODUCTION_DOCS.md`.

Осталось непокрытым автоматическими тестами: UI (Svelte-компоненты) — только
статический анализ (`check:stores`, `check:i18n`) и ручной smoke.

---

## 8. Риски и ограничения

- **Разный формат метрик** у llama.cpp → нужны адаптеры и тщательные fallback'и.
- **Prefill vs generation** не всегда разделяется → помечаем «разделение недоступно».
- **WebView2** уже на Win11, но на старых/корневых сборках может отсутствовать → проверка + подсказка.
- **Трей/автозапуск** — системные фичи, требуют аккуратной обработки событий сна/разбуждения.
- **DPI/мониторы** — пересчёт позиции при смене.
- **Данные устарели** — таймаут «свежести» (stale) на основе lastUpdate.
- **Секреты** — API-ключи не в логах, хранение в settings с пометкой.

---

## 9. Настройки (сводка)

Фактический состав `Settings` (все поля видны в панели настроек и используются):

- **Серверы**: список профилей (`servers`), активный профиль (`active_server_id`),
  у каждого — имя, URL без `/v1`, тип (`local`/`vllm`/`ollama`/`cloud`), API-ключ.
- **Опрос**: `poll_interval_ms`, `request_timeout_ms`, `smoothing_window_secs`.
- **Вид**: `theme` (auto/light/dark), `window_opacity`, `default_compact`.
- **Поведение**: `always_on_top`, `minimize_to_tray`, `autorun`, `show_last_update`,
  `hotkey`, `language` (ru/en).
- **Диагностика**: `verbose_logging`.
- **Пороги контекста**: `warning_threshold`, `critical_threshold` (0..1, в UI — проценты).
- **Геометрия окна**: `position`, `size`.

Легаси-поля одиночного сервера (`server_host`, `server_port`, `base_path`,
`api_key`, `server_label`) читаются один раз при миграции и больше не
сериализуются.

---

## 10. Сборка и проверки

```bash
# Бэкенд
cd src-tauri && cargo test --bin llama-monitor     # 103 теста
cargo fmt --check

# Фронтенд
npm run check      # check:stores + check:i18n
npm run build      # те же чеки + vite build

# Приложение
npm run tauri:build
```

Оба чека фронтенда выполняются внутри `npm run build`, поэтому регрессия по
ним не доедет до релиза. Ручные сценарии, которые они не покрывают
(трей, always-on-top, смена сервера, локаль), описаны в `PRODUCTION_DOCS.md`.
