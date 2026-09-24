# Plan 015: Reconcile missing Gmail messages after a complete recovery scan

> Executor: follow the gates in order. All verification below is FUTURE work; no implementation or regression tests ran while this plan was written. The coordinating reviewer owns plans/README.md. Do not edit the index or push without a separate instruction.

## Status

- Priority: P1
- Effort: L
- Risk: HIGH, deleting local mail on incomplete evidence
- Depends on: plans/011-safe-verification-and-ci.md
- Category: bug
- Planned at: fetched origin/main 9160b1a12ef9dc5d3fb5513b45c68fe57183074f, 2026-09-04
- Source inspected: /tmp/mxr-polish-audit-9160b1a1
- Plan destination: /Users/bhekanik/code/planetaryescape/mxr/plans/015-authoritative-gmail-resync.md
- File conflict: plans 016 and 017 also change crates/sync/src/engine.rs and sync tests. Finish and integrate 015 before either starts. Plans 014 and 023 share store migration allocation and daemon startup files; serialize those edits too.

## Why this matters

When Gmail no longer has the history behind a saved cursor, mxr restarts full sync. That fetches messages still present but never reconciles messages deleted during the missing history window. Those messages can remain in local lists and search indefinitely. Recovery must remove stale provider-backed records only after a complete, account-scoped scan proves their absence.

This is a static finding, not a live Gmail reproduction. Gmail documents finite history retention and a full-sync requirement after HTTP 404: https://developers.google.com/workspace/gmail/api/guides/sync. Re-read the official messages.list and history.list contracts before implementing reconciliation, especially spam/trash inclusion and concurrent mailbox changes.

## Current state

crates/sync/src/engine.rs:344-355 clears the cursor when the provider reports expiry:

~~~rust
Err(MxrError::SyncCursorExpired { reason }) if !recovered_expired_cursor => {
    // Logging omitted here.
    self.store
        .set_sync_cursor(account_id, &SyncCursor::empty())
        .await
        .map_err(|e| MxrError::Store(e.to_string()))?;
    recovered_expired_cursor = true;
    continue;
}
~~~

crates/provider-gmail/src/provider.rs:390-398 and :449-455 emit initial/backfill batches with this shape:

~~~rust
Ok(SyncBatch {
    upserted: synced,
    deleted_provider_ids: vec![],
    label_changes: vec![],
    next_cursor,
    has_more,
    threads_changed: vec![],
    remaining_estimate: None,
})
~~~

- engine.rs:489-504 deletes only explicit deleted_provider_ids. There is no completed-inventory sweep in that path.
- crates/provider-gmail/src/client.rs:175-181 builds messages.list with maxResults, optional q, and pageToken. It does not request includeSpamTrash. Therefore its current list cannot safely prove absence across all Gmail messages.
- crates/core/src/types.rs:2120 defines SyncBatch with upserts, explicit deletions, cursor and has_more. No field currently states that a completed batch belongs to an authoritative mailbox inventory. Do not infer that from generic has_more=false.
- crates/store/src/sync_cursor.rs stores opaque cursor bytes. Provider cursor interpretation belongs in the Gmail adapter.
- crates/daemon/src/loops.rs:591-613 already finalizes deferred analytics based on completion and recorded debt, including an empty final page. Reuse that lifecycle discipline, not the exact analytics implementation.
- Existing test: crates/sync/src/lib.rs:2161, expired_sync_cursor_resets_to_initial_and_recovers. It seeds an expired cursor but no preexisting message that disappeared remotely.

Architecture constraints: store and search depend only on core; sync depends on core/store/search. Gmail-specific cursor and list semantics stay in provider-gmail. SQLite is canonical, search is rebuildable, and successful sync must publish matching lexical results before reporting completion. Preserve local-only drafts and sent mail.

## Lessons applied from Obsidian

"The Last Page Can Be Empty" requires finalization on the scan completion transition, even when the final page has no messages. "Zero Change Can Be Success" requires a completed empty mailbox or no-change scan to finish normally. "Provider Lanes Need Account Scope" requires serialization of the same account's sync and provider mutations, without holding other accounts hostage. "Retrying a Permanent Error Is Data Loss" means a permanently invalid page token restarts a safe scan rather than retrying the identical token indefinitely. These principles were paraphrased from read-only notes; the code above is the implementation evidence.

## Scope

Only modify these paths for authoritative recovery:

