# S02 review at head `010ae951` / tree `c04fbff5`: GREEN, no blockers, six should-fix items

I found no blocker. The core S02 guarantees hold in the code. SQL compare-and-set (CAS) guards every content writer, and no path ever takes the draft lock after the provider lock. The daemon rejects stale or missing revisions before any provider effect. Same-content create replays and changed content conflicts. New clients check for revision support, and old unconditional writes fail. Recovery sessions can't trigger send, safety, schedule or provider save. All findings below come from reading the code, not from running it. Item 6 is an untested hypothesis.

Item 1 is the one I'd fix before release. It is a small fix in restore.

## Should-fix

### 1. An unedited reopened draft can send from a different address than the composer shows
- **Severity:** should-fix, high.
- **Cause:** Opening a saved draft from Drafts writes the account's primary address into the compose file and ignores the draft's own From (`crates/web/src/lib.rs:2986-2991`). The shared helper `frontmatter_from_draft` in `crates/compose/src/draft_codec.rs:59-70` already handles this correctly.
- **Why send diverges:** S02 changed browser send to reuse the draft's stored identity (`lib.rs:976-995`). For a draft that already exists, the daemon only checks the revision and then sends the stored row (`crates/daemon/src/handler/mutations.rs:3271-3279`, `3355-3359`). The base code used a fresh id on purpose, to avoid exactly this split.
- **Why nothing gets saved first:** loading the draft marks it as saved (`useComposeSession.ts:172-176`), so the send pipeline makes no update request (`useComposeSend.ts:228-233`).
- **Repro:** Reply to a message that arrived on an alias. Replies take their From from the original message (`lib.rs:2431`), so the stored draft is From the alias. Close the composer, reopen the draft from Drafts (it now shows the primary address), then send without editing. The message goes out From the alias. The composer and the safety check both saw the primary.
- **Impact:** The sender address differs from the one displayed. This contradicts `s02-draft-revisions.md:61-62`, which says send uses "the same parsed snapshot". I don't rate it a blocker for three reasons:
  - The address sent is the draft's own stored, ownership-checked From.
  - The daemon re-runs safety checks on what it actually sends.
  - Revision checks still stop another editor's content from going out.
- **Fix:** Render the draft's From in restore. Optionally, have send save the parsed snapshot first, as schedule does (`lib.rs:1297-1311`).

### 2. Typing during an explicit Retry leaves the newer text stuck behind the editor's own accepted save
- **Severity:** should-fix. No data loss.
- **Where:** `useDraftSaveActions.ts:104-121`. If the retry's result is superseded, the code returns silently (line 106). If the text changed during the retry, it throws (lines 114-120). Either way the editor stays on the recovery file at the old revision.
- **Why it can't recover on its own:** The bridge writes the new revision only to the retry copy's metadata (`lib.rs:882-902`). It also rejects any recovery autosave whose revision doesn't match the recovery's own metadata (`lib.rs:860-864`).
- **Repro:** Have a recovery at revision 1. Click Retry and keep typing until the 3s autosave fires. The daemon accepts revision 2 with the text as it was when you clicked. The editor stays at revision 1, and the next Retry gets a 409 saying "Draft changed in another editor", which is the user's own write. The test "typing during an explicit recovery retry keeps the latest private text and original token" encodes this behaviour.
- **Impact:**
  - The newer typing can only reach the saved draft by copying it by hand after "Open saved draft".
  - The accepted retry copy is left on disk with nothing pointing to it.
  - The superseded path gives the user no feedback at all.
- **Fix:** Do what canonical autosave already does (`useComposeAutosave.ts:316-322`): adopt the accepted path and revision, and keep the newer typing as unsaved changes.

### 3. Linked drafts now push to the provider on every browser autosave
- **Severity:** should-fix, or a decision to make consciously.
- **Change:** Autosave now goes through `UpdateDraft` (`lib.rs:910-925`, `2714-2718`). For a draft linked to the provider (Gmail, for example), that handler updates the provider while holding the account's provider lock. If the provider call fails, it returns before writing locally (`mutations.rs:2519-2563`). Base autosave only wrote the file (`FULL-DIFF.patch:8006-8034`).
- **Repro:** Open a Gmail-linked draft, go offline and type. Each autosave fails and the editor drops into "preserved locally". Retry also fails until the provider is reachable again.
- **Impact:** Local-first editing of linked drafts is lost. There is one provider round trip per 3-second autosave, and each one blocks that account's sync.

### 4. Discard now fails instead of being idempotent in two cases
- **Severity:** should-fix. The user is left with a Discard that never works; Close still works.
- **Missing file:** The new file lock resolves the path first and errors if the file is gone (`lib.rs:158-172`, used at `lib.rs:1402-1404`). The base removal ignored a missing file (`crates/compose/src/lib.rs:125-131`). Repro: open the same compose intent in two tabs, send from tab A, then Discard in tab B.
- **Draft deleted elsewhere:** The daemon's revision check returns "Draft not found" (`mutations.rs:2437-2441`). The bridge propagates that error before removing any files (`lib.rs:1405-1419`), so the composer stays open (`useComposeSession.ts:340-348`).

