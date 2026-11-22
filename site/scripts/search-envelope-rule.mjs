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
