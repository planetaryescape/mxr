# Draft recovery still has three verified limits

Verified against integrated S02 `a631583c8fd8438abc6892fdf10fb49dc09e3748`, tree `3a903ddea1843b4c380cd81adf344882e7d39c42`, after the final five-file correction. These remaining behaviors do not replace the [release acceptance](../build-log/daemon-portability/s02-draft-recovery.md).

## Typing during Retry can require copying into the accepted draft

`apps/web/src/features/compose/session/useDraftSaveActions.ts:104-121` refuses
to adopt an accepted retry if newer typing superseded it. The latest text and
owned attachment paths remain in the original private recovery at its original
revision, as covered by `useRecoveryAttachments.test.tsx:350`. Another Retry
can then conflict with the user's own accepted earlier snapshot. The current
error directs the user to open the saved draft; the newer text needs to be
copied across.

A future change should adopt only the confirmed accepted path/revision while
retaining newer typing as unsaved. It must handle queued private autosaves and
attachment ownership before switching paths. Prove that the latest text is
saved once, with no false Saved indicator or loss of the previous recovery.

## Provider-linked autosave cannot update canonical storage offline

`crates/web/src/lib.rs:2714` submits existing drafts through `UpdateDraft`.
For a linked provider draft, `crates/daemon/src/handler/mutations.rs:2519-2563`
updates the provider before writing local canonical content. If that call fails,
the browser retains private recovery and reports that state. The new text does
not become the canonical stored draft until an explicit retry succeeds.

Local-first editing would improve if canonical saves did not require that
provider round trip. Track unsynced local content and define its reconciliation
with provider changes before removing the provider call; otherwise a provider
pull could overwrite the newly saved local content. Cover offline edits,
reconnect, changed provider revision and separate explicit provider save.

## Discard should tolerate a draft already removed elsewhere

`crates/web/src/lib.rs:158-172` canonicalizes the compose file for locking, so
a missing file fails. Its discard handler at lines 1402-1419 also propagates
a missing canonical draft before removing the editor's files. Closing remains
available, and private content is retained, but Discard cannot finish in those
states.

Make absence idempotent while preserving revision conflicts for a draft that
still exists. Verify two tabs where one sends or deletes before the other
discards. Recovery-only discard must continue to remove only its own files and
attachment bytes.
