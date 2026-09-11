#!/usr/bin/env node
/**
 * Guard against Svelte `$` misuse on non-store variables.
 *
 * Background: `ContextBar.svelte` contained `{#if $showTip && ...}` where
 * `showTip` is a plain `let` boolean, not a store. Svelte compiles `$showTip`
 * into `get_store_value(showTip)` -> `false.subscribe(...)` ->
 * "t.subscribe is not a function", which crashed the whole app at mount and
 * produced a blank window. Nothing in the build warns about this.
 *
 * This check parses each component's <script> block, collects every local
 * binding, and fails if the template references `$<name>` for a name that was
 * never imported as a store.
 *
 * Usage: node scripts/check-store-refs.mjs
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const SRC = join(ROOT, "src");

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (full.endsWith(".svelte")) out.push(full);
  }
  return out;
}

/** Names imported from a module that is a Svelte store/publisher. */
function storeImports(script) {
  const names = new Set();
  const re = /import\s*\{([^}]+)\}\s*from\s*["']([^"']+)["']/g;
  let m;
  while ((m = re.exec(script))) {
    const spec = m[1];
    for (const raw of spec.split(",")) {
      const name = raw.trim().split(/\s+as\s+/).pop().trim();
      if (name) names.add(name);
    }
  }
  return names;
}

/** Names declared locally inside <script>. */
function localDecls(script) {
  const names = new Set();
  const patterns = [
    /\b(?:let|const|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)/g,
    /\bfunction\s+([A-Za-z_$][A-Za-z0-9_$]*)/g,
    /export\s+let\s+([A-Za-z_$][A-Za-z0-9_$]*)/g,
  ];
  for (const re of patterns) {
    let m;
    while ((m = re.exec(script))) names.add(m[1]);
  }
  return names;
}

let failures = 0;

for (const file of walk(SRC)) {
  const text = readFileSync(file, "utf8");
  const scriptMatch = text.match(/<script[^>]*>([\s\S]*?)<\/script>/);
  const script = scriptMatch ? scriptMatch[1] : "";

  // Everything after </script> is markup (plus <style>).
  const markup = text.slice(scriptMatch ? scriptMatch.index + scriptMatch[0].length : 0);
  const markupOnly = markup.split(/<style[\s>]/)[0];

  const stores = storeImports(script);
  const locals = localDecls(script);

  // `$name` in the markup, ignoring `$$` internals.
  const refs = new Set();
  const re = /(?<!\$)\$([A-Za-z_$][A-Za-z0-9_$]*)/g;
  let m;
  while ((m = re.exec(markupOnly))) refs.add(m[1]);

  for (const name of refs) {
    if (locals.has(name) && !stores.has(name)) {
      console.error(
        `FAIL ${relative(ROOT, file)}: "$${name}" is used in the template but ` +
          `"${name}" is a local binding with no .subscribe() — ` +
          `use "$${name}" only for stores (drop the "$").`,
      );
      failures++;
    }
  }
}

if (failures > 0) {
  console.error(`\n${failures} store-reference problem(s) found.`);
  process.exit(1);
}
console.log("store-reference check: OK (no $-misuse on local bindings)");
