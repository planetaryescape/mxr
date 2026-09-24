# Plan 013: Preserve draft edits across failures and retries

> Execute after plan 011 in a clean worktree. Every command below is a **future verification command, not run during authoring**. Follow the gates and stop conditions; the scope is recoverable scratch editing, not a new composer or an atomic cross-client concurrency protocol. Update this plan's index row unless the reviewer owns it.

## Status

- Priority: P1
- Effort: M
- Risk: MED. recovery must never silently overwrite retained text or a newer stored draft.
- Depends on: `plans/011-safe-verification-and-ci.md`
- Category: bug
- Planned at: fetched `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04.
- Planning evidence: source inspection only; the interrupted-edit regression has not been run.

## Why this matters

CLI and TUI preserve a failed edit initially, but both delete the same scratch filename when the user retries. A parse error or failed daemon update can therefore cost the latest text. The existing `drafts recover` command only covers stored drafts orphaned during sending, which does not protect these scratch edits.

## Current state and patterns

`crates/daemon/src/commands/mutations/compose.rs:1015`:

```rust
let path = mxr_compose::private_tmp::private_scratch_dir()?
    .join(format!("mxr-draft-edit-{}.md", draft.id));
// Clear any leftover from a prior aborted edit so the O_EXCL write succeeds.
let _ = std::fs::remove_file(&path);
mxr_compose::private_tmp::write_private(&path, content.as_bytes())?;
```

`crates/tui/src/compose_flow.rs:263` repeats this filename/removal sequence asynchronously. Its `handle_draft_edit_status` retains failed edits and sometimes prints the path; the next `prepare_draft_edit` removes them. CLI failures after editor exit propagate through `?` before cleanup, without consistently identifying the surviving file.

- `crates/compose/src/draft_codec.rs` already owns stored-Draft ↔ YAML/markdown conversion and preserves identity, account, and creation time. Reuse it.
- `crates/compose/src/private_tmp.rs` owns 0700 scratch directories and exclusive 0600 writes. Reuse it; no world-readable temp files or unchecked symlink reads.
- `crates/daemon/src/commands/mutations/compose.rs:728` lists `ListOrphanedDrafts`, specifically stale sending drafts. Do not change that JSON response to mix unrelated file records into it.
- `crates/tui/src/compose_flow.rs` contains fake-IPC compose tests; `crates/compose/src/draft_codec.rs` has round-trip tests; `crates/daemon/tests/compose_check_cli.rs` demonstrates isolated fake daemon CLI tests.
- No existing `base_hash` or scratch-edit recovery metadata was found in the fetched crates. Search again after the drift check before creating one.

Durable lesson from **First Run Is the Launch Surface**: partial failure must end in a repairable state. [NeoMutt's postponed-draft contract](https://docs.neomutt.org/howto/postponing-mail.html) demonstrates the user expectation: unsent work survives client restarts. We are repairing mxr's existing draft lifecycle, not copying its UI.

## Scope and drift check

Only modify:

```text
crates/compose/src/lib.rs
crates/compose/src/draft_codec.rs
crates/compose/src/private_tmp.rs
crates/compose/src/draft_edit.rs (new shared scratch helper)
crates/daemon/src/commands/mutations/compose.rs
crates/daemon/tests/cli_journey.rs
crates/tui/src/compose_flow.rs
crates/tui/src/async_result.rs
crates/tui/src/app/mod.rs
crates/tui/src/app/compose_helpers.rs
crates/tui/src/runner.rs
crates/tui/src/ui/drafts_modal.rs
site/src/content/docs/guides/linked-drafts.md
plans/README.md
```

No send semantics, web editor rewrite, draft schema migration, provider draft conflict resolution, IPC `UpdateDraft` contract, or sweeping cleanup. Shared scratch-file management belongs in `mxr-compose`; clients remain daemon clients. Use existing serde/YAML/UUID/time dependencies, not a custom serialization/locking framework.

Run `git fetch origin main` and `git rev-parse HEAD origin/main`; use updated remote plus plan 011. Then:

```bash
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/compose/src/lib.rs crates/compose/src/draft_codec.rs crates/compose/src/private_tmp.rs crates/compose/src/draft_edit.rs crates/daemon/src/commands/mutations/compose.rs crates/daemon/tests/cli_journey.rs crates/tui/src/compose_flow.rs crates/tui/src/async_result.rs crates/tui/src/app/mod.rs crates/tui/src/app/compose_helpers.rs crates/tui/src/runner.rs crates/tui/src/ui/drafts_modal.rs site/src/content/docs/guides/linked-drafts.md
```

Reconcile known prerequisite changes and stop on a changed draft lifecycle. Branch: `codex/013-draft-edit-recovery`. No commit/push/PR unless requested. Authorized commit subject: `fix: preserve interrupted draft edits`, user author, no attribution footer.

## Contract

1. Preparing an edit never deletes a previous failed/cancelled edit.
2. Each new attempt has its own exclusive, unpredictable filename and private permissions. Concurrent attempts cannot overwrite or clean up each other's files.
3. Every failure after file creation identifies the retained absolute path. Never print draft text, recipients, or credentials into diagnostic logs.
4. Cleanup occurs only for that attempt after a confirmed successful update, or after an explicit user discard choice.
5. Recovery checks account/draft identity and compares the recorded base with the latest fetched draft. If they differ, preserve both and ask the user to choose/copy text; never silently apply retained text over the newer draft.
6. This comparison is a recovery aid, **not an atomic stale-write guarantee**. This plan does not claim to prevent another client updating the draft between fetch and `UpdateDraft`. Atomic CAS would require a separate protocol/store contract.
7. HTML drafts remain unsupported by the markdown editor; recovery must not convert them silently.

## Steps and verification

### 1. Capture the destructive retry regression

First add a `draft_edit_recovery` regression in the existing `crates/tui/src/compose_flow.rs` tests, calling the actual `prepare_draft_edit` implementation. Use a temporary private scratch directory and a synthetic draft. Seed retained edited bytes at the filename used today, prepare the same draft again with fake account IPC, and assert the original bytes still exist. Model fake daemon requests after existing TUI compose tests; no real editor, OS keychain, or mail provider. Shared-compose and CLI tests follow the extraction in steps 2 and 3.

**Verify:** before the fix, `scripts/cargo-test -p mxr-tui --tests draft_edit_recovery` must run this test and fail for the retained-file assertion. Label this expected red run in the execution log; do not accept unrelated compile errors or zero tests as reproduction.

### 2. Share per-attempt scratch ownership

Add `draft_edit.rs` with a small typed attempt record: draft/account identity, original `updated_at` and the original rendered text (or another already available stable equality representation), attempt path, and minimal sidecar metadata needed after restart. Store content only inside the private scratch area; activity/context logs contain no draft bodies. Prefer the existing draft codec and YAML serialization. Use UUID attempt filenames and exclusive private writes; do not rename over an existing candidate.

Keep the sidecar and edit file recognizable after interrupted creation. Incomplete/corrupt sidecars must leave the content intact and return a recoverable error. Legacy `mxr-draft-edit-<id>.md` files have no provenance; retain them and show their path rather than auto-applying them. Read private regular files with the ownership/symlink protections already established by `private_tmp`; if those APIs cannot protect reads, add the smallest matching helper there.

**Verify:** `scripts/cargo-test -p mxr-compose --tests` passes with tests for original preservation, separate attempts, incomplete metadata, symlink rejection, permissions, and cleanup limited to the owned attempt.

### 3. Route CLI and TUI through the same recovery decision

Replace both unconditional removals with shared preparation. When retained candidates exist, surface an explicit resume/start-fresh/discard choice using the CLI's existing prompt library and the TUI stored-drafts interaction. Limit `app/mod.rs`, `app/compose_helpers.rs`, `runner.rs`, and `ui/drafts_modal.rs` changes to passing this recovery decision and selected attempt. Noninteractive execution fails with the retained paths and actionable next step; it must not guess a candidate or delete one. Starting fresh leaves other candidates untouched.

On resume, fetch the latest stored draft through existing IPC, verify identity, and compare with the recorded base. On mismatch, keep files and report the conflict without calling `UpdateDraft`. For a matching base, reopen the retained content and use `draft_codec` for conversion. Do not introduce a second draft ID. Wrap parse/validation/editor/update failures with the retained path in both clients. Preserve existing safety checks.

**Verify:** `scripts/cargo-test -p mxr --test cli_journey draft_edit_recovery` and `scripts/cargo-test -p mxr-tui --tests draft_edit_recovery` each run new tests and pass. Test sequences must include failed save → retry → recovered content; parse failure → corrected retry; editor failure; cancelled edit; distinct concurrent scratch attempts; and changed stored base → no update request. A zero-test filtered run is failure of the verification gate.

### 4. Finish the lifecycle and user wording

Test that an acknowledged update removes only its own scratch/sidecar; IPC loss keeps both even when server success is uncertain. The next recovery attempt must re-fetch stored state and explain any difference. Clarify in linked-draft documentation what scratch recovery does and that `drafts recover` still refers to orphaned sends. Do not claim general concurrent-write safety.

**Verify:** `scripts/cargo-test -p mxr-compose --tests`, `scripts/cargo-test -p mxr-tui --tests draft_edit`, `scripts/cargo-test -p mxr --test cli_journey`, `cargo build -p mxr`, and `git diff --check` all pass. Use the isolated fake daemon fixture; unset inherited daemon-address overrides and disable keychain access.

## Done criteria

- [ ] Preparing CLI/TUI edits never removes retained text.
- [ ] Regression sequences verify preserved bytes after failure, retry, and separate concurrent attempts.
- [ ] Recovery cannot cross account/draft identity and refuses a changed recorded base without mutating the daemon.
- [ ] All failure messages identify recovery paths; logs/activity do not contain draft bodies.
- [ ] Confirmed success cleans up only the current attempt; ambiguous failure retains it.
- [ ] HTML and linked-draft identity behavior remain covered by existing tests.
- [ ] Focused tests, final build, diff checks, and scoped-file review pass; index/reviewer records completion.

## STOP conditions and maintenance

Stop if existing recovery metadata appears after prerequisite work, shared scratch ownership needs a new provider/store design, a safe resume requires an atomic revision promise beyond this contract, existing private-file protections cannot be preserved, or a gate fails twice. Never resolve a conflict by overwriting a stored/provider draft or deleting retained text. A future CAS plan should share the daemon's update contract across all clients; do not imply this scratch fix already provides it.
