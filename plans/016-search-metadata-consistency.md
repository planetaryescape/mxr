# Plan 016: Preserve reply-later state in every search refresh

> Executor: implement only the paths below and run each future gate. This plan records static code findings; its regression tests have not been written or run. The coordinating reviewer owns plans/README.md. Do not edit that index or push without a separate instruction.

## Status

- Priority: P1
- Effort: M
- Risk: LOW for normal refreshes, MED for post-send failure handling
- Depends on: plans/011-safe-verification-and-ci.md
- Category: bug
- Planned at: fetched origin/main 9160b1a12ef9dc5d3fb5513b45c68fe57183074f, 2026-09-04
- Source inspected: /tmp/mxr-polish-audit-9160b1a1
- Plan destination: /Users/bhekanik/code/planetaryescape/mxr/plans/016-search-metadata-consistency.md
- Execute after any in-flight 014/015 work is integrated. This shares mutations.rs with 014 and sync engine/tests with 015. Plan 017 must execute after this plan.
- Delivery unit: implement 016's bounded path corrections first, then 017's durable publication repair. Final consistency acceptance belongs to the pair. Do not ship the combined guarantee before the overlap/failure regressions pass under 017.

## Why this matters

The reply queue reads SQLite while is:reply-later reads Tantivy. Sending a reply currently clears only SQLite. Reading a body that needs repair can overwrite Tantivy's reply-later flag with false. Both make existing ways of finding pending replies disagree. Correct the write paths rather than changing query semantics or adding a new queue.

## Current state

crates/daemon/src/handler/mutations.rs:2941-2944 clears the replied-to parent directly:

~~~rust
state
    .store
    .clear_reply_later(&parent.id, chrono::Utc::now())
    .await
~~~

crates/daemon/src/handler/reply_later.rs:30-43 already provides the shared behavior:

~~~rust
if flag {
    state.store.set_reply_later(message_id, now).await
} else {
    state.store.clear_reply_later(message_id, now).await
}
// Error mapping omitted in this excerpt.
refresh_reply_later_search_marker(state, message_id, flag).await
~~~

crates/sync/src/engine.rs:277-284 rebuilds a search entry during body repair with a hardcoded flag:

~~~rust
SearchIndexEntry {
    envelope: envelope.clone(),
    body: Some(normalized.clone()),
    reply_later: false,
}
~~~

The same hardcoded false appears in persist_synced_message at engine.rs:250-258. The main batch sync path correctly reads reply_later_message_ids at :476-483. Use that as the preservation pattern. These methods are reachable through mailbox.rs:840, :861 and :941, including provider hydration and local normalization.

Existing tests:

- handler/tests/body_and_invites.rs:848-903 asserts that explicit SetReplyLater updates both search and the reply queue.
- handler/tests/platform_and_export.rs:1273-1331 creates the parent's flag directly in the store and only asserts it was cleared in the store after send. It never seeds/asserts the index, so it misses this mismatch.
- crates/sync/src/lib.rs contains in-memory engine tests and fake providers. Keep tests at the public engine/daemon behavior boundary.

The daemon owns mutations, clients read the same IPC state, and SQLite is authoritative. Post-send cleanup cannot turn an accepted send into a resendable failure. Search remains optional to delivery success but its failure must be visible through existing diagnostics. Inspection found no durable metadata repair marker in Store::set_reply_later/clear_reply_later and no revision on SearchIndexEntry. A warning cannot repair failed publication. Plan 017 owns durable retry for these reply-later changes as well as threading changes.

## Lessons applied from Obsidian

"Zero Change Can Be Success" means setting an already-correct flag is a successful idempotent update, not a reason to skip needed index repair. "Nonblocking Work Needs a Visible Home" means a failed post-send search update needs an existing diagnostic/event outcome. "Provider Lanes Need Account Scope" cautions against a global provider lock for local-only metadata. These read-only notes inform the fix; they are not evidence that current code already honors it.

## Scope

