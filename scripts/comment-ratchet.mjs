#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const baselinePath = join(repo, 'scripts/comment-ratchet.baseline.json');

const SOURCE = /\.(rs|ts|tsx|mts|cts|js|jsx|mjs|cjs)$/;
const EXCLUDED = [/^vendor\//, /(^|\/)node_modules\//, /\.d\.ts$/, /(^|\/)dist\//, /\.min\.js$/];

const USAGE = `Comment ratchet: no file may gain comments, and the baseline only moves down.

  node scripts/comment-ratchet.mjs                      check against the baseline (CI)
  node scripts/comment-ratchet.mjs --update             lower the baseline to current counts
  node scripts/comment-ratchet.mjs --list [prefix...]   print each counted comment
  node scripts/comment-ratchet.mjs --stats              totals, and lines a full sweep would remove

Doc comments (///, //!, /** */) and tool directives are not counted. --update never
raises a count: raise one by editing scripts/comment-ratchet.baseline.json, so the
raise shows in review. .agents/skills/delete-comment/SKILL.md says which comments stay.`;

const TOOL_DIRECTIVE =
  /^(eslint-|@ts-|prettier-ignore|biome-ignore|istanbul |c8 |v8 ignore|@vitest-environment|@jsx|@refresh|#__PURE__|@__PURE__|webpackChunkName|@vite-ignore|<reference |SAFETY:|SPDX-License-Identifier|Copyright )/;

function endOfCharLiteralOrLifetime(src, quote) {
  if (src[quote + 1] === '\\') {
    const close = src.indexOf("'", quote + 3);
    return close < 0 ? src.length : close + 1;
  }
  const width = src.codePointAt(quote + 1) > 0xffff ? 2 : 1;
  const isCharLiteral = src[quote + 1 + width] === "'";
  return isCharLiteral ? quote + 2 + width : quote + 1;
}

export function rustComments(src) {
  const out = [];
  const n = src.length;
  const rawString = /(?:br|cr|r)(#*)"/y;
  const isIdent = (c) => c !== undefined && /[A-Za-z0-9_]/.test(c);
  let i = 0;
  while (i < n) {
    const c = src[i];
    const d = src[i + 1];
    if (c === '/' && d === '/') {
      let j = src.indexOf('\n', i);
      if (j < 0) j = n;
      const text = src.slice(i, j);
      const doc = (text.startsWith('///') && !text.startsWith('////')) || text.startsWith('//!');
      out.push({ start: i, end: j, doc });
      i = j;
      continue;
    }
    if (c === '/' && d === '*') {
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') {
          depth++;
          j += 2;
        } else if (src[j] === '*' && src[j + 1] === '/') {
          depth--;
          j += 2;
        } else j++;
      }
      const text = src.slice(i, j);
      const doc = (text.startsWith('/**') && !text.startsWith('/***') && text !== '/**/') || text.startsWith('/*!');
      out.push({ start: i, end: j, doc });
      i = j;
      continue;
    }
    if (!isIdent(src[i - 1])) {
      rawString.lastIndex = i;
      const raw = rawString.exec(src);
      if (raw) {
        const close = '"' + raw[1];
        const j = src.indexOf(close, i + raw[0].length);
        i = j < 0 ? n : j + close.length;
        continue;
      }
    }
    if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j += src[j] === '\\' ? 2 : 1;
      i = j + 1;
      continue;
    }
    if (c === "'") {
      i = endOfCharLiteralOrLifetime(src, i);
      continue;
    }
    i++;
  }
  return out;
}

let ts;
function typescript() {
  if (!ts) {
    const require = createRequire(join(repo, 'apps/web/package.json'));
    try {
      ts = require('typescript');
    } catch {
      throw new Error('comment-ratchet needs TypeScript: run `npm ci` in apps/web first.');
    }
  }
  return ts;
}

export function scriptComments(src, fileName) {
  const ts = typescript();
  const kind = /\.(tsx|jsx)$/.test(fileName) ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sf = ts.createSourceFile(fileName, src, ts.ScriptTarget.Latest, false, kind);
  const seen = new Map();
  const collect = (pos, end, k) => {
    if (seen.has(pos)) return;
    const text = src.slice(pos, end);
    const doc = k === ts.SyntaxKind.MultiLineCommentTrivia && text.startsWith('/**') && text !== '/**/';
    seen.set(pos, { start: pos, end, doc });
  };
  const tokens = [];
  const visit = (node) => {
    const children = node.getChildren(sf);
    if (children.length === 0) tokens.push(node);
    else children.forEach(visit);
  };
  visit(sf);
  const isJsxText = (token) => token?.kind === ts.SyntaxKind.JsxText;
  tokens.forEach((token, i) => {
    if (isJsxText(token)) return;
    if (!isJsxText(tokens[i - 1])) ts.forEachLeadingCommentRange(src, token.getFullStart(), collect);
    if (!isJsxText(tokens[i + 1])) ts.forEachTrailingCommentRange(src, token.getEnd(), collect);
  });
  return [...seen.values()].sort((a, b) => a.start - b.start);
}

function commentsIn(path, src) {
  return path.endsWith('.rs') ? rustComments(src) : scriptComments(src, path);
}

