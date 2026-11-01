# Provider filing and mode state

Use this reference when changing sync label writes or the candidate reads behind
Updates and Reading. The public [mode guide](../../site/src/content/docs/guides/email-modes.md)
owns the user workflow; the [JSON reference](../../site/src/content/docs/reference/json-output.md#source-filing-evidence)
owns `source_filed_at`. This contract was checked at `v0.6.199`.

## State ownership

Provider inbox membership, mode completion and row attention are separate state.
A sync-observed inbox removal does not write a mode done mark or complete a task.
Updates and Reading can retain an eligible source outside the inbox, subject to
their existing done, snooze, source and expiry rules. Reading's Later shelf has
its own saved membership.

`source_filed_at` is observation time. The label means mxr saw the source leave
the inbox; it does not establish when the provider changed it or who acted.
Arrival/source dates and task deadlines keep their own fields. Updates labels
the newest email in a folded line, even when an older email supplies the fact.
Parcel trackers have no single source email to label.

## Current evidence predicate

`EXTERNAL_FILING` in `crates/store/src/message_events.rs` requires:

- An inbox-removal event (`unlabeled` or `archived`) attributed to `sync`.
- No later relevant inbox transition for that message and label, from any source.
- No current link between the message and that inbox label.

The later-transition check uses the event row ID, so archive/undo in the same
second remain ordered. A return to the inbox clears the marker. A later local
archive must not revive an older sync removal. Identical label replay adds no
transition. First seeing an already-archived message is not an observed removal.

`Store::mode_candidates` unions real inbox candidates with these retained
sources, excluding Trash and Spam. The daemon's classification and per-mode
filters still decide which rows to show. `Store::place_candidates` remains an
inbox-only store read; do not widen every archived-mail query to imitate the mode
list. Check both direct membership and the actual digest/edition projection.

## Write paths

`replace_message_labels_tx` in `crates/store/src/message.rs` compares old and new
inbox membership and records any transition in the same transaction as the label
replacement. `set_message_labels` passes its actual event source; batched
`apply_sync_upserts` passes `Sync`. Both callers take `BEGIN IMMEDIATE` before
reading, so a deferred WAL read snapshot does not have to upgrade to a writer.

Incremental add/remove paths also record transitions. Preserve their attribution
and no-change behavior when adding another reconciliation path. Recording only
incremental sync would leave full-sync observations invisible to the modes.

## Verify the contract

Run the store regressions and actual mode-list regressions:

```bash
scripts/cargo-test -p mxr-store --lib a_source_filed_by_sync_is_marked_filed_and_an_mxr_archive_is_not
scripts/cargo-test -p mxr-store --lib batch_sync_records_inbox_departures_without_inventing_historical_archives
scripts/cargo-test -p mxr-store --lib candidates_are_inbox_mail_with_pins
scripts/cargo-test -p mxr --lib externally_filed_
scripts/cargo-test -p mxr-tui --lib filed_
```

The store tests cover stale local-archive evidence, bulk sync, unchanged replay,
account scope and pre-archived history. The daemon tests prove retained items
reach the digest/edition and that explicit let-go still clears them. TUI tests
keep the marker visible with long content and on Reading's Later shelf.
These fixtures do not establish behavior against a live personal mailbox.

## See also

- [Email mode model](../blueprint/22-email-modes.md)
- [Updates guide](../../site/src/content/docs/guides/updates.md)
- [Reading guide](../../site/src/content/docs/guides/reading.md)
