#!/usr/bin/env node
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { searchEnvelopeMisreads } from './search-envelope-rule.mjs';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = resolve(__dirname, '..', '..');
const docsRoot = join(repoRoot, 'site', 'src', 'content', 'docs');
const openapiPath = join(repoRoot, 'site', 'public', 'openapi.json');

const banned = [
  { re: /list_id:/, message: 'search field is `list:`, not `list_id:`' },
  { re: /\.from\.email/, message: '`mxr search` emits string `.from`, not `.from.email`' },
  { re: /has_attachments/, message: '`mxr search` does not emit `has_attachments`' },
  { re: /credential_source\s*=\s*"byo"/, message: 'Gmail credential source is `custom`, not `byo`' },
  { re: /cors_allow_localhost/, message: 'bridge config uses `cors_allowlist`' },
  { re: /tomorrow_morning/, message: 'snooze config uses `morning_hour`' },
  { re: /--view\s+body/, message: '`mxr cat --view` accepts reader|raw|html|headers' },
  { re: /\|\s*xargs\s+-r/, message: 'GNU-only `xargs -r`; prefer mxr stdin or portable while-read' },
];

function* walk(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) yield* walk(path);
    else if (/\.(md|mdx)$/.test(path)) yield path;
  }
}

function docsContentId(file) {
  const relativePath = relative(docsRoot, file).split(sep).join('/');
  return relativePath.replace(/\.(md|mdx)$/, '').replace(/(^|\/)index$/, '$1').replace(/\/$/, '');
}

let failed = false;

const contentIds = new Map();

for (const file of walk(docsRoot)) {
  const contentId = docsContentId(file);
  const previous = contentIds.get(contentId);
  if (previous) {
    console.error(`[docs-validate] duplicate docs id "${contentId}": ${previous} and ${file}`);
    failed = true;
  } else {
    contentIds.set(contentId, file);
  }

  const text = readFileSync(file, 'utf8');
  for (const rule of banned) {
    if (rule.re.test(text)) {
      console.error(`[docs-validate] ${file}: ${rule.message}`);
      failed = true;
    }
  }

  for (const { filter, command, keys } of searchEnvelopeMisreads(text)) {
    console.error(
      `[docs-validate] ${file}: \`${command}\` emits an envelope keyed by ${keys}; ` +
        `jq must enter through one of those (found \`${filter}\`)`,
    );
    failed = true;
  }
}

const homepage = join(docsRoot, 'index.mdx');
const homepageText = readFileSync(homepage, 'utf8');

if (/—/.test(homepageText)) {
  console.error(`[docs-validate] ${homepage}: em dashes are banned in homepage copy`);
  failed = true;
}

// Smart typography rewrites `--` to an em dash and `"` to curly quotes inside
// the homepage's raw HTML nodes, silently breaking copy-paste. `{`...`}` opts out.
const commandNodePatterns = [
  /<code(?:\s[^>]*)?>([^<]*)<\/code>/g,
  /<span class="(?:search-string|search-cmd|run-cmd|hero-install-cmd)"[^>]*>([^<]*)<\/span>/g,
];
for (const pattern of commandNodePatterns) {
  for (const [node, inner] of homepageText.matchAll(pattern)) {
    if (inner.startsWith('{`')) continue;
    if (/--|"/.test(inner)) {
      console.error(
        `[docs-validate] ${homepage}: ${node} needs the {\`...\`} form, ` +
          'or smart typography will mangle its flags and quotes',
      );
      failed = true;
    }
  }
}

const openapi = JSON.parse(readFileSync(openapiPath, 'utf8'));
const pathCount = Object.keys(openapi.paths || {}).length;
if (pathCount === 0) {
  console.error('[docs-validate] OpenAPI spec has no paths');
  failed = true;
}

if (failed) process.exit(1);
console.log(`[docs-validate] ok (${pathCount} OpenAPI paths)`);
