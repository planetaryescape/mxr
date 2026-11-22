#!/usr/bin/env node

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const { renderThemesCss } = await import("../src/styles/themes.ts");

const target = fileURLToPath(new URL("../src/styles/themes.generated.css", import.meta.url));
const next = renderThemesCss();

if (process.argv.includes("--check")) {
  let current = "";
  try {
    current = readFileSync(target, "utf8");
  } catch {}
  if (current !== next) {
    console.error("src/styles/themes.generated.css is stale. Run `npm run gen:themes`.");
    process.exit(1);
  }
} else {
  writeFileSync(target, next);
}
