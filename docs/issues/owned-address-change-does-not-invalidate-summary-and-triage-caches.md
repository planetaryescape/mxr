# Owned-address changes do not invalidate cached summaries and triage

**Status:** fixed · **Found:** cross-model review of the owned-address
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

## Fixed

The thread-summary key (`valid_cached_summary` and `load_summary_context` in
`crates/daemon/src/handler/summarize.rs`) and the triage content hash
(`triage_content_hash` in `crates/daemon/src/handler/triage.rs`) both include
`owned_addresses_revision` from `crates/daemon/src/state.rs`. The revision
covers the primary as well as the sorted, lowercased set, because both prompts
mark the primary, so a primary change must miss too. Test:
`summary_cache_misses_after_an_owned_alias_is_added`,
`triage_content_hash_changes_when_the_owned_set_changes`.

Triage reads the owner's addresses once. The same snapshot feeds the hash and the prompt, and a failed read fails the search instead of hashing an empty set. The summary check reads the thread's account, the same one the writer uses, and a failed read is a cache miss. Test: `a_thread_spanning_two_accounts_validates_under_the_account_its_summary_names`.

`SUMMARY_PROMPT_VERSION` is not bumped. The new key already misses every
existing row, and the prompt wording did not change, so a bump would add
nothing. Neither cache needs pruning: `thread_summaries` holds one row per
thread (`ON CONFLICT(thread_id)`) and `triage_cache` one per message and prompt
version (`ON CONFLICT(message_id, prompt_version)`), so a regenerated record
replaces the old one in place.