- crates/daemon/src/handler/mutations.rs
- crates/daemon/src/handler/reply_later.rs
- crates/daemon/src/handler/mailbox.rs only if an existing caller must pass the canonical ID after hydration
- crates/daemon/src/handler/tests/body_and_invites.rs
- crates/daemon/src/handler/tests/platform_and_export.rs
- crates/sync/src/engine.rs
- crates/sync/src/lib.rs
- crates/sync/src/test_support.rs

Out of scope: new reply queues, query grammar, a general index outbox, thread canonicalization, search performance work, UI redesign, database migrations, or new protocol fields. Plan 017 owns threading/index publication. Do not change provider flags for local-only reply-later state.

## Git and commands

Use an isolated worktree on codex/016-search-metadata-consistency. Fetch first:

~~~sh
git fetch origin main
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/daemon/src/handler/mutations.rs crates/daemon/src/handler/reply_later.rs crates/daemon/src/handler/mailbox.rs crates/daemon/src/handler/tests/body_and_invites.rs crates/daemon/src/handler/tests/platform_and_export.rs crates/sync/src/engine.rs crates/sync/src/lib.rs crates/sync/src/test_support.rs
~~~

Expected: no unexplained changes. Compare Current state to the implementation ref after integrating 014/015 if completed. Stop if the post-send lifecycle or sync refresh contract has changed in an unexplained way. Never switch/reset/clean the primary checkout. Use type: description commits, user author only.

Every following command is a FUTURE gate. Complete 011 first and use its repaired scripts/cargo-test with an isolated MXR_* test profile. Tests must use in-memory state or owned temporary files and fake providers. No live daemon, HOME/CODEX_HOME changes, or global process cleanup.

## Steps

### 1. Add tests that seed both representations

In the existing daemon tests, flag the parent through Request::SetReplyLater rather than directly through Store. Assert it appears in both ListReplyQueue and is:reply-later before sending. After a successful reply, assert both omit it and the fake sender records exactly one send. Add equivalent tests for SendDraft and SendStoredDraft using existing fixture builders. Name tests with plan016_.

In sync tests, persist/index a message, mark it reply-later, then call repair_body and persist_synced_message. Assert each operation preserves the flag in search and SQLite, including the path where the store remaps an incoming provider ID to its existing local ID.

Define two additional deterministic acceptance cases now: pause body hydration after its metadata read and start SetReplyLater on the same message; and accept a reply, fail search publication, reopen the store with working search, and run repair. For the overlap case, test the mechanism actually chosen in 017. With a publication guard, assert the flag writer is waiting, release hydration, then await both writers. With revision rejection, allow the flag to complete first and prove hydration cannot publish the older revision. Do not require a guarded writer to complete while the test holds its lock. Both cases must converge to the persisted flag, with one send in the second case. Use barriers/fault injection, not sleeps. Implement and run these paired acceptance tests with 017; do not commit skipped tests or assert the current wrong result as intended behavior. Record the interleaving and pass the fixture design to the 017 executor.

Future gates: scripts/cargo-test -p mxr --lib plan016_ -- --nocapture and scripts/cargo-test -p mxr-sync --lib plan016_ -- --nocapture. These are deliberate RED gates: expect flag/search assertions to fail on current code, not compilation or fixture setup. Capture the failing assertions.

### 2. Reuse the shared post-send metadata updater

Change clear_reply_later_for_reply_parent to call reply_later::set_reply_later_at with false and the parent ID. Preserve existing parent resolution and account scoping. Keep failure nonfatal to the accepted send. Emit the existing warning/diagnostic event with only parent ID and error category, and retain whatever accepted-send protection plan 014 added. Never reset the draft or issue another provider call because Tantivy failed.

If set_reply_later_at fails after the SQLite write, a repeated explicit flag operation must retry search refresh even if SQLite already has the desired value. Do not add an early return based only on flag equality. This manual retry is not sufficient for final acceptance: 017 must persist repair debt with the flag write and retry automatically at startup/maintenance before the pair is considered complete.