- crates/core/src/types.rs, crates/core/src/provider.rs, crates/core/src/error.rs
- crates/provider-gmail/src/provider.rs, crates/provider-gmail/src/client.rs, crates/provider-gmail/src/cursor.rs, crates/provider-gmail/src/types.rs
- crates/sync/src/engine.rs, crates/sync/src/lib.rs, crates/sync/src/test_support.rs
- crates/store/src/sync_cursor.rs, crates/store/src/sync_upsert.rs, crates/store/src/message.rs, crates/store/src/lib.rs
- One new store module crates/store/src/sync_reconciliation.rs and one new migration in crates/store/migrations/ with the next free number
- .sqlx/*.json only when generated for changed checked queries
- crates/daemon/src/loops.rs, crates/daemon/src/server.rs, crates/daemon/src/handler/diagnostics/mod.rs, crates/daemon/src/handler/tests/routing_and_search.rs
- crates/provider-fake/src/lib.rs, crates/provider-imap/src/lib.rs, crates/provider-outlook/src/lib.rs only to default a new adapter-neutral batch field and update fixtures
- docs/blueprint/04-sync.md

If adding a SyncBatch field requires fixing additional struct literals, those mechanical default initializers in existing crates/*/src/**/*.rs tests/providers are allowed. List each in the PR; no behavior changes outside the paths above. Avoid changing SyncBatch if the existing provider trait can carry explicit inventory evidence with a smaller compatible addition.

Out of scope: IMAP reconciliation redesign, archive retention policy, deleting provider mail, automatically purging local drafts, a user-facing resync control panel, new semantic retrieval features, or rebuilding every account.

## Git and command setup

Use an isolated worktree on codex/015-authoritative-gmail-resync. Fetch first and inspect drift:

~~~sh
git fetch origin main
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/core crates/provider-gmail crates/sync crates/store crates/daemon/src/loops.rs crates/daemon/src/server.rs crates/daemon/src/handler/diagnostics/mod.rs crates/daemon/src/handler/tests/routing_and_search.rs crates/provider-fake crates/provider-imap crates/provider-outlook .sqlx docs/blueprint/04-sync.md
~~~

Expected: no unexplained source differences. Compare the excerpts with the fetched ref. Incorporate recorded predecessor changes; stop if they invalidate this recovery design. Never reset or clean the primary checkout. Commit logical units as type: description with the user as sole author.

Every command below is a FUTURE gate. Use the repaired scripts/cargo-test and isolated environment from 011, in-memory/file-backed test stores, and fake/Wiremock Gmail responses. No real account credentials, live mail, HOME/CODEX_HOME reassignment, or global process cleanup.

## Steps

### 1. Reproduce retention-gap behavior and incomplete inventories

Extend the existing expired-cursor test with two seeded provider-backed messages and a recovery scan that returns only one. Give every new test the plan015_ prefix. Add an explicit empty-final-page sequence: one populated page with has_more=true, then an empty page with completion. Add an entirely empty successful mailbox, an interrupted scan, and two accounts sharing the same provider ID string.

Future gate: scripts/cargo-test -p mxr-sync --lib plan015_ -- --nocapture. This is a deliberate RED gate: missing-message reconciliation fails as asserted, while setup and account fixtures succeed. Capture actual failing assertions. Do not invent a live-provider failure claim.

### 2. Define and persist authoritative scan evidence

Add an adapter-neutral inventory contract that distinguishes ordinary delta completion, incomplete inventory pages, and authoritative completed inventory. The Gmail adapter owns its scan token/start history cursor and spam/trash semantics. Ordinary has_more=false alone must never authorize deletion.

In sync_reconciliation.rs, persist a recovery generation keyed by account. At scan start snapshot only existing provider-backed message candidates in that account, excluding local-only synthetic sent mail and drafts. Persist candidate membership, IDs seen in the scan, cursor/progress, and completion state. Do not use wall-clock timestamps alone as the mutation/version boundary. Newly inserted messages after the snapshot are never deletion candidates. Use SQL transactions and account-scoped keys; SQLite rows, not an in-memory HashSet alone, must survive restart.

Keep page progress and seen IDs consistent with committed message upserts. If a page fails partway, replay must be idempotent. Starting a new generation abandons the prior incomplete generation without sweeping it. A completed generation can be finalized again without changing the result. Allocate a fresh migration; preserve existing opaque cursor rows and schema compatibility.

Future gate: scripts/cargo-test -p mxr-store --lib plan015_ -- --nocapture. Expected: at least six tests pass for account isolation, restart, interrupted page rollback, candidate snapshot, empty completion, and repeated finalization.

### 3. Make the Gmail inventory complete and safe under concurrent changes

Use the official list API with includeSpamTrash=true for this authoritative inventory, or another documented complete inventory request. Test the actual query parameters with Wiremock. Do not apply deletion to a filtered recent-mail listing or Gmail's approximate resultSizeEstimate.

Capture a valid history baseline through the adapter. Walk pages to explicit completion, including empty final pages. Catch up intervening Gmail changes before finalization. Protect client-originated mutations with the existing account lane. Because other Gmail clients can change mail while pagination runs, revalidate still-unseen candidate IDs with the provider immediately before removing them. Only a definitive per-message absence response or an explicit deletion event counts; authorization failures, quota responses, transport timeouts, skipped parse failures, and an empty search result do not. If this validation cannot be completed, keep the candidates and pending generation and report failure/incomplete state.

