# Plan 017: Thread the complete archive and publish canonical IDs to search

> Executor: follow this handoff in order. Commands are FUTURE gates, not results from plan writing. The coordinating reviewer owns plans/README.md. Do not edit that index or push without a separate instruction.

## Status

- Priority: P1
- Effort: L
- Risk: MED, thread identity and index consistency
- Depends on: plans/011-safe-verification-and-ci.md and plans/016-search-metadata-consistency.md
- Category: bug and perf
- Planned at: fetched origin/main 9160b1a12ef9dc5d3fb5513b45c68fe57183074f, 2026-09-04
- Source inspected: /tmp/mxr-polish-audit-9160b1a1
- Plan destination: /Users/bhekanik/code/planetaryescape/mxr/plans/017-complete-threading-and-index-publication.md
- File conflict: plan 015 changes the same sync engine, outcome handling, tests, and store migration sequence. Integrate 015 first if it is being executed. Never run 015 and 017 edits concurrently. Preserve 016's flag reads in every new index path.

## Why this matters

IMAP does not provide native thread IDs. mxr's fallback threads only the newest 10,000 messages, leaving older conversations outside the calculation. It also commits lexical documents before changing SQLite thread IDs, so search can retain the old thread IDs. Complete threading and final search publication belong to the same sync contract.

These are static findings. No 10,001-message runtime reproduction was run during the audit. This plan includes that test and a smaller index-divergence reproduction before changes.

## Current state

crates/sync/src/engine.rs:709-718 loads a capped mailbox listing:

~~~rust
let envelopes = self
    .store
    .list_envelopes_by_account(account_id, 10_000, 0)
    .await
    .map_err(|e| MxrError::Store(e.to_string()))?;
~~~

The query at crates/store/src/message.rs:354-388 orders newest-first with LIMIT/OFFSET. The omitted rows never enter the mail-threading input. IMAP sets native_threading=false at crates/provider-imap/src/lib.rs:1205.

engine.rs:573-575 commits lexical updates before its rethread call at :592-594. Inside rethread_account it changes only SQLite at :769-772:

~~~rust
self.store
    .update_message_thread_id(&message_id, &canonical_thread_id)
    .await
    .map_err(|e| MxrError::Store(e.to_string()))?;
~~~

crates/search/src/index.rs:233 and :312 store envelope.thread_id in Tantivy. The owed search filter in crates/daemon/src/handler/diagnostics/search_execute.rs:165-192 compares that indexed thread ID with SQLite-derived owed thread IDs. The stale ID can therefore affect filtering, not only display.

Existing exemplars:

- crates/sync/src/lib.rs:901, sync_rethreads_messages_when_provider_lacks_native_thread_ids
- crates/sync/src/lib.rs:1019, sync_rethreads_messages_without_message_id_headers
- engine.rs:720-761 maps local IDs to the existing mail-threading library and chooses the canonical root's stored thread ID
- engine.rs:476-484 preserves SQLite reply-later metadata while building lexical entries
- crates/store/src/thread.rs:30-139 uses bounded/batched thread hydration
- crates/daemon/src/loops.rs:591-613 keeps completion separate from whether the last page carried messages

Keep the existing mail-threading crate and canonical-ID policy. Store/search remain below sync; daemon/provider dependencies must not enter them. SQLite is canonical, lexical search is rebuildable, and semantic work cannot block mail readiness.

## Lessons applied from Obsidian

"The Last Page Can Be Empty" means deferred threading must settle on completed backfill, including an empty final page. "Zero Change Can Be Success" means an idle sync should finish without running an unnecessary full-archive pass, while a zero-payload completion may still owe work. "Provider Lanes Need Account Scope" means one account's threading/provider work must not become a global lock. These are paraphrased design lessons, not copied personal examples or proof of current behavior.

## Scope

