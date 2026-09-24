# Plan 023: Show interrupted mutation jobs as terminal failures

> Executor: this is a future implementation handoff. No tests or builds below ran during plan writing. Follow the steps and stop conditions; the coordinating reviewer owns plans/README.md. Do not edit the index, push, or open a PR without a separate instruction.

## Status

- Priority: P2
- Effort: M
- Risk: MED, shutdown and partial external mutations
- Depends on: plans/011-safe-verification-and-ci.md
- Category: bug
- Planned at: fetched origin/main 9160b1a12ef9dc5d3fb5513b45c68fe57183074f, 2026-09-04
- Source inspected: /tmp/mxr-polish-audit-9160b1a1
- Plan destination: /Users/bhekanik/code/planetaryescape/mxr/plans/023-interrupted-mutation-jobs.md
- Serialize after active plans 014, 015, 016 and 017 are integrated. This shares mutations.rs, server.rs, and potentially startup state with them. No functional dependency on their features is intended.

## Why this matters

A batch job persists progress, but the task running it is not registered for shutdown. A daemon crash/restart can leave its database row permanently running. A user then sees activity that is no longer happening and cannot tell how much mail changed. Finish abandoned records honestly, preserve confirmed progress, and do not replay uncertain external mutations.

This is a static lifecycle finding. No live daemon was killed during the audit.

## Current state

crates/daemon/src/handler/mutations.rs:744-749 persists and starts a job:

~~~rust
persist_job(&state, &job).await;
let background_job_id = job_id.clone();
tokio::spawn(async move {
    run_mutation_job(state, background_job_id, cmd, client_correlation_id).await;
});
~~~

- The task changes the row to Running at :786 and writes a final status only at :871-881. There is no startup transition for stale queued/running mutation jobs.
- persist_job at :922-948 swallows persistence failure, even on the initial write before any mutation. The caller can return JobStarted for a row that does not exist.
- list_jobs at :754-764 reads persisted JSON unchanged. update_job at :954-965 silently stops if the row is absent or cannot be loaded.
- crates/store/src/mutation_jobs.rs stores protocol-owned JSON plus scalar status/timestamps. Its pruning query at :71-81 retains only the newest records without excluding active jobs.
- crates/store/migrations/045_mutation_jobs.sql has no restrictive status check; preserve schema unless a new column is actually necessary.
- crates/protocol/src/types.rs:1780 has Queued, Running, Succeeded, Failed. JobData includes progress, undo_ids, error and finished_at. Existing Failed plus an explicit interruption error is sufficient for this fix; no new enum is required.
- crates/daemon/src/state.rs:74-87 registers and removes account worker handles. Its RuntimeTasks::take_all and shutdown_runtime_tasks provide the shutdown pattern.
- TUI diagnostics already renders job.status, counters, undo IDs and error at crates/tui/src/ui/diagnostics_page.rs:678-705. The web JobsRoute renders the same fields in apps/web/src/features/jobs/JobsRoute.tsx. Reuse these locations.

Existing tests: crates/store/src/mutation_jobs.rs:92, mutation_jobs_round_trip_and_prune; crates/daemon/src/handler/tests/mutations_and_delivery.rs contains the job-start/wait integration around :657. Use fake providers with barriers and file-backed temporary stores for restart tests, not process-wide pgrep/kill.

## Lessons applied from Obsidian

"Nonblocking Work Needs a Visible Home" means status must remain visible in the existing job list and reflect persisted truth. "Zero Change Can Be Success" means an idempotent recovery pass that finds no abandoned jobs succeeds. "Retrying a Permanent Error Is Data Loss" means reporting interruption must not start an unbounded replay loop. "Provider Lanes Need Account Scope" requires shutdown/job coordination to preserve account-local provider serialization. These are paraphrased from read-only notes; no personal examples are copied.

## Scope

- crates/daemon/src/handler/mutations.rs
- crates/daemon/src/handler/tests/mutations_and_delivery.rs
- crates/daemon/src/server.rs
- crates/daemon/src/state.rs
- crates/store/src/mutation_jobs.rs
- crates/tui/src/ui/diagnostics_page.rs, rendering tests only unless an interruption message is hidden
- apps/web/src/features/jobs/JobsRoute.tsx and a new apps/web/src/features/jobs/JobsRoute.test.tsx
- crates/mcp/src/lib.rs, tests only if an existing job inspection or generic permitted read capability already reaches ListJobs/GetJob
- crates/daemon/src/commands/history.rs and crates/daemon/src/main.rs, only existing job text/JSON formatting tests or correction
- crates/web/src/routes_v6.rs, route tests only unless an existing job response drops error/progress

