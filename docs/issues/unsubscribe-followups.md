# Unsubscribe follow-ups

**Status:** open · **Found:** 2026-10-22 during review of the unsubscribe receipt
work (G08a, round 2) on `fix/unsubscribe-receipts`

Two items were found in review and are out of scope for that change.

## Browser-open success is inferred from spawning the opener

`crates/daemon/src/unsubscribe.rs:50` reports `Success` as soon as
`open_in_browser` returns `Ok`. `open_in_browser` (`crates/daemon/src/unsubscribe.rs:81-91`)
only spawns `open` or `xdg-open` and never waits for it, so the receipt says
"confirmed" when the opener launched, not when the sender's page was shown or
acted on. A browser-based unsubscribe is therefore recorded as sent even if the
page never loads, and the claim blocks any later attempt for that message.

Options: keep it and say in the user-facing text that the page was opened, or
treat a browser-open as a handoff and leave the receipt unconfirmed. Either way
the receipt method for these cases should not read as a server-confirmed send.

## Dev databases that ran the earlier 093 have no foreign key

`crates/store/migrations/093_unsubscribe_receipts.sql:12` now declares
`message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE`. Migration
093 was edited in place before it was released, so only developer databases that
applied the earlier version lack the foreign key. On those, deleting a message or
account leaves its receipt behind. Only a dev database can be in that state, since
093 has not shipped. The fix is to drop and re-migrate those databases; no
production migration is needed.
