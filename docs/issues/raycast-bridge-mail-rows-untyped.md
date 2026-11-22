# Raycast hand-types the mailbox, search and thread rows

**Status:** open · **Severity:** low · **Found:** 2026-11-22 (delete-comment sweep, slice d-apps)

`apps/raycast/src/bridge/mail.ts` declares the row JSON that the mailbox,
search and thread bridge routes return, because the OpenAPI spec does not
describe those rows. The types mirror the web app's `features/mailbox/types.ts`,
so a bridge change can break Raycast without a type error.

Smallest fix: describe the rows in the bridge's OpenAPI spec, regenerate the
contract that `apps/raycast/src/bridge/types.ts` re-exports, and delete the
hand-written copies.
