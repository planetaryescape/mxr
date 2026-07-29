// `mxr search --format json` emits one of two envelopes, and which one you get
// depends on the flags. A message search returns `results`/`paging`/`explain`;
// `--group-by` returns an aggregation of `query`/`group_by`/`total`/`groups`.
// A jq filter has to enter through the envelope its own command produced, so
// the check reads the flags between `mxr search` and the pipe. Backslash
// continuations push the filter onto later lines, so join them before matching.
const searchJsonJq =
  /mxr search\b([^\n]*?--format json(?![a-z])[^\n]*?)\|\s*jq\s+(?:-[a-zA-Z]+\s+)*['"]?\s*([^\s|'"]+)/g;

const messageEnvelope = {
  command: 'mxr search --format json',
  keys: new Set(['results', 'paging', 'explain']),
};
const aggregationEnvelope = {
  command: 'mxr search --group-by ... --format json',
  keys: new Set(['query', 'group_by', 'total', 'groups']),
};

/**
 * Find jq filters that read a `mxr search --format json` pipeline through a key
 * the command's envelope does not have.
 */
export function searchEnvelopeMisreads(text) {
  const joined = text.replace(/\\\n\s*/g, ' ');
  const misreads = [];
  for (const [, flags, filter] of joined.matchAll(searchJsonJq)) {
    if (filter === '.') continue;
    const envelope = /--group-by\b/.test(flags) ? aggregationEnvelope : messageEnvelope;
    const key = /^\.([A-Za-z_][A-Za-z0-9_]*)/.exec(filter);
    if (key !== null && envelope.keys.has(key[1])) continue;
    misreads.push({
      filter,
      command: envelope.command,
      keys: [...envelope.keys].map((name) => `.${name}`).join(', '),
    });
  }
  return misreads;
}
