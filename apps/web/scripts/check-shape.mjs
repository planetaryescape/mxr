#!/usr/bin/env node
/*
 * Shape rule (docs/blueprint/27, D127): surfaces, panes, rows and sheets are
 * square, controls get the 3px control radius, and round shapes are for
 * people and dots. This flags, in TSX under src:
 *   - `rounded-full` outside the components that draw people, dots and counts;
 *   - `rounded-lg` and larger (`rounded-xl`, `rounded-2xl`, side variants too);
 *   - an arbitrary radius above 3px (`rounded-[8px]`, `rounded-[0.5rem]`).
 * `rounded-md` and `rounded-sm` stay: both read --radius-control (3px).
 *
 * ROUND_ALLOWED lists the files that may use `rounded-full`, each with its
 * reason. A new round shape belongs in one of them, not in a screen.
 *
 * Run directly, it checks src and exits 1 with file:line for each finding.
 * check-shape.test.mjs feeds it fixtures.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** Relative to src/. */
export const ROUND_ALLOWED = new Map([
  ["components/Avatar.tsx", "people"],
  ["components/CountBadge.tsx", "counts"],
  ["components/UnreadDot.tsx", "the unread mark"],
  ["components/StatusDot.tsx", "dots: sync health, label colours, your turn"],
]);

/** The corner and side suffixes Tailwind accepts: `rounded-t-lg`, `rounded-tl-xl`. */
const SIDE = "(?:-(?:t|r|b|l|s|e|tl|tr|br|bl|ss|se|ee|es))?";
const BOUNDARY_BEFORE = "(?<![\\w-])";
const FULL = new RegExp(`${BOUNDARY_BEFORE}rounded${SIDE}-full(?![\\w-])`, "g");
const LARGE = new RegExp(`${BOUNDARY_BEFORE}rounded${SIDE}-(?:lg|xl|[2-9]xl)(?![\\w-])`, "g");
const ARBITRARY = new RegExp(
  `${BOUNDARY_BEFORE}rounded${SIDE}-\\[\\s*(\\d*\\.?\\d+)\\s*(px|rem)\\s*\\](?![\\w-])`,
  "g",
);
const MAX_CONTROL_PX = 3;

/** Comments name the classes they forbid; blank them, keeping line numbers. */
function withoutComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "))
    .replace(/(^|\s)\/\/.*$/gm, "$1");
}

/**
 * Findings in a TSX source: `{ line, message }`. `round` is whether this
 * file may use `rounded-full`.
 */
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

/** True when `rel` (relative to src/) may use `rounded-full`. */
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
