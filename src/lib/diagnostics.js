/**
 * Diagnostics are codes, not prose.
 *
 * The Rust side sends `{ code, detail? }` (see `DiagCode` in
 * `src-tauri/src/api/types.rs`) so the diagnostics block is translated like the
 * rest of the widget instead of staying Russian under an English locale.
 * `detail` carries values that cannot be translated — an OS or network error
 * string — and is appended verbatim.
 */

/**
 * Render one diagnostic as display text.
 *
 * @param {{code?: string, detail?: string} | string} diag
 * @param {(key: string) => string} t the i18n translator.
 */
export function diagText(diag, t) {
  // Tolerate a plain string so an older backend payload cannot blank the block.
  if (typeof diag === "string") return diag;
  if (!diag || typeof diag !== "object") return "";
  const key = `diag_${diag.code}`;
  const translated = t(key);
  // A missing translation returns the key itself; showing the bare code is
  // more useful than showing `diag_something_new`.
  const label = translated === key ? String(diag.code || "") : translated;
  return diag.detail ? `${label}: ${diag.detail}` : label;
}

/** True when the diagnostics list contains `code`. */
export function hasDiag(diagnostics, code) {
  return (diagnostics || []).some((d) => d && d.code === code);
}
