# Owned-address changes do not invalidate cached summaries and triage

**Status:** open, deferred · **Found:** cross-model review of the owned-address
refactor (rounds 1 and 2) · **Severity:** should-fix

## What happens

Thread summaries and triage results are cached by a content hash. The owned
address set is an input to both prompts, but neither cache key includes it.

- Thread summary: `crates/daemon/src/handler/summarize.rs:107-109` builds the
  key from `thread_summary_content_hash`, `relationship_hash` and
  `SUMMARY_PROMPT_VERSION`. `relationship_hash` covers contacts that are not
  owned, so an alias that is registered or promoted does not change the key.
  The owned list is also printed in the prompt ("Account owner addresses"),
  so a cached summary can describe the old ownership.
- Triage: `crates/daemon/src/handler/triage.rs:256` hashes only the envelope
  and body text. The owned-address block in the prompt is not in the key.

A cached result made before an alias was added, or before a primary was
removed, is served until the message or relationship changes.

## Why it is deferred

Changing the key means changing every cached record, so it goes in its own
PR. The owned-address refactor does not alter the prompt wording, so existing
records stay correct except where ownership changed.

## Fix sketch

Add an owned-address revision (a hash of the sorted owned set from
`owned_addresses_for`) to both keys. Bump `SUMMARY_PROMPT_VERSION` so existing
summaries are regenerated once. Triage needs the same revision in
`triage_content_hash`.
