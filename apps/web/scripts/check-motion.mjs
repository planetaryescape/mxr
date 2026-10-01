#!/usr/bin/env node
/*
 * Motion rule (rubric B2, DESIGN.md): durations come from the motion tokens
 * in src/styles/tokens.css, and only transform and opacity move. oxlint
 * can't see Tailwind class strings or CSS, so this checks both:
 *
 * - TS/TSX: no transition-colors, transition-all or bare `transition`; a
 *   transition-[…] list only names transform or opacity; no numeric or
 *   arbitrary duration-*; no ease-in (ease-in-out is fine).
 * - CSS (outside tokens.css): no literal time in a transition or animation
 *   declaration, and transitions only name transform or opacity.
 *
 * Exits 1 with file:line for each finding.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const src = join(root, "src");
const TOKENS = join(src, "styles", "tokens.css");
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

function* files(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* files(path);
    else yield path;
  }
}

/** Blank out comments, keeping line numbers. */
function stripComments(text, lineComments) {
  let out = text.replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "));
  if (lineComments) out = out.replace(/(^|[^:"'`])\/\/.*$/gm, "$1");
  return out;
}

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

function checkScript(path, text, report) {
  const code = stripComments(text, true);
  for (const { pattern, message } of CLASS_RULES) {
    for (const match of code.matchAll(pattern)) {
      report(path, lineOf(code, match.index), `${match[1]}: ${message}`);
    }
  }
  for (const match of code.matchAll(/transition-\[([^\]]*)\]/g)) {
    const props = match[1].split(",").map((prop) => prop.trim());
    if (props.some((prop) => !MOVING.has(prop))) {
      report(path, lineOf(code, match.index), `${match[0]}: only transform and opacity move`);
    }
  }
}

function checkStyles(path, text, report) {
  const css = stripComments(text, false);
  // Declarations: `name: value` up to the next ; { or }.
  for (const match of css.matchAll(/([\w-]+)\s*:\s*([^;{}]+)/g)) {
    const [, name, value] = match;
    const line = () => lineOf(css, match.index);
    if (/^(transition|animation)(-duration|-delay)?$/.test(name) && path !== TOKENS) {
      const literal = value.match(/(?<![\w-])\d*\.?\d+m?s\b/);
      if (literal) report(path, line(), `${name}: ${literal[0]} is not a motion token`);
    }
    if (name === "transition-property" || name === "transition") {
      const props = value
        .replace(/!important/, "")
        .split(",")
        .map((part) => part.trim().split(/\s+/)[0]);
      const moving = props.filter((prop) => prop && !prop.startsWith("var("));
      if (moving.some((prop) => !MOVING.has(prop))) {
        report(path, line(), `${name}: only transform and opacity move`);
      }
    }
  }
}

const findings = [];
const report = (path, line, message) =>
  findings.push(`${relative(root, path)}:${line}: ${message}`);

for (const path of files(src)) {
  if (/\.test\.tsx?$/.test(path) || path.endsWith("routeTree.gen.ts")) continue;
  if (path.endsWith(join("src", "api", "generated.ts"))) continue;
  const text = readFileSync(path, "utf8");
  if (/\.tsx?$/.test(path)) checkScript(path, text, report);
  else if (path.endsWith(".css")) checkStyles(path, text, report);
}

if (findings.length > 0) {
  console.error(
    `Motion rule (tokens only; only transform and opacity move):\n${findings.join("\n")}`,
  );
  process.exit(1);
}
