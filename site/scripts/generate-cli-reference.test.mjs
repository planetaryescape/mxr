import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { generateReference } from './generate-cli-reference.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'mxr-cli-reference-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const snapshots = join(root, 'snapshots');
  const output = join(root, 'output');
  mkdirSync(snapshots);
  mkdirSync(output);
  writeFileSync(join(output, 'previous.md'), 'last successful generation');
  writeFileSync(join(output, 'concepts.md'), 'handwritten concepts');
  function snapshot(name, body) {
    writeFileSync(join(snapshots, `cli_help__cli_help_${name}.snap`), `---\nsource: fixture\n---\n${body}\n`);
  }
  return { snapshots, output, snapshot };
}

test('generation requires the root reference input', (t) => {
  const { snapshots, output } = fixture(t);
  assert.throws(() => generateReference(snapshots, output), /cli_help_root.snap missing/);
  assert.equal(readFileSync(join(output, 'previous.md'), 'utf8'), 'last successful generation');
});

test('generation rejects a newly listed command without its own help before replacing output', (t) => {
  const { snapshots, output, snapshot } = fixture(t);
  snapshot('root', 'Usage: mxr [COMMAND]\n\nCommands:\n  added  A new command\n  help  Print help');
  assert.throws(() => generateReference(snapshots, output), /Missing help snapshot for mxr added/);
  assert.equal(readFileSync(join(output, 'previous.md'), 'utf8'), 'last successful generation');
});

test('generation rejects a missing nested help input at any depth', (t) => {
  const { snapshots, output, snapshot } = fixture(t);
  snapshot('root', 'Usage: mxr [COMMAND]\n\nCommands:\n  llm  Model configuration');
  snapshot('llm', 'Usage: mxr llm [COMMAND]\n\nCommands:\n  key  Manage keys');
  assert.throws(() => generateReference(snapshots, output), /Missing help snapshot for mxr llm key/);
  snapshot('llm_key', 'Usage: mxr llm key <COMMAND>\n\nCommands:\n  status  Key status');
  assert.throws(() => generateReference(snapshots, output), /Missing help snapshot for mxr llm key status/);
});

test('complete snapshots render own and nested options and preserve handwritten concepts', (t) => {
  const { snapshots, output, snapshot } = fixture(t);
  snapshot('root', 'Usage: mxr [COMMAND]\n\nCommands:\n  llm  Model configuration\n  help  Print help');
  snapshot('llm', 'Usage: mxr llm [COMMAND]\n\nCommands:\n  key  Manage keys\n  help  Print help\n\nOptions:\n  --format <FORMAT>  Output format');
  snapshot('llm_key', 'Usage: mxr llm key <COMMAND>\n\nCommands:\n  status  Key status');
  snapshot('llm_key_status', 'Usage: mxr llm key status\n\nOptions:\n  --show-provider  Include provider name');
  generateReference(snapshots, output);
  const page = readFileSync(join(output, 'llm.md'), 'utf8');
  assert.match(page, /`--format <FORMAT>`/);
  assert.match(page, /### `mxr llm key status`/);
  assert.match(page, /`--show-provider`/);
  assert.equal(readFileSync(join(output, 'concepts.md'), 'utf8'), 'handwritten concepts');
});

test('the selected command pages publish their curated examples', (t) => {
  const { snapshots, output, snapshot } = fixture(t);
  const names = [
    'assistant', 'backup', 'categories', 'count', 'draft', 'headers', 'humanize',
    'jobs', 'mcp', 'profile', 'recovery', 'restart', 'route', 'signatures',
    'snoozed', 'threads', 'triage', 'undo', 'voice',
  ];
  const examples = JSON.parse(readFileSync(new URL('./cli-reference-examples.json', import.meta.url), 'utf8'));
  assert.deepEqual(Object.keys(examples).sort(), [...names].sort());
  snapshot('root', `Usage: mxr [COMMAND]\n\nCommands:\n${names.map((name) => `  ${name}  Command help`).join('\n')}`);
  for (const name of names) snapshot(name, `Usage: mxr ${name} [OPTIONS]`);
  generateReference(snapshots, output);
  for (const name of names) {
    const page = readFileSync(join(output, `${name}.md`), 'utf8');
    assert.match(page, /## Use When/);
    assert.match(page, /## Everyday Examples/);
    for (const example of examples[name].examples) assert.ok(page.includes(example), example);
  }
});

test('legacy snapshot names retain canonical hyphens and do not duplicate alias sections', (t) => {
  const { snapshots, output, snapshot } = fixture(t);
  snapshot('root', 'Usage: mxr [COMMAND]\n\nCommands:\n  daemon  Run daemon');
  snapshot('daemon', 'Usage: mxr daemon [COMMAND]\n\nCommands:\n  dial-stdio  Connect stdio');
  snapshot('daemon_dial_stdio', 'Usage: mxr daemon dial-stdio\n\nOptions:\n  --socket <PATH>  Socket path');
  snapshot('daemon_alias', 'Usage: mxr daemon dial-stdio\n\nOptions:\n  --socket <PATH>  Socket path');
  generateReference(snapshots, output);
  const page = readFileSync(join(output, 'daemon.md'), 'utf8');
  assert.equal(page.match(/### `mxr daemon dial-stdio`/g)?.length, 1);
  assert.match(page, /`--socket <PATH>`/);
});
