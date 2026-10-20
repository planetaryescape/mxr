import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tool = resolve(process.env.RELEASE_PLEASE_DIR || join(repo, '.openapi/tooling/node_modules/release-please'));
const requireTool = createRequire(join(tool, 'package.json'));
// Load the package entry point first: importing Simple directly creates a
// BaseStrategy/manifest/factory cycle in this release-please version.
const { VERSION: toolVersion } = requireTool(tool);
const { Simple } = requireTool('./build/src/strategies/simple.js');
const { Version } = requireTool('./build/src/version.js');
const artifact = 'site/public/openapi.json';
const script = join(repo, 'site/scripts/dump-openapi-spec.sh');

test('real release-please extra-file updaters keep a version-only release fresh', async (t) => {
  // The action's v5 lockfile bundles release-please 17.6.0. No GitHub client,
  // token or network-backed SCM method is used by this offline release fixture.
  assert.equal(toolVersion, '17.6.0');
  const config = JSON.parse(readFileSync(join(repo, 'release-please-config.json'), 'utf8')).packages['.'];
  const original = new Map(['Cargo.toml', 'Cargo.lock', artifact].map((path) => [path, readFileSync(join(repo, path))]));
  const current = JSON.parse(original.get(artifact).toString('utf8')).info.version;
  const previous = Version.parse(current);
  const next = new Version(previous.major, previous.minor, previous.patch + 1);
  const strategy = new Simple({
    github: { repository: { owner: 'offline-fixture', repo: 'mxr' } },
    targetBranch: 'main',
    includeComponentInTag: false,
    skipChangelog: true,
    extraFiles: config['extra-files'],
    changelogNotes: { buildNotes: async () => 'Version-only offline fixture\nNo route or schema changes.\n' },
  });
  const candidate = await strategy.buildReleasePullRequest([], undefined, false, [], { newVersion: next });
  const changed = new Map();
  function restore() {
    // Restore only this fixture's own writes; never discard an unexpected edit.
    for (const [path, written] of changed) {
      assert.deepEqual(readFileSync(join(repo, path)), written, `concurrent edit to ${path}; refusing to overwrite it`);
      writeFileSync(join(repo, path), original.get(path));
    }
  }
  const interrupt = () => { restore(); process.exit(130); };
  const terminate = () => { restore(); process.exit(143); };
  process.once('SIGINT', interrupt);
  process.once('SIGTERM', terminate);
  try {
    for (const update of candidate.updates) {
      if (!original.has(update.path)) continue;
      const content = update.updater.updateContent(original.get(update.path).toString('utf8'));
      changed.set(update.path, Buffer.from(content));
      writeFileSync(join(repo, update.path), content);
    }
    const checked = spawnSync('/bin/bash', [script, '--check'], {
      cwd: repo, env: process.env, encoding: 'utf8',
    });
    const metadata = spawnSync('cargo', ['metadata', '--locked', '--format-version', '1', '--no-deps'], {
      cwd: repo, env: process.env, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    });
    assert.equal(metadata.status, 0, metadata.stderr);
    const exporter = join(JSON.parse(metadata.stdout).target_directory, 'debug/examples/dump_openapi_spec');
    const exported = spawnSync(exporter, [], { cwd: repo, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
    assert.equal(exported.status, 0, exported.stderr);
    const spec = JSON.parse(exported.stdout);
    const updated = JSON.parse(readFileSync(join(repo, artifact), 'utf8'));
    t.diagnostic(JSON.stringify({ toolVersion, previousVersion: current, releaseVersion: next.toString(), exportedVersion: spec.info.version, storedVersion: updated.info.version, updaterPaths: [...changed.keys()] }));
    assert.equal(spec.info.version, next.toString(), 'the actual exporter must compile the fixture version');
    assert.equal(checked.status, 0, checked.stderr);
    assert.deepEqual(updated, spec, 'the actual updater must preserve all exported route/schema data');
    assert.deepEqual([...changed.keys()].sort(), ['Cargo.lock', 'Cargo.toml', artifact].sort());
  } finally {
    process.removeListener('SIGINT', interrupt);
    process.removeListener('SIGTERM', terminate);
    restore();
  }
});