Future gate: scripts/cargo-test -p mxr --lib plan016_ -- --nocapture. Expected: both send-path tests pass, successful send count remains one, and a test with a closed/failing search worker proves no resend or rollback to editable state.

### 3. Preserve persisted metadata when repairing a message

In persist_synced_message, obtain the reply-later value using the final stored message ID after any ID remapping. In repair_body, read the existing flag for envelope.id before constructing SearchIndexEntry. Propagate lookup failures rather than replacing unknown state with false. Retain body normalization, label association and search commit behavior. A genuinely new unflagged message remains false.

Use the existing store APIs and current SearchIndexEntry construction. Do not introduce duplicate flag storage. Read the canonical flag after body/store work so repair does not reuse the stale flag from the original provider envelope.

Future gate: scripts/cargo-test -p mxr-sync --lib plan016_ -- --nocapture. Expected: at least four sync tests pass for repaired flagged body, hydrated flagged body, remapped message ID, and new unflagged message.

### 4. Verify existing client contracts and failure behavior

Add a daemon request sequence for flag, body read/repair, queue list, search, unflag, repeated unflag. Use one test-owned state and fake provider. Assert queue/search agreement after each completed successful operation. Cover missing parent and missing stored body without changing send success.

No client changes or migration are expected in 016. CLI, TUI, web and MCP already consume these daemon requests. The deterministic overlap case from step 1 must be run as part of 017's final gate, because a canonical lookup alone cannot reject an older publication after a newer flag change. 017 owns the account-scoped publication ordering and durable repair needed to close that case. No global lock is permitted. SQLite and Tantivy are not one transaction.

Future gates:

~~~sh
scripts/cargo-test -p mxr --lib plan016_ -- --nocapture
scripts/cargo-test -p mxr-sync --tests
scripts/cargo-test -p mxr --lib reply_later -- --nocapture
scripts/cargo-test -p mxr --lib dispatch_send_stored_draft_replay_returns_original_receipt_without_resending -- --nocapture
cargo build -p mxr
git diff --check
~~~

Expected: all exit 0, at least four daemon plan016_ tests and four sync plan016_ tests pass, existing explicit-flag and send-replay tests still pass, and diff contains only Scope paths. No SQLx regeneration is needed because this plan uses existing queries without changing schema.

## Regression matrix

| Sequence | Expected result |
|---|---|
| Mark reply-later, SendDraft reply | Queue and search clear; one provider send |
| Mark reply-later, SendStoredDraft reply | Same result through stored send path |
| Flagged message, repair_body | Body changes; flag remains true in store/search |
| Flagged message, provider hydration | Canonical stored ID and flag preserved |
| New message without flag | Index false, no synthetic queue entry |
| Repeat clear when store is already clear | Index refresh still runs and succeeds |
| Accepted reply, search update fails | Send remains accepted; failure visible, no resend |
| Hydration publishes after a concurrent completed flag update | Paired acceptance in 017 must reject/repair stale metadata |
| Accepted reply, failed publication, restart with working search | Paired acceptance in 017 repairs automatically without resending |
| Missing parent or body | Existing defined fallback; no panic or unrelated flag change |

## Done criteria and stop conditions

- All future gates pass with named nonzero regression counts.
- No hardcoded false remains in the two repair/hydration SearchIndexEntry paths.
- Post-send clearing uses the shared metadata/search updater and never changes delivery acceptance on index failure.
- No new protocol, migration, provider flag, or UI feature was added.
- Report actual test output and the restricted diff to the reviewer; they update the index.
- Report 016's successful-operation tests separately from the two required paired tests owned by 017. Do not claim durable/concurrent consistency from this patch alone.

Stop on unexpected source drift, evidence that the store flag belongs to a different canonical ID, an unexplained concurrent overwrite, or any proposal to make accepted send retryable because indexing failed. Stop after the same gate fails twice. Plan 017 must incorporate this plan's flag preservation in its new final index publication path.
