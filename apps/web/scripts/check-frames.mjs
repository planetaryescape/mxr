#!/usr/bin/env node
/*
 * Frame rule (docs/blueprint/27, D128): every page wears the one page frame,
 * and a frame has at most one primary action. This flags:
 *   - a route whose component never reaches `<PageFrame>` (directly, or through
 *     `<Page>` / `<ListWithReader>`, which render it), looking through its
 *     imports from features/ up to MAX_DEPTH files deep;
 *   - a `primary` slot that holds more than one action (a fragment of buttons).
 *
 * Routes with no component of their own (redirects, `() => null`) are skipped.
 * EXEMPT lists the rest that legitimately have no page, each with its reason.
 *
 * Run directly, it checks src and exits 1 with file:line for each finding.
 * check-frames.test.mjs feeds it in-memory files.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/** Relative to src/. */
export const EXEMPT = new Map([
  ["routes/__root.tsx", "the shell, which wraps every page"],
  ["routes/analytics.tsx", "layout route: an Outlet for the dashboard pages"],
  ["routes/compose.new.tsx", "deep link: opens the compose sheet and leaves"],
  ["routes/compose.$draftId.tsx", "deep link: opens the compose sheet and leaves"],
]);

/** Components that render the frame for their caller. */
const FRAME_TAGS = ["PageFrame", "Page", "ListWithReader"];
const FRAME_TAG = new RegExp(`<(?:${FRAME_TAGS.join("|")})(?=[\\s/>])`);
const MAX_DEPTH = 4;

/** Comments name the tags they mention; blank them, keeping line numbers. */
function withoutComments(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "))
    .replace(/(^|\s)\/\/.*$/gm, "$1");
}

/** `import x from "path"` / `import { a } from "path"` specifiers that are not type-only. */
export function importsOf(text) {
  const found = [];
  for (const match of withoutComments(text).matchAll(
    /import\s+(?!type\b)([\s\S]*?)\s+from\s+["']([^"']+)["']/g,
  )) {
    found.push({ names: match[1], from: match[2] });
  }
  return found;
}

/** The identifier a route file names as its `component`, or null for a redirect or inline one. */
export function routeComponent(text) {
  return /component:\s*([A-Za-z_$][\w$]*)\s*[,}\n]/.exec(withoutComments(text))?.[1] ?? null;
}

/**
 * Whether `file` renders the frame. `files` maps a path (relative to src/)
 * to its text; `resolve(from, specifier)` turns an import into such a path
 * (or null when it isn't a source file).
 */
export function rendersFrame(file, files, resolve, depth = MAX_DEPTH, seen = new Set()) {
  if (seen.has(file)) return false;
  seen.add(file);
  const text = files.get(file);
  if (text === undefined) return false;
  if (FRAME_TAG.test(withoutComments(text))) return true;
  if (depth === 0) return false;
  for (const { from } of importsOf(text)) {
    const next = resolve(file, from);
    if (next && rendersFrame(next, files, resolve, depth - 1, seen)) return true;
  }
  return false;
}

/** Findings for one route file: `{ line, message }`. */
export function checkRoute(file, files, resolve) {
  const text = files.get(file) ?? "";
  const name = routeComponent(text);
  if (!name) return [];
  const own = importsOf(text).find(({ names }) =>
    new RegExp(`(?<![\\w$])${name.replace(/\$/g, "\\$")}(?![\\w$])`).test(names),
  );
  const target = own ? resolve(file, own.from) : file;
  if (target && rendersFrame(target, files, resolve)) return [];
  const line = text.split("\n").findIndex((row) => row.includes(`component: ${name}`)) + 1;
  return [
    {
      line: Math.max(line, 1),
      message: `${name} never renders PageFrame (Page and ListWithReader do)`,
    },
  ];
}

/** The balanced `{...}` expression starting at `open` (the index of its `{`), or null. */
function braced(text, open) {
  let depth = 0;
  let quote = null;
  for (let i = open; i < text.length; i += 1) {
    const char = text[i];
    if (quote) {
      if (char === "\\") i += 1;
      else if (char === quote) quote = null;
      continue;
    }
    if (char === '"' || char === "'" || char === "`") quote = char;
    else if (char === "{") depth += 1;
    else if (char === "}" && --depth === 0) return text.slice(open + 1, i);
  }
  return null;
}

const ACTION = /<(?:Button|Link|a)(?=[\s/>])/g;

/** Findings for the `primary` slots in one TSX source: more than one action in it. */
export function checkPrimary(text) {
  const source = withoutComments(text);
  const findings = [];
  for (const match of source.matchAll(/(?<![\w-])primary=\{/g)) {
    const expression = braced(source, match.index + match[0].length - 1);
    if (expression === null) continue;
    const body = expression.trim().replace(/^\(\s*/, "");
    // A ternary or `&&` renders one of its branches; only a fragment or an
    // array puts several actions in the slot at once.
    const several = /^(?:<>|<Fragment[\s>]|\[)/.test(body) && (body.match(ACTION)?.length ?? 0) > 1;
    if (several) {
      findings.push({
        line: source.slice(0, match.index).split("\n").length,
        message: "a frame has one primary action; move the others into overflow",
      });
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

/** Resolve `@/x` and relative imports to a path relative to src/, or null. */
export function makeResolver(exists) {
  return (from, specifier) => {
    let base;
    if (specifier.startsWith("@/")) base = specifier.slice(2);
    else if (specifier.startsWith(".")) base = join(dirname(from), specifier);
    else return null;
    for (const candidate of [`${base}.tsx`, `${base}.ts`, join(base, "index.tsx")]) {
      if (exists(candidate)) return candidate;
    }
    return null;
  };
}

function main() {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const src = join(root, "src");
  const sources = new Map();
  for (const path of files(src)) {
    if (!/\.tsx?$/.test(path) || /\.test\.tsx?$/.test(path)) continue;
    sources.set(relative(src, path), readFileSync(path, "utf8"));
  }
  const resolve = makeResolver((candidate) => sources.has(candidate));
  const findings = [];
  for (const [rel, text] of sources) {
    if (rel.startsWith("routes/") && rel.endsWith(".tsx") && !EXEMPT.has(rel)) {
      for (const { line, message } of checkRoute(rel, sources, resolve)) {
        findings.push(`src/${rel}:${line}: ${message}`);
      }
    }
    if (rel.endsWith(".tsx")) {
      for (const { line, message } of checkPrimary(text))
        findings.push(`src/${rel}:${line}: ${message}`);
    }
  }
  if (findings.length > 0) {
    console.error(`Page frames (see scripts/check-frames.mjs):\n${findings.join("\n")}`);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) main();
