#!/usr/bin/env node
/*
 * Motion rule (rubric B2, DESIGN.md): durations come from the motion tokens
 * in src/styles/tokens.css, and only transform and opacity move. oxlint
 * can't see Tailwind class strings, inline styles or CSS, so this checks:
 *
 * - Tailwind classes: no transition-colors, transition-all or bare
 *   `transition`; a transition-[…] list only names transform or opacity;
 *   no numeric or arbitrary duration-*; no ease-in (ease-in-out is fine);
 *   arbitrary properties ([transition:…], [animation:…]) follow the CSS
 *   rule below.
 * - Inline styles (`transition: "…"`, `animationDuration: 200`): the CSS
 *   rule below.
 * - CSS (outside tokens.css): no literal time in a transition or animation
 *   declaration, and transitions only name transform or opacity.
 *
 * Run directly, it checks src and exits 1 with file:line for each finding.
 * check-motion.test.mjs feeds it fixtures.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const MOVING = new Set(["opacity", "transform", "translate", "scale", "rotate", "none"]);

/** A class token: bounded by quotes, backticks, whitespace or line ends. */
const token = (body) => new RegExp(`(?:^|["'\`\\s])(${body})(?=["'\`\\s]|$)`, "g");

const CLASS_RULES = [
  {
    pattern: token("[\\w:-]*?transition(?:-colors|-all)?"),
    message: "colour, `all` and bare transitions move more than transform and opacity",
  },
  {
    pattern: token("[\\w:-]*?duration-(?:\\[[^\\]]*\\]|\\d+)"),
    message: "use a motion token (duration-fast, duration-base, duration-slow)",
  },
  { pattern: token("[\\w:-]*?ease-in"), message: "no ease-in on UI: use ease-out" },
];

/** Blank out comments, keeping line numbers. */
function stripComments(text, lineComments) {
  let out = text.replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "));
  if (lineComments) out = out.replace(/(^|[^:"'`])\/\/.*$/gm, "$1");
  return out;
}

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

/**
 * Problems with one transition or animation declaration (kebab-case name),
 * as messages. Times must come from tokens: `var(--motion-…)`.
 */
export function declarationProblems(name, value) {
  const problems = [];
  if (/^(transition|animation)(-duration|-delay)?$/.test(name)) {
    const literal = value.match(/(?<![\w-])\d*\.?\d+m?s\b/);
    if (literal) problems.push(`${name}: ${literal[0]} is not a motion token`);
  }
  if (name === "transition-property" || name === "transition") {
    const props = value
      .replace(/!important/, "")
      .split(",")
      .map((part) => part.trim().split(/\s+/)[0]);
    const named = props.filter((prop) => prop && !prop.startsWith("var("));
    if (named.some((prop) => !MOVING.has(prop))) {
      problems.push(`${name}: only transform and opacity move`);
    }
  }
  return problems;
}

const kebab = (name) => name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);

/** Findings in a TS/TSX source: `{ line, message }`. */
export function checkScript(text) {
  const findings = [];
  const code = stripComments(text, true);
  const at = (index, message) => findings.push({ line: lineOf(code, index), message });
  for (const { pattern, message } of CLASS_RULES) {
    for (const match of code.matchAll(pattern)) at(match.index, `${match[1]}: ${message}`);
  }
  for (const match of code.matchAll(/transition-\[([^\]]*)\]/g)) {
    const props = match[1].split(",").map((prop) => prop.trim());
    if (props.some((prop) => !MOVING.has(prop))) {
      at(match.index, `${match[0]}: only transform and opacity move`);
    }
  }
  // Tailwind arbitrary properties: [transition:color_200ms], [animation-duration:1s].
  for (const match of code.matchAll(
    /\[((?:transition|animation)(?:-property|-duration|-delay)?):([^\]]+)\]/g,
  )) {
    for (const problem of declarationProblems(match[1], match[2].replaceAll("_", " "))) {
      at(match.index, problem);
    }
  }
  // Inline styles: transition: "opacity 200ms", animationDuration: "1s" or 200.
  for (const match of code.matchAll(
    /\b((?:transition|animation)(?:Property|Duration|Delay)?)\s*:\s*(?:(["'`])([^"'`]*)\2|(\d[\d.]*)\b)/g,
  )) {
    const name = kebab(match[1]);
    if (match[4] !== undefined) {
      if (/(duration|delay)$/.test(name))
        at(match.index, `${name}: ${match[4]} is not a motion token`);
      continue;
    }
    for (const problem of declarationProblems(name, match[3])) at(match.index, problem);
  }
  return findings;
}

/** Findings in a stylesheet: `{ line, message }`. */
export function checkStyles(text) {
  const findings = [];
  const css = stripComments(text, false);
  // Declarations: `name: value` up to the next ; { or }.
  for (const match of css.matchAll(/([\w-]+)\s*:\s*([^;{}]+)/g)) {
    for (const message of declarationProblems(match[1], match[2])) {
      findings.push({ line: lineOf(css, match.index), message });
    }
  }
  return findings;
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
  const tokens = join(src, "styles", "tokens.css");
  const findings = [];
  for (const path of files(src)) {
    if (/\.test\.tsx?$/.test(path) || path.endsWith("routeTree.gen.ts")) continue;
    if (path.endsWith(join("src", "api", "generated.ts")) || path === tokens) continue;
    const text = readFileSync(path, "utf8");
    const found = /\.tsx?$/.test(path)
      ? checkScript(text)
      : path.endsWith(".css")
        ? checkStyles(text)
        : [];
    for (const { line, message } of found) {
      findings.push(`${relative(root, path)}:${line}: ${message}`);
    }
  }
  if (findings.length > 0) {
    console.error(
      `Motion rule (tokens only; only transform and opacity move):\n${findings.join("\n")}`,
    );
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