No database migration or new protocol state is expected. Out of scope: automatic resume/retry, job scheduling priorities, a queue framework, new dashboards, cancelling arbitrary OS processes, altering provider dedup guarantees, or changing undo retention policy beyond protecting active jobs from premature pruning.

## Git and commands

Use an isolated worktree on codex/023-interrupted-mutation-jobs. Fetch and check drift:

~~~sh
git fetch origin main
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/daemon/src/handler/mutations.rs crates/daemon/src/handler/tests/mutations_and_delivery.rs crates/daemon/src/server.rs crates/daemon/src/state.rs crates/store/src/mutation_jobs.rs crates/tui/src/ui/diagnostics_page.rs apps/web/src/features/jobs crates/mcp/src/lib.rs crates/daemon/src/commands/history.rs crates/daemon/src/main.rs crates/web/src/routes_v6.rs
~~~

Expected: completed predecessor edits are accounted for and Current state still identifies the affected lifecycle. Stop on unexplained changes. Never switch/reset/clean the primary checkout. Use type: description commits, user author only.

All following commands are FUTURE gates. Complete 011 first and use its repaired scripts/cargo-test and isolated MXR_* environment. Tests own their stores, tasks, sockets, and fake providers. Never repurpose HOME/CODEX_HOME or kill unrelated processes.

## Steps

### 1. Characterize interrupted and unpersisted jobs

Add plan023_ tests to the existing daemon mutation tests. Seed file-backed queued and running rows with partial progress, nonempty undo IDs, and an unfinished final chunk. Reopen the store and invoke a factored startup reconciliation function. Assert each abandoned job becomes Failed with finished_at and an interruption message, preserving confirmed counters and undo IDs. Succeeded/Failed rows stay unchanged.

Add an initial-persistence failure test: starting the job must return an error and never invoke the fake provider. Use a fake provider barrier to characterize shutdown while a chunk is in progress. Do not use sleeps or terminate any real daemon.

Future gate: scripts/cargo-test -p mxr --lib plan023_ -- --nocapture. This is the deliberate RED step: expect abandoned-state/initial-write assertions to fail on current code, not test setup. Capture actual failures.

### 2. Make job persistence and recovery truthful

Change the initial persist_job call to propagate failure before spawning or returning JobStarted. Split initial required persistence from best-effort diagnostic logging if necessary. If saving confirmed progress later fails, stop scheduling further chunks, preserve the known in-memory result, and expose a failure through existing operation events. Do not claim durable progress that the store rejected.

Add a store query for active persisted rows and a conditional update using expected queued/running status. Keep Store protocol-agnostic: daemon deserializes JobData, edits the existing fields, and asks Store to update scalar columns and data_json together. Reconciliation must not scan only the retained display limit. Corrupt JSON must produce a diagnostic identifying the job and stop silent disappearance; do not invent progress from it.

Use status Failed with error text that says the daemon was interrupted, counters contain only confirmed progress, and the last in-flight chunk may have applied. Preserve undo IDs already persisted. Unknown side effects are not counted as confirmed failure or success. A second recovery pass changes zero rows and succeeds. Prune only terminal jobs so heavy concurrent activity cannot erase a running job.

Future gate: scripts/cargo-test -p mxr-store --lib plan023_ -- --nocapture. Expected: at least four tests pass covering conditional update, scalar/JSON agreement, terminal-only pruning, and more active rows than the display limit. Run the existing mutation_jobs_round_trip_and_prune test and update its fixtures to reflect terminal-only retention.

### 3. Reconcile old jobs before accepting new work

Run abandoned-job reconciliation during startup before accepting new mutation jobs. If startup maintenance remains asynchronous, capture the exact preexisting job IDs before readiness and reconcile only that snapshot with conditional updates; never mark a newly started job failed merely because startup maintenance runs later. Do not use an arbitrary age timeout for ownership.

Record an existing operation failure/event for each interrupted job using job IDs, confirmed counters and a minimal reason. ListJobs/GetJob must return the terminal state after restart. Keep the recovery routine independently testable against a reopened temporary store.

Future gate: scripts/cargo-test -p mxr --lib plan023_ -- --nocapture. Expected: queued/running recovery, repeat recovery, terminal preservation, and a startup/new-job race test pass. A job created after the startup snapshot remains active.

### 4. Register running tasks for bounded shutdown

