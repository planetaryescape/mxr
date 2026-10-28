import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const script = join(repo, 'site/scripts/dump-openapi-spec.sh');
const source = join(repo, 'site/public/openapi.json');

test('OpenAPI CI checks the committed input before fixtures or lint copies', () => {
  const workflow = readFileSync(join(repo, '.github/workflows/openapi-conformance.yml'), 'utf8');
  const checked = workflow.indexOf('run: bash site/scripts/dump-openapi-spec.sh --check');
  const fixtureRun = workflow.indexOf('run: node --test scripts/openapi-release-version.test.mjs');
  const lintCopy = workflow.indexOf('cp site/public/openapi.json .openapi/spec.json');
  assert.ok(checked >= 0 && checked < fixtureRun && fixtureRun < lintCopy);
  for (const path of ['Cargo.toml', 'Cargo.lock', 'site/public/openapi.json', 'site/scripts/dump-openapi-spec.sh', 'site/scripts/openapi-artifact.mjs', 'scripts/openapi-*.test.mjs', 'release-please-config.json']) {
    assert.equal(workflow.split(`- "${path}"`).length - 1, 2, `${path} must select push and PR checks`);
  }
  assert.ok(workflow.includes('schemathesis run --dry-run --validate-schema=true'));
  assert.ok(workflow.includes('openapi_documented_paths_are_reachable_by_documented_method'));
});

function fixture(t) {
  const parent = join(repo, 'dist/openapi-tests');
  mkdirSync(parent, { recursive: true });
  const root = mkdtempSync(join(parent, 'case-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const artifact = join(root, 'artifact.json');
  copyFileSync(source, artifact);
  return { root, artifact };
}

function run(artifact, check = true, env = process.env) {
  return spawnSync('/bin/bash', [script, ...(check ? ['--check'] : []), artifact], {
    cwd: repo, env, encoding: 'utf8',
  });
}

test('the committed artifact matches the real exporter without being rewritten', (t) => {
  const { artifact } = fixture(t);
  const before = readFileSync(artifact);
  const result = run(artifact);
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(readFileSync(artifact), before);
});

for (const kind of ['route', 'schema', 'version']) {
  test(`the real check rejects a stale ${kind} without refreshing its input`, (t) => {
    const { artifact } = fixture(t);
    const spec = JSON.parse(readFileSync(artifact, 'utf8'));
    if (kind === 'route') delete spec.paths[Object.keys(spec.paths)[0]];
    if (kind === 'schema') delete spec.components.schemas[Object.keys(spec.components.schemas)[0]];
    if (kind === 'version') spec.info.version = '0.0.0-stale';
    writeFileSync(artifact, JSON.stringify(spec));
    const stale = readFileSync(artifact);
    const result = run(artifact);
    assert.equal(result.status, 1, result.stderr);
    assert.match(result.stderr, /OpenAPI artifact is stale/);
    assert.deepEqual(readFileSync(artifact), stale);
  });
}

test('formatting and object-key order do not count as drift; regeneration is byte-idempotent', (t) => {
  const { artifact } = fixture(t);
  const spec = JSON.parse(readFileSync(artifact, 'utf8'));
  writeFileSync(artifact, JSON.stringify(Object.fromEntries(Object.entries(spec).reverse()), null, 4));
  const checked = run(artifact);
  assert.equal(checked.status, 0, checked.stderr);
  const first = run(artifact, false);
  assert.equal(first.status, 0, first.stderr);
  const regenerated = readFileSync(artifact);
  const second = run(artifact, false);
  assert.equal(second.status, 0, second.stderr);
  assert.deepEqual(readFileSync(artifact), regenerated);
});

test('a missing artifact fails checking without creating one', (t) => {
  const { root } = fixture(t);
  const missing = join(root, 'missing.json');
  const result = run(missing);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /missing artifact/);
  assert.equal(existsSync(missing), false);
});

function fakeCargo(root, exporter, buildStatus = 0) {
  const bin = join(root, 'bin');
  const target = join(root, 'exporter-fixture');
  mkdirSync(bin);
  mkdirSync(join(target, 'debug/examples'), { recursive: true });
  const cargo = join(bin, 'cargo');
  writeFileSync(cargo, `#!/bin/bash\ncase "$1" in\n build) echo 'scheduler chatter'; exit ${buildStatus};;\n metadata) echo '{"target_directory":${JSON.stringify(target)}}';;\n *) exit 90;;\nesac\n`);
  chmodSync(cargo, 0o755);
  const helper = join(target, 'debug/examples/dump_openapi_spec');
  writeFileSync(helper, `#!/bin/bash\n${exporter}\n`);
  chmodSync(helper, 0o755);
  return { ...process.env, PATH: `${bin}:${process.env.PATH}` };
}

test('failed builds and partial or invalid exports preserve the previous artifact', (t) => {
  for (const [exporter, buildStatus, expected] of [
    ['echo ignored', 23, 23],
    ["printf '{partial'; exit 17", 0, 17],
    ["printf '{invalid'", 0, 1],
  ]) {
    const { root, artifact } = fixture(t);
    const before = readFileSync(artifact);
    const result = run(artifact, false, fakeCargo(root, exporter, buildStatus));
    assert.equal(result.status, expected, result.stderr);
    assert.deepEqual(readFileSync(artifact), before);
    assert.equal(readdirSync(root).some((name) => name.startsWith('.mxr-openapi.')), false);
  }
});

test('build chatter stays out of successful JSON output', (t) => {
  const { root, artifact } = fixture(t);
  const result = run(artifact, false, fakeCargo(root, `cat ${JSON.stringify(source)}`));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stderr, /scheduler chatter/);
  assert.deepEqual(JSON.parse(readFileSync(artifact, 'utf8')), JSON.parse(readFileSync(source, 'utf8')));
});

