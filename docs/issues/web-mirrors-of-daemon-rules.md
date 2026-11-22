# The web app restates rules that the daemon and TUI also own

**Status:** open · **Severity:** low · **Found:** 2026-11-22 (delete-comment sweep, slice d-apps)

These pieces copy logic that lives elsewhere. They stay correct only while
someone changes both sides.

- `apps/web/src/features/freshness/copy.ts` mirrors `freshness_copy` in `crates/protocol/src/types/freshness.rs`.
- `apps/web/src/features/compose/assist/textRegion.ts` mirrors `split_body` in `crates/composer/src/check/regions.rs`; `textRegion.test.ts` repeats its cases.
- `apps/web/src/features/thread/InviteCard.tsx` mirrors the TUI card in `crates/tui/src/ui/message_view.rs` ("change one, change the other").
- `apps/web/src/features/mailbox/lenses.test.ts` assumes the sidebar ids in `crates/web/src/chrome.rs`.
- `apps/web/src/features/desk/DeskRow.tsx` mirrors `DeskLaneKind::shows_ask`.
- `apps/web/src/state/uiPrefsStore.ts` (`readReadingDisplay`) copies `parseReaderSettings` from `@planetaryescape/reader` to keep zod out of the entry chunk; `uiPrefsStore.test.ts` fails if they drift.
- `apps/web/src/features/rules/mailAction.test.ts` re-exports a parser that lives inline in `RuleEditorRoute`.

Smallest fix: have the daemon serve the derived value (freshness words, text
region cut, desk ask flag) through the bridge, or extract the pure logic into a
small shared module the entry chunk can import, then delete the copies.