Bound page fetch, candidate revalidation, and memory use. Page through candidate IDs instead of materializing complete bodies. Never hold the account lane during optional semantic/contact work. If a page token expires, restart a new generation safely. If history expires again during the scan, discard its authority and restart or report failure; never sweep an uncertain inventory.

Future gate: scripts/cargo-test -p mxr-provider-gmail --lib plan015_ -- --nocapture. Expected: at least six tests pass, including includeSpamTrash, empty final page, invalid token, definitive missing ID, temporary lookup failure, and concurrent mail appearing between pages.

### 4. Finalize local deletion and index publication

In SyncEngine, finalize only a completed authoritative generation after candidate revalidation. Use the same computed candidate set for an internal preview/dry-run result and applied deletion. It is an internal store/sync contract, not a new product screen. Delete only eligible rows for that account and reuse existing cleanup behavior for body, labels, threads, and dependent rows. Retain synthetic/local-only sent mail, local drafts, and records created or changed outside the validated generation.

Publish matching search removals before reporting successful completion. Persist enough pending reconciliation state to replay lexical removal if SQLite deletion succeeds but index publication fails. Do not rely only on startup document-count equality. Finish counts/thread invalidation through the existing sync outcome. An empty last page can still remove old messages and update completion status.

Future gate: scripts/cargo-test -p mxr-sync --lib plan015_ -- --nocapture. Expected: all step 1 cases pass; injected index failure leaves repairable pending state and the next pass converges without deleting unrelated records.

### 5. Check status parity and end-to-end recovery

Use existing daemon sync progress/failure/success and threads-changed notifications. A completed recovery with zero upserts is successful. CLI, TUI, web and MCP should observe the same corrected lists/search through their existing daemon requests; no new lifecycle enum or command is expected. If the implementation introduces a new user-visible recovery state, STOP and amend this plan to include protocol and all four clients before landing it.

Add a daemon plan015_ regression that requests search/list before and after a recovery with a missing old message, checks last-success/in-progress fields rather than a positive upsert count, and verifies another account remains unchanged.

Future gate: scripts/cargo-test -p mxr --lib plan015_ -- --nocapture. Expected: at least three daemon integration tests pass, including zero-upsert completion and index repair retry. Then run the gates below.

~~~sh
resync_sqlx_dir="$(mktemp -d /tmp/mxr-plan015-sqlx.XXXXXX)"
cargo run -p mxr-store --example create_db -- "$resync_sqlx_dir/check.db"
DATABASE_URL="sqlite:$resync_sqlx_dir/check.db" cargo sqlx prepare --workspace
DATABASE_URL="sqlite:$resync_sqlx_dir/check.db" cargo sqlx prepare --check --workspace
scripts/cargo-test -p mxr-sync -p mxr-provider-gmail -p mxr-store --tests
cargo build -p mxr
git diff --check
~~~

Expected: exit 0 throughout, changed-query SQLx metadata only, all named regressions present and passing. SQLx commands follow the existing .github/workflows/ci.yml:361-365 create_db/prepare workflow against an owned temporary DB. Update docs/blueprint/04-sync.md with the implemented generation/completion guarantee and validation results.

## Regression matrix

| Case | Required result |
|---|---|
| Expired history and one deleted old message | Deleted locally/search only after complete authoritative recovery |
| Populated page then empty final page | Completion and eligible cleanup occur |
| Entire account genuinely empty | Successful complete scan; eligible stale records removed |
| Error, crash, cancellation or invalid token halfway | No incomplete-generation sweep; restart can resume/restart safely |
| Spam/trash present | Included in inventory; never falsely removed as absent |
| Same provider ID in another account | Other account unchanged |
| Local draft or synthetic sent record | Preserved |
| Concurrent sync/foreground mutation or new local sent row | Account serialization and candidate version checks prevent false deletion |
| Remote client changes between list pages | Candidate revalidation prevents page-boundary omissions becoming deletions |
| Definitive 404 versus 401/429/timeout | Only valid missing-message evidence permits local removal |
| SQLite commit followed by search failure | Pending cleanup survives; next run converges |
| Completed no-change scan or repeated finalization | Success without fabricated upsert/deletion counts |

## Done criteria and stop conditions

- Every future gate passes with nonzero regression counts.
- Local deletion requires explicit completed inventory authority and validated candidate absence.
- Preview and mutation use the identical account/version-filtered candidate set.
- Empty final page, interruption, concurrent changes, local-only records, and two-account cases pass.
- No provider logic leaks into the store/daemon, and no new UI feature was added.
- Diff is within Scope; report actual results to the coordinating reviewer for the index.

STOP if the Gmail inventory is filtered or nonauthoritative, missing-message lookup cannot distinguish absence from authorization failure, a migration needs real mailbox data to run, or the implementation proposes deleting before final completion. Also stop after the same gate fails twice or unexpected source drift invalidates this plan. Do not solve ambiguity with an unconditional account wipe.

Maintenance: review completion and candidate-validation conditions first. Future pagination changes must keep the empty-final-page and interrupted-generation tests. Coordinate engine changes with plans 016 and 017 before merging.
