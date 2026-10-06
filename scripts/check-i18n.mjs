#!/usr/bin/env node
// Guards the two things that silently break localization:
//
//   1. The `ru` and `en` dictionaries drifting apart. A key added to one and
//      not the other renders as the raw key (`ctx_summary`) for half the users,
//      and nothing else in the build notices.
//   2. A `DiagCode` added in Rust without a `diag_<code>` translation. The
//      diagnostics block would then show `diag_something_new` instead of a
//      sentence, in both locales.
//
// Run via `npm run check:i18n` (wired into `npm run build`).

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { dictionaries, SUPPORTED_LOCALES } from "../src/i18n.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

// --- 1. dictionary parity -------------------------------------------------
const [baseLocale, ...otherLocales] = SUPPORTED_LOCALES;
const baseKeys = Object.keys(dictionaries[baseLocale]).sort();

for (const locale of otherLocales) {
  const keys = new Set(Object.keys(dictionaries[locale]));
  const missing = baseKeys.filter((k) => !keys.has(k));
  const extra = Object.keys(dictionaries[locale])
    .filter((k) => !baseKeys.includes(k))
    .sort();
  if (missing.length) failures.push(`[${locale}] missing keys: ${missing.join(", ")}`);
  if (extra.length) failures.push(`[${locale}] keys not in ${baseLocale}: ${extra.join(", ")}`);
}

// --- 2. every Rust DiagCode has a translation -----------------------------
const typesRs = readFileSync(join(root, "src-tauri/src/api/types.rs"), "utf8");
// Matches both `DiagCode::Stale => "stale"` and `DiagCode::PollFailed(_) => "poll_failed"`.
const codes = [...typesRs.matchAll(/DiagCode::\w+(?:\([^)]*\))?\s*=>\s*"([a-z0-9_]+)"/g)].map(
  (m) => m[1],
);
if (codes.length === 0) {
  failures.push("found no DiagCode wire codes in src-tauri/src/api/types.rs — did the enum move?");
}
for (const code of codes) {
  for (const locale of SUPPORTED_LOCALES) {
    if (!(`diag_${code}` in dictionaries[locale])) {
      failures.push(
        `[${locale}] no translation for diagnostic code \`${code}\` (expected diag_${code})`,
      );
    }
  }
}

if (failures.length) {
  console.error("i18n check failed:");
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}

console.log(
  `i18n check: OK (${baseKeys.length} keys × ${SUPPORTED_LOCALES.length} locales, ` +
    `${codes.length} diagnostic codes)`,
);