- crates/sync/src/engine.rs, crates/sync/src/lib.rs, crates/sync/src/test_support.rs
- crates/store/src/message.rs, crates/store/src/thread.rs, crates/store/src/message_flags.rs, crates/store/src/sync_upsert.rs, crates/store/src/lib.rs
- One new module crates/store/src/search_metadata_repair.rs and one new migration in crates/store/migrations/ using the next free number, if persistent repair state cannot reuse a predecessor's exact mechanism. This module may track both account threading debt and changed-message publication debt.
- .sqlx/*.json only for changed checked-query metadata
- crates/daemon/src/handler/diagnostics/search_execute.rs and crates/daemon/src/handler/tests/routing_and_search.rs, tests only unless search hydration must consume canonical IDs
- crates/daemon/src/loops.rs and crates/daemon/src/server.rs only for existing completion/recovery integration
- crates/daemon/src/handler/reply_later.rs, crates/daemon/src/handler/mutations.rs, crates/daemon/src/reindex.rs, crates/daemon/src/handler/account_config.rs only to use the shared metadata publication/repair contract
- crates/daemon/src/handler/tests/body_and_invites.rs and crates/daemon/src/handler/tests/platform_and_export.rs for the paired plan016_ overlap/restart acceptance tests
- benches/sync_overlap.rs only for a compact-header/threading benchmark in the existing target
- docs/blueprint/04-sync.md

Out of scope: new thread algorithms, changing subject fallback, broad search redesign, a general outbound/background job queue, an IMAP provider rewrite, graph visualizations, relationship/semantic features, UI redesign, or loading the entire archive's bodies into memory. The changed-message repair marker is limited to lexical metadata convergence. Use the existing library and normal store batching instead of a custom replacement.

## Git and command setup

Use an isolated worktree on codex/017-complete-threading-and-index-publication after 016 is integrated. Fetch and inspect:

~~~sh
git fetch origin main
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/sync crates/store crates/daemon/src/handler/diagnostics/search_execute.rs crates/daemon/src/handler/tests/routing_and_search.rs crates/daemon/src/loops.rs crates/daemon/src/server.rs crates/daemon/src/handler/reply_later.rs crates/daemon/src/handler/mutations.rs crates/daemon/src/reindex.rs crates/daemon/src/handler/account_config.rs crates/daemon/src/handler/tests/body_and_invites.rs crates/daemon/src/handler/tests/platform_and_export.rs benches/sync_overlap.rs .sqlx docs/blueprint/04-sync.md
~~~

Expected: differences from completed 015/016 are understood and incorporated; no unexplained contract drift. Do not overwrite predecessor work or reset the primary checkout. Use type: description commits, user author only.

All following commands are FUTURE gates. Complete 011 before scripts/cargo-test. Use its isolated MXR_* profile and test-owned temporary stores. Never use live mail, change HOME/CODEX_HOME, or kill global processes.

## Steps

### 1. Reproduce the cutoff and stale index separately

Add plan017_ tests modeled on the two existing threading tests. Seed 10,001 or more messages with a parent/reply conversation spanning the newest-10,000 boundary. Use batched fixture inserts and compact bodies so the test is practical. Test a wholly old conversation too.

Separately use only two related messages with distinct initial thread IDs. After sync, assert the stored IDs agree and each lexical result carries that same ID. Add an owed-reply query test against the daemon's real filter so index metadata is tested through its consumer. Do not merely assert the helper returned a list.

Implement the two paired plan016_ cases specified in plan 016: deterministic hydration-versus-SetReplyLater overlap, and accepted reply followed by failed search publication and restart. The former must prevent an older completed publication from becoming final state; the latter must repair automatically with no second send. Keep barriers in test-only hooks around the real metadata read/publication boundaries. They are mandatory acceptance tests, not optional runtime investigation.

Future gate: scripts/cargo-test -p mxr-sync --lib plan017_ -- --nocapture. This is a deliberate RED gate: expect cutoff/index assertions to fail on the current behavior. Fixture setup and unrelated tests must not fail. Capture the actual mismatches.

### 2. Read complete, compact threading metadata

Add a store query/DTO for the fields used by mail-threading: local ID, current thread ID, Message-ID, In-Reply-To, References, date and subject. Fetch every relevant row for one account using stable keyset pagination. Do not call the rich envelope listing and do not select bodies, labels, recipient arrays, or attachment data. Preserve missing-header and subject-fallback behavior.

Feed the complete compact input to the existing mail-threading library. It requires a graph-wide input, so total header memory can still be O(number of messages); state that honestly. Bound database pages and later body/index hydration. Do not replace the 10,000 cap with a larger arbitrary cap.

Future gate: scripts/cargo-test -p mxr-store --lib plan017_ -- --nocapture. Expected: at least four tests pass for complete >10,000-row traversal, stable pagination with tied dates, account isolation, and empty account. Then run the cutoff sync tests and expect canonical IDs to agree in SQLite.

### 3. Settle threading once when a changed sync completes

Persist per-account threading debt when changed envelope/reference data arrives for a provider without native threading. If schema work is needed, add a new migration/module; do not edit existing migrations. Keep debt durable with message upserts or another transactionally consistent marker. A crash between receiving pages must not erase it.

During initial/backfill sync, continue committing message bodies and lexical content per page so reads remain available. Defer the archive-wide canonical threading pass until the backfill completion transition. A completed delta that changed threading input also settles debt. A no-change tick with no debt must not rescan the whole archive. An empty final page with debt must run it. Providers with native threading retain their existing path.

Compute canonical IDs with the existing root/member policy. Update changed memberships in a bounded transaction and capture every changed message ID plus old/new touched thread IDs. Preserve thread tombstones and do not erase unrelated per-thread data without following existing foreign-key behavior.

Future gate: scripts/cargo-test -p mxr-sync --lib plan017_ -- --nocapture. Expected: explicit tests show one canonical pass after a multi-page backfill, a pass on empty final completion, no pass on a clean idle tick, and resumed debt after reopening the store. Add test instrumentation at the query/pass boundary rather than timing assertions.

### 4. Publish changed canonical IDs and preserve repair debt

Build lexical entries for all messages whose thread ID changed, including messages outside the last provider page. Hydrate bodies and metadata in bounded batches through existing store APIs. Read current reply-later and labels from SQLite, preserving plan 016. A single current-page envelope vector cannot supply old changed messages.

Commit the corrected lexical entries before clearing threading/index debt and reporting a completed ready outcome. SQLite and Tantivy cannot commit in one transaction. Persist changed IDs or an equivalent revision-based repair marker in the same transaction as thread changes, so an index error or crash leaves enough work to retry. Clear only the generation/revision actually published, preserving newer debt if sync advanced meanwhile. Startup or the next sync must retry pending publication even when document counts match.

Keep initial-page content readable while repair is pending. Use existing sync progress/failure status to tell clients the pass has not fully completed. Do not hold a global lock or put semantic/contact fanout inside the account provider lane.

This step also owns the unfinished failure/overlap guarantee from 016. Record a changed-message repair marker in the same store transaction as setting/clearing reply-later. A post-send publication failure leaves that marker pending; startup and the existing bounded maintenance/sync completion path retry it by rereading canonical SQLite metadata. Do not clear debt merely because a warning was emitted or document counts match. Accepted delivery remains accepted regardless of repair outcome.

Use one shared account-scoped metadata publication guard owned by the shared SyncEngine, built from existing Tokio mutex types. All production full-document writers in engine.rs, reply_later.rs, mutations.rs, reindex.rs, and account_config.rs must either use that guard or dispatch through the same guarded publisher. Read the current envelope/flag and enqueue/await the index update while holding the account's publication guard. Flag mutations acquire it before writing flags, so a hydrated older flag cannot publish after a completed newer flag mutation. Keep provider network calls outside this guard; do not nest acquisition or invert provider-lane then metadata-guard order. Hold it only for bounded local store/index batches, and allow other accounts to proceed. Full rebuild must use the same publication ordering and preserve pending debt. If a generation/revision approach already landed through 015, reuse it only if the deterministic overlap test proves it prevents stale publication; a lookup followed by an unchecked write is insufficient.

Future gates: scripts/cargo-test -p mxr-sync --lib plan017_ -- --nocapture and scripts/cargo-test -p mxr --lib plan017_ -- --nocapture. Expected: canonical index IDs and owed filtering agree, an injected publish failure survives restart, repeated repair is idempotent, and reply-later metadata remains unchanged.

### 5. Verify scale and client behavior without adding new UI

Use the existing sync_overlap benchmark target to record a compact-header threading case with a deterministic large fixture. Record total input rows, metadata bytes or process peak memory if measurable, pass count, and elapsed time. This is evidence, not a flaky unit-test deadline. Require complete participation of the fixture and bounded body hydration, not a guessed milliseconds threshold. Compare against the old implementation only below its cutoff or explicitly identify its missing work above the cutoff.

CLI, TUI, web and MCP already consume daemon thread/search data. Verify existing requests return canonical IDs and existing threads-changed tombstones invalidate merged-away threads. No new lifecycle enum is intended. If a new status is introduced, stop and amend Scope to include protocol plus every client, instead of leaving it daemon-only.

Future gates:

~~~sh
threading_sqlx_dir="$(mktemp -d /tmp/mxr-plan017-sqlx.XXXXXX)"
cargo run -p mxr-store --example create_db -- "$threading_sqlx_dir/check.db"
DATABASE_URL="sqlite:$threading_sqlx_dir/check.db" cargo sqlx prepare --workspace
DATABASE_URL="sqlite:$threading_sqlx_dir/check.db" cargo sqlx prepare --check --workspace
scripts/cargo-test -p mxr-sync -p mxr-store --tests
scripts/cargo-test -p mxr --lib plan017_ -- --nocapture
scripts/cargo-test -p mxr -p mxr-sync --lib plan016_ -- --nocapture
cargo bench --bench sync_overlap -- plan017
cargo build -p mxr
git diff --check
~~~

Expected: all exit 0; benchmark output contains the newly named plan017 case rather than zero matching benchmarks; at least eight plan017 sync tests and two daemon consumer tests pass. SQLx regeneration follows the existing CI create_db/prepare workflow with an owned temporary DB. Record actual measurements and update the sync guarantee documentation only after these gates pass.

## Regression matrix

| Case | Required outcome |
|---|---|
| Conversation straddles row 10,000 | One canonical thread in SQLite and search |
| Entire conversation older than newest 10,000 | Still threaded correctly |
| Missing Message-ID and subject fallback | Existing library behavior preserved |
| Related messages arrive in different pages | Canonicalization settles at completed backfill |
| Empty final page | Outstanding threading/publication debt settles |
| Empty account or unchanged delta | Successful completion without full scan when no debt |
| Crash after membership commit, before search commit | Durable repair fixes same-count index metadata |
| New debt arrives while old generation publishes | Only completed generation cleared |
| Flagged reply-later message is reindexed | Existing flag remains correct |
| Hydration overlaps SetReplyLater on the same message | Guard serializes writers, or revision validation rejects stale publication; test barriers must not wait for completion while holding the writer's lock |
| Accepted reply's flag clear commits, index fails, daemon restarts | Durable metadata repair clears stale search without another send |
| is:owed consumes a merged thread | Search result is retained using canonical ID |
| Native-threading provider | No added fallback threading pass |
| Two accounts and merged-away threads | No cross-account merge; tombstones preserved |

## Done criteria and stop conditions

- All future gates pass with nonzero new tests/benchmark output.
- The fallback threading path has no arbitrary mailbox-size cutoff.
- Rich envelope/body loading is absent from the full threading input query.
- Search IDs match committed canonical thread IDs after successful completion.
- Repair debt survives failed publication and restart, including same-count metadata changes.
- The paired plan016_ overlap and post-send repair tests pass. Only now is the full reply-later consistency finding accepted as fixed.
- Idle scans and backfill pass count meet the explicit assertions; measured scale evidence is recorded.
- Only Scope paths changed; reviewer receives results and updates the index.

Stop if mail-threading requires a semantic policy change, canonical IDs cannot be preserved, per-thread dependencies would be silently lost, or implementation requires unrelated provider/client redesign. Stop if an incomplete final page is treated as completion or the same gate fails twice. Do not suppress the >10,000 test or replace it with a helper-only assertion.

Maintenance: future sync paths that change thread membership must record publication debt. Any new index-entry builder must preserve stored reply-later metadata. Keep plans 015 and 016's account/completion contracts intact.
