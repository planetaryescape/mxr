# Provider legacy sync cursor shims have no removal date

The IMAP and Gmail adapters still decode cursor shapes from before the opaque, versioned cursor envelope.

## Where

- `crates/provider-imap/src/cursor.rs`: `decode_legacy`, `LegacyOutcome`, and the scalar `uid_validity` / `uid_next` reconstruction of a singleton INBOX entry.
- `crates/provider-gmail/src/cursor.rs`: `decode_legacy`, `LegacyOutcome`.

## What it works around

Cursors persisted by releases before the v1 envelope. The comments that explained these shims were deleted in the comment sweep. The Gmail module doc says the legacy path "can be removed once" old cursors are gone, but nothing says when that is.

## Smallest fix

Pick the release after which no store holds a pre-v1 cursor. Delete both `decode_legacy` paths and their tests. A cursor that fails to decode already becomes `SyncCursorExpired`, which runs a full resync.
