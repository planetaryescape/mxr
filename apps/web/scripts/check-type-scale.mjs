#!/usr/bin/env node

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const ALLOWLIST = new Map([]);

const LITERAL = /(?<![\w-])text-\[\s*\d*\.?\d+\s*(?:px|rem)\s*\]/g;

export function checkScript(text) {
  const findings = [];
  for (const match of text.matchAll(LITERAL)) {
    const line = text.slice(0, match.index).split("\n").length;
    findings.push({ line, message: `${match[0]}: use a text-* token from tokens.css` });
  }
  return findings;
}

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