function isDirective(text) {
  const body = text.replace(/^\/[/*]+!?\s*/, '');
  return TOOL_DIRECTIVE.test(body);
}

export function classify(path, src) {
  const lineStarts = [0];
  for (let i = 0; i < src.length; i++) if (src[i] === '\n') lineStarts.push(i + 1);
  const lineOf = (pos) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  const counted = [];
  const doc = [];
  const directive = [];
  for (const c of commentsIn(path, src)) {
    const text = src.slice(c.start, c.end);
    const entry = { ...c, text, line: lineOf(c.start) + 1, lastLine: lineOf(Math.max(c.start, c.end - 1)) + 1 };
    if (c.doc) doc.push(entry);
    else if (isDirective(text)) directive.push(entry);
    else counted.push(entry);
  }
  const commentOnlyLines = (entries) => {
    const covered = new Set();
    for (const e of entries) {
      for (let l = e.line; l <= e.lastLine; l++) {
        const from = lineStarts[l - 1];
        const to = l < lineStarts.length ? lineStarts[l] - 1 : src.length;
        const before = src.slice(from, Math.max(from, Math.min(e.start, to)));
        const after = src.slice(Math.min(Math.max(e.end, from), to), to);
        const firstLine = l === e.line;
        const lastLine = l === e.lastLine;
        if ((!firstLine || before.trim() === '') && (!lastLine || after.trim() === '')) covered.add(l);
      }
    }
    return covered.size;
  };
  return {
    counted,
    doc,
    directive,
    countedLines: commentOnlyLines(counted),
    docLines: commentOnlyLines(doc),
  };
}

function sourceFiles() {
  return execFileSync('git', ['ls-files'], { cwd: repo, encoding: 'utf8', maxBuffer: 64 << 20 })
    .split('\n')
    .filter((p) => SOURCE.test(p) && !EXCLUDED.some((re) => re.test(p)) && existsSync(join(repo, p)));
}

function scan() {
  const result = new Map();
  for (const path of sourceFiles()) result.set(path, classify(path, readFileSync(join(repo, path), 'utf8')));
  return result;
}

function readBaseline() {
  return existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, 'utf8')) : {};
}

function writeBaseline(counts) {
  const sorted = Object.fromEntries(Object.entries(counts).filter(([, n]) => n > 0).sort(([a], [b]) => a.localeCompare(b)));
  writeFileSync(baselinePath, JSON.stringify(sorted, null, 2) + '\n');
}

function show(path, entry) {
  const first = entry.text.split('\n')[0].trim();
  return `${path}:${entry.line}: ${first.length > 100 ? first.slice(0, 97) + '...' : first}`;
}

function check() {
  const baseline = readBaseline();
  const files = scan();
  const over = [];
  const under = [];
  for (const [path, r] of files) {
    const allowed = baseline[path] ?? 0;
    if (r.counted.length > allowed) over.push([path, r, allowed]);
    else if (r.counted.length < allowed) under.push([path, r.counted.length, allowed]);
  }
  for (const path of Object.keys(baseline)) if (!files.has(path)) under.push([path, 0, baseline[path]]);
  for (const [path, r, allowed] of over) {
    console.error(`${path}: ${r.counted.length} comments, baseline allows ${allowed}`);
    for (const e of r.counted) console.error(`  ${show(path, e)}`);
  }
  for (const [path, now, allowed] of under) console.error(`${path}: ${now} comments, baseline still allows ${allowed}`);
  if (over.length) {
    console.error(
      '\nComments are ratcheted down. Say it in code (names, types, a test), or follow the ' +
        'delete-comment skill (.agents/skills/delete-comment/SKILL.md) for what may stay.',
    );
  }
  if (under.length) console.error('\nLock in the removed comments: node scripts/comment-ratchet.mjs --update');
  if (over.length || under.length) process.exit(1);
  const total = [...files.values()].reduce((s, r) => s + r.counted.length, 0);
  console.log(`comment-ratchet: ${total} comments, none above baseline.`);
}

function update() {
  const seeding = !existsSync(baselinePath);
  const baseline = readBaseline();
  const next = {};
  const raised = [];
  for (const [path, r] of scan()) {
    const allowed = seeding ? r.counted.length : (baseline[path] ?? 0);
    if (r.counted.length > allowed) raised.push(`${path}: ${r.counted.length} > ${allowed}`);
    next[path] = Math.min(r.counted.length, allowed);
  }
  writeBaseline(next);
  if (raised.length) {
    console.error('Baseline kept for files that gained comments (raise one only by editing the baseline):');
    for (const line of raised) console.error(`  ${line}`);
    process.exit(1);
  }
}

function list(prefixes) {
  for (const [path, r] of scan()) {
    if (prefixes.length && !prefixes.some((p) => path.startsWith(p))) continue;
    for (const e of r.counted) console.log(show(path, e));
  }
}

function stats() {
  const areas = new Map();
  const total = { files: 0, counted: 0, countedLines: 0, doc: 0, docLines: 0, directive: 0 };
  for (const [path, r] of scan()) {
    const area = path.split('/').slice(0, 2).join('/');
    const a = areas.get(area) ?? { counted: 0, countedLines: 0, doc: 0, docLines: 0 };
    a.counted += r.counted.length;
    a.countedLines += r.countedLines;
    a.doc += r.doc.length;
    a.docLines += r.docLines;
    areas.set(area, a);
    total.files++;
    total.counted += r.counted.length;
    total.countedLines += r.countedLines;
    total.doc += r.doc.length;
    total.docLines += r.docLines;
    total.directive += r.directive.length;
  }
  console.log(JSON.stringify({ total, areas: Object.fromEntries([...areas].sort((x, y) => y[1].countedLines - x[1].countedLines)) }, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [mode, ...rest] = process.argv.slice(2);
  if (mode === undefined || mode === '--check') check();
  else if (mode === '--update') update();
  else if (mode === '--list') list(rest);
  else if (mode === '--stats') stats();
  else {
    console.error(USAGE);
    process.exit(mode === '--help' ? 0 : 2);
  }
}
