import { readFileSync, renameSync } from 'node:fs';
import { isDeepStrictEqual } from 'node:util';

const [mode, artifact, generated, before] = process.argv.slice(2);
if (!['check', 'update'].includes(mode) || !artifact || !generated || !before) {
  throw new Error('Expected mode, artifact, generated spec and pre-check snapshot paths.');
}

const exported = JSON.parse(readFileSync(generated, 'utf8'));
if (!exported.info?.version || !exported.paths || !exported.openapi) {
  throw new Error('Exporter did not produce an OpenAPI document; artifact was not changed.');
}

if (mode === 'check') {
  const original = readFileSync(before);
  const current = readFileSync(artifact);
  if (!original.equals(current)) {
    throw new Error('OpenAPI artifact changed during --check; retry from a stable input.');
  }
  if (!isDeepStrictEqual(JSON.parse(current.toString('utf8')), exported)) {
    throw new Error('OpenAPI artifact is stale. Run bash site/scripts/dump-openapi-spec.sh and commit site/public/openapi.json.');
  }
  console.log(`[dump-openapi-spec] current: ${artifact}`);
} else {
  renameSync(generated, artifact);
  console.log(`[dump-openapi-spec] wrote ${artifact}`);
}
