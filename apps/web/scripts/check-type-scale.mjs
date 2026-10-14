#!/usr/bin/env node
/*
 * Type rule: font sizes come from the scale in src/styles/tokens.css
 * (text-2xs … text-xl). A pixel or rem size in a class string is a second
 * scale, and the drift it lets in is what the scale exists to stop. This
 * flags `text-[13px]`, `md:text-[12.5px]` and `text-[1.1rem]`. It leaves
 * `text-[length:var(--…)]` (density tokens) and `text-[var(--…)]` alone.
 *
 * Files on ALLOWLIST may keep a literal size, each with the reason here.
 * Keep the list short: a token is the default, an entry is the exception.
 *
 * Run directly, it checks src and exits 1 with file:line for each finding.
 * check-type-scale.test.mjs feeds it fixtures.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** Relative to src/. A directory entry covers every file below it. */
export const ALLOWLIST = new Map([
  [
    "features/reading/ReadingReader.tsx",
    "reader title is 24px, then 28px from sm: and there is no token between 2xl (24) and 3xl (32)",
  ],
]);

/** A literal size: a number with px or rem, inside `text-[...]`, after a class boundary. */
const LITERAL = /(?<![\w-])text-\[\s*\d*\.?\d+\s*(?:px|rem)\s*\]/g;

/** Findings in a TSX source: `{ line, message }`. */
export function checkScript(text) {
  const findings = [];
  for (const match of text.matchAll(LITERAL)) {
    const line = text.slice(0, match.index).split("\n").length;
    findings.push({ line, message: `${match[0]}: use a text-* token from tokens.css` });
  }
  return findings;
}

/** True when `rel` (relative to src/) is covered by an allowlist entry. */
export function allowed(rel, list = ALLOWLIST) {
  for (const entry of list.keys()) {
    if (rel === entry || rel.startsWith(`${entry}/`)) return true;
  }
  return false;
}

function* files(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* files(path);
    else yield path;
  }
}

function main() {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const src = join(root, "src");
  const findings = [];
  for (const path of files(src)) {
    if (!path.endsWith(".tsx") || path.endsWith(".test.tsx")) continue;
    const rel = relative(src, path);
    if (allowed(rel)) continue;
    for (const { line, message } of checkScript(readFileSync(path, "utf8"))) {
      findings.push(`${relative(root, path)}:${line}: ${message}`);
    }
  }
  if (findings.length > 0) {
    console.error(
      `Type scale (tokens only; see scripts/check-type-scale.mjs):\n${findings.join("\n")}`,
    );
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
