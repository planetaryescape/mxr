#!/usr/bin/env node
/**
 * Post-build hook for the docs site.
 *
 * 1. Concatenates every Markdown source under `src/content/docs` into
 *    `dist/llms-full.txt` so an LLM can ingest the entire docs corpus
 *    in one shot.
 * 2. Emits a `.md` sibling next to every built `dist/.../index.html`
 *    so `curl https://mxr-mail.vercel.app/cookbook/triage.md` returns
 *    clean Markdown rather than minified HTML. This is the emerging
 *    convention popularised by Cloudflare/Vercel/Mintlify docs.
 *
 * The curated `llms.txt` is hand-written and lives at
 * `public/llms.txt`. It is copied verbatim into `dist/` by Astro;
 * we don't regenerate it here.
 */

import { readFileSync, readdirSync, writeFileSync, mkdirSync, statSync, existsSync } from 'node:fs';
import { join, relative, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const SITE_ROOT = resolve(__dirname, '..');
const DOCS_ROOT = join(SITE_ROOT, 'src', 'content', 'docs');
const DIST_ROOT = join(SITE_ROOT, 'dist');

function walk(dir, accumulator = []) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      walk(full, accumulator);
    } else if (/\.(md|mdx)$/.test(entry)) {
      accumulator.push(full);
    }
  }
  return accumulator;
}

function stripFrontmatter(raw) {
  if (!raw.startsWith('---')) return { body: raw, title: null, description: null };
  const end = raw.indexOf('\n---', 3);
  if (end === -1) return { body: raw, title: null, description: null };
  const fm = raw.slice(3, end).trim();
  const body = raw.slice(end + 4).replace(/^\n+/, '');
  const title = (fm.match(/^title:\s*(?:"([^"]*)"|'([^']*)'|(.+))$/m) || [])
    .slice(1)
    .find(Boolean) || null;
  const description = (fm.match(/^description:\s*(?:"([^"]*)"|'([^']*)'|(.+))$/m) || [])
    .slice(1)
    .find(Boolean) || null;
  return { body, title, description };
}

function buildLlmsFull() {
  if (!existsSync(DOCS_ROOT)) {
    console.warn(`[llms-full] docs root missing at ${DOCS_ROOT}; skipping`);
    return;
  }
  const files = walk(DOCS_ROOT).sort();
  const sections = [];
  sections.push('# mxr — full docs corpus');
  sections.push('');
  sections.push('Concatenated from every Markdown source under `src/content/docs`.');
  sections.push('Generated at build time. For an LLM-ingestible curated index, see `/llms.txt`.');
  sections.push('');

  for (const file of files) {
    const raw = readFileSync(file, 'utf8');
    const { body, title, description } = stripFrontmatter(raw);
    const slug = '/' + relative(DOCS_ROOT, file).replace(/\.(md|mdx)$/, '').replace(/\\/g, '/').replace(/\/index$/, '');
    sections.push('---');
    sections.push('');
    sections.push(`# ${title || slug}`);
    sections.push(`URL: https://mxr-mail.vercel.app${slug}/`);
    if (description) sections.push(`> ${description}`);
    sections.push('');
    sections.push(body.trim());
    sections.push('');
  }

  if (!existsSync(DIST_ROOT)) mkdirSync(DIST_ROOT, { recursive: true });
  writeFileSync(join(DIST_ROOT, 'llms-full.txt'), sections.join('\n'));
  console.log(`[llms-full] wrote ${files.length} pages to ${join(DIST_ROOT, 'llms-full.txt')}`);
}

function buildMarkdownSiblings() {
  if (!existsSync(DOCS_ROOT) || !existsSync(DIST_ROOT)) return;
  const files = walk(DOCS_ROOT);
  let count = 0;
  for (const file of files) {
    const raw = readFileSync(file, 'utf8');
    const { body, title, description } = stripFrontmatter(raw);
    const rel = relative(DOCS_ROOT, file).replace(/\.(md|mdx)$/, '').replace(/\\/g, '/');
    // Skip the docs root index — Astro's index page is the homepage.
    const target = rel === 'index'
      ? join(DIST_ROOT, 'index.md')
      : join(DIST_ROOT, rel + '.md');
    mkdirSync(dirname(target), { recursive: true });
    const out = [];
    if (title) out.push(`# ${title}`);
    if (description) out.push(`> ${description}`);
    if (out.length) out.push('');
    out.push(body.trim());
    writeFileSync(target, out.join('\n') + '\n');
    count++;
  }
  console.log(`[llms-md] wrote ${count} .md siblings into ${DIST_ROOT}`);
}

buildLlmsFull();
buildMarkdownSiblings();
