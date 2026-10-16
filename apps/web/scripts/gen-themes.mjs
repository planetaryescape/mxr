#!/usr/bin/env node
/*
 * Writes src/styles/themes.generated.css from the theme inputs in
 * src/styles/themes.ts (docs/blueprint/27 "Generating themes", D139).
 * `--check` writes nothing and exits 1 when the committed file is stale, so
 * a hand edit to the generated CSS fails lint. Needs Node's built-in
 * TypeScript support (22.18 or newer; CI runs 24).
 */

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const { renderThemesCss } = await import("../src/styles/themes.ts");

const target = fileURLToPath(new URL("../src/styles/themes.generated.css", import.meta.url));
const next = renderThemesCss();

if (process.argv.includes("--check")) {
  let current = "";
  try {
    current = readFileSync(target, "utf8");
  } catch {
    // A missing file is stale too.
  }
  if (current !== next) {
    console.error("src/styles/themes.generated.css is stale. Run `npm run gen:themes`.");
    process.exit(1);
  }
} else {
  writeFileSync(target, next);
}
