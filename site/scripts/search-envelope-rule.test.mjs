import assert from 'node:assert/strict';
import test from 'node:test';
import { searchEnvelopeMisreads } from './search-envelope-rule.mjs';

const accepted = [
  ["mxr search 'q' --format json | jq .", 'whole-document filter'],
  ["mxr search 'q' --format json | jq '.results[0]'", 'results'],
  ["mxr search 'q' --format json | jq -r '.results[].from'", 'results with a jq flag'],
  ["mxr search 'q' --limit 200 --format json | jq '.paging | {total, has_more}'", 'paging'],
  ["mxr search 'q' --explain --format json | jq '.explain.executed_mode'", 'explain'],
  [
    "mxr search 'q' --format json | jq -r '.results[] | [.message_id, .subject] | @tsv'",
    'results feeding a piped jq expression',
  ],
  ["mxr search 'q' --group-by from --format json | jq '.groups[]'", 'groups'],
  ["mxr search 'q' --group-by list --format json | jq -r '.total'", 'aggregation total'],
  ["mxr search 'q' --format json \\\n  | jq -r '.results[].subject'", 'results across a continuation'],
  [
    "mxr search 'q' --group-by category --format json \\\n  | jq -r '.groups[].key'",
    'groups across a continuation',
  ],
  ["mxr search 'q' --format jsonl 2>/dev/null | jq -r '.subject'", 'jsonl records are bare'],
  ["mxr search 'q' --format ids | mxr archive --yes", 'ids output has no jq stage'],
];

const rejected = [
  ["mxr search 'q' --format json | jq '.groups[]'", 'aggregation key on a message search'],
  ["mxr search 'q' --format json | jq -r '.total'", 'aggregation total on a message search'],
  ["mxr search 'q' --format json | jq '.[0]'", 'bare-array assumption'],
  ["mxr search 'q' --group-by from --format json | jq '.results[]'", 'message key on an aggregation'],
  ["mxr search 'q' --group-by from --format json | jq '.paging.next_offset'", 'paging on an aggregation'],
  [
    "mxr search 'q' --format json \\\n  --sort relevance \\\n  | jq '.groups[].count'",
    'aggregation key across continuations',
  ],
  [
    "mxr search 'q' --group-by list --format json \\\n  | jq -r '.results[].from'",
    'message key across a continuation',
  ],
];

test('valid jq entry points pass', () => {
  for (const [example, description] of accepted) {
    assert.deepEqual(searchEnvelopeMisreads(example), [], description);
  }
});

test('cross-mode jq filters are reported', () => {
  for (const [example, description] of rejected) {
    assert.equal(searchEnvelopeMisreads(example).length, 1, description);
  }
});

test('the report names the envelope the command produces', () => {
  const [search] = searchEnvelopeMisreads("mxr search 'q' --format json | jq '.groups[]'");
  assert.equal(search.command, 'mxr search --format json');
  assert.equal(search.keys, '.results, .paging, .explain');
  assert.equal(search.filter, '.groups[]');

  const [aggregation] = searchEnvelopeMisreads(
    "mxr search 'q' --group-by from --format json | jq '.results[]'",
  );
  assert.equal(aggregation.command, 'mxr search --group-by ... --format json');
  assert.equal(aggregation.keys, '.query, .group_by, .total, .groups');
  assert.equal(aggregation.filter, '.results[]');
});