Add job task registration/removal to RuntimeTasks using the existing handle ownership pattern. Completed handles must be removed; a task must not await or abort itself. Stop accepting new jobs once shutdown starts. Check shutdown between chunks and persist the interruption status before exiting. Give an in-flight provider operation the existing bounded shutdown grace; do not hold a global lock and do not issue extra provider calls to compensate for interruption.

If the task is aborted while an external operation is unresolved, leave its durable row recoverable by step 3. A restart does not resume or replay it. Preserve existing per-account provider guards and mutation IDs; do not infer that those IDs guarantee remote idempotency.

Future gate: scripts/cargo-test -p mxr --lib plan023_ -- --nocapture. Expected: barrier tests prove no additional chunks begin after shutdown, task handles are removed on completion, unfinished work becomes terminal/recoverable, and a healthy other account is not cancelled by the first account's job failure.

### 5. Verify existing job views across clients

Use the existing Failed state and error field in CLI text/JSON, TUI diagnostics and web JobsRoute. A interrupted 4/10 job must still say 4/10 confirmed, not imply all ten ran or that it is still running. Display its error persistently beside the progress. Add TUI and web tests for this case and zero-change recovery.

Check whether MCP already exposes job inspection directly or through an existing permitted generic read capability. If so, test that existing path returns the same job ID/status/progress/error and preserves source=mcp and daemon permission checks. If no public MCP capability reaches jobs, record MCP job inspection as N/A for this plan. The internal daemon_json helper alone is not a public tool. Do not add standalone MCP tools or a generic request tool to expand parity. This plan corrects existing job views.

Future gates:

~~~sh
scripts/cargo-test -p mxr --lib plan023_ -- --nocapture
scripts/cargo-test -p mxr-store --lib mutation_jobs -- --nocapture
scripts/cargo-test -p mxr-tui -p mxr-web --lib plan023_ -- --nocapture
npm --prefix apps/web run test -- src/features/jobs/JobsRoute.test.tsx
npm --prefix apps/web run typecheck
cargo build -p mxr
git diff --check
~~~

Expected: exit 0 throughout, at least eight daemon plan023_ tests, at least four store plan023_ tests, and nonzero rendering/route regression tests in each changed client crate. If an existing MCP job read capability was found and tested, additionally run scripts/cargo-test -p mxr-mcp --lib plan023_ -- --nocapture and require nonzero matching tests; otherwise record that gate N/A. The new web test passes; no schema/OpenAPI generation is needed if existing fields remain unchanged. If a protocol or schema addition becomes necessary, stop and amend Scope plus compatibility/migration gates first.

## Regression matrix

| Case | Required outcome |
|---|---|
| Queued/running rows from a previous process | Failed with finished_at and clear interruption reason |
| Partial job with persisted undo IDs | Confirmed progress and IDs preserved |
| Last chunk may have applied but progress was not persisted | Error states uncertainty; no invented counter or automatic replay |
| Initial job-row write fails | No JobStarted and zero provider calls |
| Mid-job persistence fails | No additional chunks scheduled; failure observable |
| New job begins while startup maintenance is pending | Not mistaken for an abandoned job |
| Already succeeded/failed or no active rows | Unchanged, recovery succeeds with zero changes |
| Shutdown between chunks | Stops cleanly and persists terminal state |
| Shutdown during provider call | Bounded task handling; startup can resolve abandoned status |
| More active jobs than history retention limit | Active records are not pruned |
| Existing CLI/TUI/web job views and any existing MCP job read path | Same failed state, confirmed counters, undo IDs and error; MCP N/A if no existing capability |

## Done criteria and stop conditions

- All future gates pass and new tests have nonzero counts.
- A persisted job never remains running solely because its process ended.
- Startup cannot mark a newly accepted job interrupted.
- JobStarted requires successful durable insertion; terminal pruning preserves active rows.
- Interrupted jobs never auto-resume, and progress clearly means confirmed work only.
- All existing client views can inspect the same interruption state.
- Only Scope files changed; send actual results to the coordinating reviewer for index update.

Stop if making counters exact would require replaying an unknown provider operation, startup ownership cannot be determined safely, or a fix requires a queue framework/new status enum. Also stop on unexplained drift or after the same gate fails twice. Do not replace uncertainty with fabricated success/failure counts.

Maintenance: review task ownership and startup timing together. Future mutation-job retry features must be a separate design with explicit side-effect/idempotency evidence; this plan intentionally provides truthful interruption reporting only.
