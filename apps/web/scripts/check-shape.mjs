#!/usr/bin/env node

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const ROUND_ALLOWED = new Map([
  ["components/Avatar.tsx", "people"],
  ["components/CountBadge.tsx", "counts"],
  ["components/UnreadDot.tsx", "the unread mark"],
  ["components/StatusDot.tsx", "dots: sync health, label colours, your turn"],
]);

const SIDE = "(?:-(?:t|r|b|l|s|e|tl|tr|br|bl|ss|se|ee|es))?";
const BOUNDARY_BEFORE = "(?<![\\w-])";
const FULL = new RegExp(`${BOUNDARY_BEFORE}rounded${SIDE}-full(?![\\w-])`, "g");
const LARGE = new RegExp(`${BOUNDARY_BEFORE}rounded${SIDE}-(?:lg|xl|[2-9]xl)(?![\\w-])`, "g");
const ARBITRARY = new RegExp(
  `${BOUNDARY_BEFORE}rounded${SIDE}-\\[\\s*(\\d*\\.?\\d+)\\s*(px|rem)\\s*\\](?![\\w-])`,
  "g",
);
const MAX_CONTROL_PX = 3;

function withoutComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "))
    .replace(/(^|\s)\/\/.*$/gm, "$1");
}

export function checkScript(text, { round = false } = {}) {
  const source = withoutComments(text);
  const findings = [];
  const add = (index, message) =>
    findings.push({ line: source.slice(0, index).split("\n").length, message });
  if (!round) {
    for (const match of source.matchAll(FULL)) {
      add(match.index, `${match[0]}: round is for Avatar, UnreadDot, StatusDot and CountBadge`);
    }
  }
  for (const match of source.matchAll(LARGE)) {
    add(match.index, `${match[0]}: surfaces are square and controls use rounded-md (3px)`);
  }
  for (const match of source.matchAll(ARBITRARY)) {
    const px = Number(match[1]) * (match[2] === "rem" ? 16 : 1);
    if (px > MAX_CONTROL_PX) add(match.index, `${match[0]}: radii stop at ${MAX_CONTROL_PX}px`);
  }
  return findings.sort((a, b) => a.line - b.line);
}

export function roundAllowed(rel, list = ROUND_ALLOWED) {
  return list.has(rel);
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
    const round = roundAllowed(rel);
    for (const { line, message } of checkScript(readFileSync(path, "utf8"), { round })) {
      findings.push(`${relative(root, path)}:${line}: ${message}`);
    }
  }
  if (findings.length > 0) {
    console.error(`Shape (see scripts/check-shape.mjs):\n${findings.join("\n")}`);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
