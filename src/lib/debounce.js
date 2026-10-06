/**
 * Debounced settings persistence.
 *
 * The settings panel used to persist on every `input` event, so typing
 * `30000` into the poll-interval field wrote the settings file four times and
 * re-registered the global hotkey four times — and typing a hotkey fired a
 * registration per character, each failing and logging a warning.
 *
 * Text and number fields now go through `apply()` (saved once the user stops
 * typing); toggles and dropdowns use `applyNow()` because a single click is
 * already a complete decision.
 */

/** How long the user must stop typing before the value is persisted. */
export const APPLY_DELAY_MS = 400;

/**
 * Wrap a persistence function in a debounce.
 *
 * @param {(patch: object) => void} save called with the accumulated patch.
 * @param {number} [delayMs] quiet period before saving.
 */
export function createDebouncedApply(save, delayMs = APPLY_DELAY_MS) {
  let timer = null;
  let pending = null;

  function flush() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (pending) {
      const patch = pending;
      pending = null;
      save(patch);
    }
  }

  /** Queue `patch`; the write happens once the user pauses. */
  function apply(patch) {
    pending = { ...(pending || {}), ...patch };
    if (timer) clearTimeout(timer);
    timer = setTimeout(flush, delayMs);
  }

  /** Persist `patch` immediately, flushing anything already queued first. */
  function applyNow(patch) {
    flush();
    save(patch);
  }

  /** Drop anything queued without saving (component teardown). */
  function cancel() {
    if (timer) clearTimeout(timer);
    timer = null;
    pending = null;
  }

  return { apply, applyNow, flush, cancel };
}