### 5. `mxr drafts orphaned` still advertises a delete that now always fails
- **Severity:** should-fix.
- **Where:** The CLI prints "Delete any with: mxr drafts delete <draft-id>" (`compose.rs:866`). Delete now refuses drafts whose status isn't `draft` (`mutations.rs:2448-2450`, and `crates/store/src/draft.rs:325` checks `status = 'draft'`).
- **Regression:** Base deleted unconditionally (`FULL-DIFF.patch:5521-5545`).
- **Fix:** Change the hint to "resume, then delete", or decide in S03 whether deleting a draft stuck mid-send should be allowed.

### 6. Hypothesis: a transport error while reopening can lose the link to preserved edits
- **Severity:** should-fix. Untested.
- **Where:** `activeDrafts.ts:31-57` forgets the remembered draft entry on any refresh or restore error, including when the bridge is unreachable.
- **Impact:** The forgotten entry may hold a recovery path or the "Open preserved edits" path. The text stays in the private scratch directory but nothing in the UI points to it. The catch-and-forget behaviour predates S02; S02 puts more at stake in that entry.

## Nits
- **Capability check on every draft write.** Each draft write first sends a full `GetStatus` (`crates/client/src/lib.rs:268-289`). That includes per-account message counts and a search probe (`status_helpers.rs:12-38`, `server.rs:790-807`). If status itself fails (`diagnostics/mod.rs:1198`), the user is wrongly told to upgrade the daemon.
- **Conflict message text.** It shows Rust internals: "expected Some(1), current Some(2)" (`error.rs:20`, `:43`). In the browser, item 2's self-conflict is reported as "another editor".
- **API docs.** The regenerated OpenAPI drops the summaries for several compose endpoints and for save-local and delete-stored, and drops their 401 responses. Send and safety-check 200 responses have no body schema.
- **Activity volume.** Every browser autosave now records a `draft.update` activity row (`mapper.rs:349-360`). Only the draft id is stored, so there's no privacy issue, but the volume rises from zero to one row per autosave.
- **Daemon lock map.** The per-draft lock map in the daemon (`state.rs:1203-1215`) never shrinks. This is the same accepted tradeoff as the bridge's lock map.
- **Hypothesis: failed attachment copy.** If a bridge-managed attachment was deleted by another session, `copy_recovery_attachments` fails (`lib.rs:~3127`). No recovery file is written, so the text exists only in the browser buffer.

## Checks that held
- **Store writers.** Update, delete and reply-header writes all use CAS (`draft.rs:239-266`, `319-331`, `401-417`). Insert is a plain `INSERT`, not an upsert. I enumerated every production writer and none bypasses the revision.
- **Order and checks before provider effects.** The draft lock is taken before the provider lock, and the revision check runs before any provider effect, in:
  - update (`mutations.rs:2502` → `2519`)
  - delete (`2599`)
  - provider save (`3866` → `3887`)
  - reconciliation (`2649-2720`)
- **No deadlock.** The sync loop drops the provider guard before reconciling (`loops.rs:568-579`). No caller of `acquire_provider_operation` then takes a draft lock.
- **Create replay.** `create_draft_locked` compares content with revision and timestamps normalised, so the same content replays and changed content conflicts (`2454-2473`).
- **Version handling.**
  - New clients check daemon support before draft writes, and the CLI, TUI, MCP and bridge all go through that check.
  - Only one production status constructor exists, and it sets the support flag (`diagnostics/mod.rs:1228-1229`).
  - Old update, delete and provider-save requests conflict.
- **Bridge effects.** Send, safety check, provider save and schedule refuse recovery sessions and check the observed revision.
- **Bridge autosave.** Canonical autosave commits the file only after the daemon accepts it, and preserves a recovery file otherwise (`lib.rs:910-950`). Recovery autosave is local-only (`857-909`).
- **MCP.** Both provider-save tool names require the revision returned by their preview. Delete passes the expected revision through, and a missing one conflicts.
- **CLI and TUI.** Editors use unique scratch files and report the retained path on failure (`compose.rs:1109-1139`, `tui/compose_flow.rs:263-267`, `329-349`).
- **Migration and activity.** Migration 072 is idempotent (`pool.rs:1084-1091`). Activity context records only the draft id.

## Practical limits
- I ran nothing: no tests, no git, no builds.
- I read the working tree in this directory and relied on the manifest for head `010ae9518c5ee9bf96c194628483e10a0f0ac4ce` and tree `c04fbff55205d5f8bcb8fa9048bb8f07953e4642`. I couldn't verify those SHAs myself.
- I skimmed the mechanical revision additions in test fixtures and didn't read every frontend test end to end.
- The 6c9a22e8 browser journey and the Node 24 test results are your evidence; I didn't re-verify them.