test('check mode rejects an artifact changed while exporting', (t) => {
  const { root, artifact } = fixture(t);
  const env = fakeCargo(root, `printf 'changed during export' > ${JSON.stringify(artifact)}\ncat ${JSON.stringify(source)}`);
  const result = run(artifact, true, env);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /changed during --check/);
  assert.equal(readFileSync(artifact, 'utf8'), 'changed during export');
});

test('array ordering and invalid artifact JSON are not silently normalized away', (t) => {
  const { root, artifact } = fixture(t);
  const spec = JSON.parse(readFileSync(artifact, 'utf8'));
  spec['x-fixture-order'] = ['first', 'second'];
  const exported = join(root, 'exported.json');
  writeFileSync(exported, JSON.stringify(spec));
  const env = fakeCargo(root, `cat ${JSON.stringify(exported)}`);
  spec['x-fixture-order'].reverse();
  for (const stale of [JSON.stringify(spec), '{invalid']) {
    writeFileSync(artifact, stale);
    const result = run(artifact, true, env);
    assert.equal(result.status, 1, result.stderr);
    assert.equal(readFileSync(artifact, 'utf8'), stale);
  }
});

test('without Cargo, publishing retains the artifact but strict checking fails', (t) => {
  const { root, artifact } = fixture(t);
  const bin = join(root, 'no-cargo-bin');
  mkdirSync(bin);
  symlinkSync('/usr/bin/dirname', join(bin, 'dirname'));
  const env = { ...process.env, PATH: bin };
  const before = readFileSync(artifact);
  assert.equal(run(artifact, false, env).status, 0);
  const checked = run(artifact, true, env);
  assert.equal(checked.status, 2);
  assert.match(checked.stderr, /Cargo is required/);
  assert.deepEqual(readFileSync(artifact), before);
});

test('cancellation removes temporary output without replacing the artifact', { timeout: 5_000 }, async (t) => {
  const { root, artifact } = fixture(t);
  const before = readFileSync(artifact);
  const env = fakeCargo(root, "echo exporter-ready >&2\nprintf '{partial'\nexec sleep 30");
  const child = spawn('/bin/bash', [script, artifact], { cwd: repo, env, detached: true, stdio: ['ignore', 'ignore', 'pipe'] });
  const closed = once(child, 'close');
  t.after(() => { if (child.exitCode === null && child.signalCode === null) process.kill(-child.pid, 'SIGKILL'); });
  let stderr = '';
  let terminationSent = false;
  // The shell can write termination diagnostics; keep its stderr pipe open through close.
  for await (const chunk of child.stderr) {
    stderr += chunk.toString();
    if (!terminationSent && stderr.includes('exporter-ready')) {
      terminationSent = true;
      process.kill(-child.pid, 'SIGTERM');
    }
  }
  const [code, signal] = await closed;
  assert.ok(code === 143 || signal === 'SIGTERM', `${code} ${signal}: ${stderr}`);
  assert.deepEqual(readFileSync(artifact), before);
  assert.equal(readdirSync(root).some((name) => name.startsWith('.mxr-openapi.')), false);
});
