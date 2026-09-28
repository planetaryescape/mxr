# Undo snapshots are taken before the provider lock

Status: logged, not fixed. No known occurrence.

## What

`apply_mutation_batch` (`crates/daemon/src/handler/mutations.rs`) loads
every envelope of the batch up front and builds each message's undo
snapshot from that copy, before `apply_mutation_to_envelope` takes the
account's provider lock (`state.acquire_provider_operation`). A mutation
can wait on that lock while another one runs.

If a label is added to the message while this mutation waits (a rule, a
sync, another client's label edit), the snapshot doesn't have it. Undo
restores the snapshot's label set, so it removes the label that arrived in
between.

## Why it is rare

- The wait is one provider call per message queued ahead on the same
  account, usually milliseconds.
- Undo is only offered for about 60 seconds, and only this batch's own
  messages are touched.

## Fix

Take the snapshot under the lock, from the store's copy at that moment:

1. Split `apply_mutation_to_envelope` so the caller holds the guard:
   acquire `acquire_provider_operation(&account_id)` in the batch loop
   (as the sweep path already does before `apply_mutation_under_guard`).
2. With the guard held, re-read the envelope (`store.get_envelope`) and
   build the `UndoEntrySnapshot` from that read, not from the batch's
   up-front copy.
3. Apply through `apply_mutation_under_guard` with the same guard, so no
   other mutation on the account can land between the snapshot and the
   change.
4. Test: hold the provider lock from the test, add a label to the message,
   release, and check that undo keeps the new label.

Sync writes labels without the provider lock, so a sync landing in the
same window still races; a per-message revision check at undo time (skip
labels added after `applied_at`) would close that too.
