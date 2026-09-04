# Plan 022: Resolve truthful previews once and use them across action surfaces

> Follow the steps and verification gates. This file describes future implementation; no passing checks are claimed. The coordinator owns `plans/README.md` during this batch.

## Status

- **Priority:** P1
- **Effort:** L — deliver in the bounded vertical slices below
- **Risk:** MED–HIGH — mutation intent, permissions, and expiring Undo eligibility
- **Depends on:** `011-safe-verification-and-ci.md`, `020-web-mutation-consistency.md`, `021-web-navigation-and-keyboard.md`
- **Category:** bug
- **Planned at:** fetched `origin/main` `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- **Consumer:** Plan 025 real client journey verification

## Why this matters

The CLI's Undo dry run succeeds without checking whether the mutation exists or can still be undone. The MCP mutation preview reads envelopes without passing through execution's action validation. In the web mailbox, keyboard batch archive/trash/spam bypass the confirmation used by toolbar buttons. Make the existing preview and confirmation promises truthful without changing what a user's selected message IDs mean.

## Current state

At `crates/daemon/src/commands/mutations/mod.rs:22`, Undo dry run never connects to the daemon:

```rust
if dry_run {
    print_undo_output(&mutation_id, true, format)?;
    return Ok(());
}
```

Actual Undo at `handler/mutations.rs:1105` reads the undo entry and checks `entry.expires_at <= now`; it also needs accounts/providers/current envelopes. Its expired path deletes the entry. A pure preview must not run that cleanup or any provider mutation.

`commands/mutations/helpers.rs:417` prints the normal selection preview before `build_request(selection.ids)` is called at line 433. Execution's actual typed verb/payload therefore is not the input to preview. `handler/mutations.rs:533` resolves IDs/account groups for execution, then locates providers at line 614. The `Route { dry_run: true }` branch at line 552 returns before those provider checks.

`crates/mcp/src/lib.rs:384` previews by `ListEnvelopesByIds`, then echoes `input.action`; `mutate` at line 411 builds a typed command separately. Current supported MCP actions are archive, read-and-archive, trash, spam, read/unread, and star/unstar. There is no current MCP Undo tool; do not add an unrelated tool just to copy a button.

`crates/protocol/src/types.rs:910` only has executing `UndoMutation { mutation_id }`. `crates/web/src/routes_v6.rs:842` has an executing Undo route. Existing agent-profile classification/account scope is exhaustive in `handler/mod.rs`; new read-only preview variants still require account allowlists and must not grant mutation permission.

`apps/web/src/features/mailbox/BulkActionBar.tsx:82` opens confirmation for archive/trash/spam. `MailboxList.tsx:299,309,319` calls `archive/spam/trash.mutate(ids)` directly for `e`, `!`, Delete/Backspace. The selected set is the currently loaded explicit message IDs, not all matches. `features/mailbox/actions.ts:175` also calls read-and-archive directly, bypassing the shared result handling.

The TUI has the useful central pattern at `crates/tui/src/app/mutation_helpers.rs:815`:

```rust
if count > 1 {
    self.modals.pending_bulk_confirm = Some(PendingBulkConfirm {
        title,
        detail,
        request,
        effect,
        optimistic_effect,
        status_message,
    });
}
```

It already stores the exact request with the dialog. Preserve that property when adding daemon eligibility information. The project contract in `.agents/skills/mxr-development/SKILL.md` is “Destructive or batch mutations need a dry-run/preview path before commit.” Previewing does not itself authorize execution.

## Lessons carried forward

- **Same-Code-Path Preview:** one resolved operation includes verb, IDs, account, destination/label changes, and validation. Preview renders it; execution applies it. A parallel envelope-count query is insufficient.
- **Keyboard Parity Is a Product Contract:** keyboard, toolbar, and palette users need the same ability to review, cancel, perform, and recover. Page-local motions can remain local.
- **Optimistic Retries Preserve User Intent:** preview is read-only and retryable; do not retry an external side effect merely because preview succeeded. Preserve existing safe correlation/reconciliation policy.
- **Parser Parity Is Not Execution Parity:** compare actual effects against the preview using the real daemon/store/FakeProvider. A serialized command shape alone is not proof.

## Scope

Only these files or explicitly named new helpers/tests:

- `crates/protocol/src/types.rs`, `crates/protocol/src/lib.rs` — additive preview request/response DTOs, category mapping and serialization tests
- `crates/daemon/src/handler/mutations.rs`, `handler/mod.rs`; optional new `handler/mutation_preview.rs` registered normally from `mod.rs`
- `crates/daemon/src/handler/tests/mutations_and_delivery.rs`, `routing_and_search.rs`
- `crates/daemon/src/commands/mutations/helpers.rs`, `commands/mutations/mod.rs`; `crates/daemon/tests/cli_journey.rs`; root `Cargo.toml` only for a test-only `rmcp = { workspace = true }` dependency if the real MCP journey requires its public parameter wrapper, plus the corresponding existing-package edge in `Cargo.lock` if Cargo changes it
- `crates/daemon/src/activity/mapper.rs` only to classify previews as read-only events through existing activity writer; no bodies/credentials/attachment bytes
- `crates/web/src/routes_v6.rs`, `router.rs`, `openapi.rs`; `crates/web/tests/integration.rs`
- `crates/mcp/src/lib.rs` — existing mutation preview/apply tools and their tests
- `crates/tui/src/app/mutation_helpers.rs`, `app/mutation_actions.rs`, `app/mod.rs`, `app/input.rs`, `async_result.rs`, `ipc.rs`, `runtime.rs`, `runner.rs`, `ui/bulk_confirm_modal.rs`; focused existing `runner/tests/mutations_and_bulk.rs`
- `apps/web/src/features/mailbox/BulkActionBar.tsx`, `MailboxList.tsx`, `MailboxList.test.tsx`, `actions.ts`, `actions.test.ts`, `api.ts`, `types.ts`, `useOptimisticMailMutation.ts`, `.test.tsx`
- New `apps/web/src/features/mailbox/useMailAction.ts`, `.test.tsx`, and `MutationPreviewDialog.tsx` if needed to replace duplicated confirmation ownership
- `apps/web/src/state/modalStore.ts` only for the shared dialog; `src/api/generated.ts` by generation
- `apps/web/e2e/mutations.spec.ts`; new `mutation-preview-parity.spec.ts`
- Existing preview/Undo usage examples in `site/src/content/docs/guides/for-agents.md` and `site/src/content/docs/guides/recipes.md`

No changes to provider implementations, query semantics, account selection semantics, sending, draft safety, unrelated rule/snooze/unsubscribe flows, or retry/idempotency policy. Do not build a generic workflow engine or durable preview-token service. Do not reimplement label/folder mappings in clients. Plan 021's context resolution and Plan 020's mutation completion/invalidation must be reused.

## Commands, environment, and Git

Use branch `codex/022-shared-mutation-previews` in a disposable worktree. Fetch `origin main` and record fetched/working SHAs. Inspect baseline differences and reconcile the prerequisite interfaces before editing:

```sh
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/protocol crates/daemon/src/handler crates/daemon/src/commands/mutations crates/daemon/src/activity/mapper.rs crates/daemon/tests/cli_journey.rs crates/web crates/mcp crates/tui apps/web/src/features/mailbox apps/web/src/state/modalStore.ts apps/web/e2e
git status --short
```

Use Node 24 and locked dependencies (`node --version` → `v24.*`; `npm ci --prefix apps/web`). Use `scripts/cargo-test` only after Plan 011 removes global process killing. Root package is `mxr`, other packages are `mxr-protocol`, `mxr-web`, `mxr-mcp`, and `mxr-tui`.

| Purpose | Command | Success |
|---|---|---|
| Protocol | `scripts/cargo-test -p mxr-protocol --tests` | All pass |
| Handler regression | `scripts/cargo-test -p mxr --lib preview` and `scripts/cargo-test -p mxr --lib undo` | Named new/old tests execute and pass |
| CLI journey | `scripts/cargo-test -p mxr --test cli_journey undo` and `scripts/cargo-test -p mxr --test cli_journey preview` | Nonzero intended tests pass |
| MCP | `scripts/cargo-test -p mxr-mcp --tests` | All pass |
| Bridge | `scripts/cargo-test -p mxr-web --tests` | All pass |
| TUI | `scripts/cargo-test -p mxr-tui --tests` | All pass |
| API types | `npm --prefix apps/web run gen:types` | Exit 0; generated changes reviewed |
| Web unit | `npm --prefix apps/web test -- src/features/mailbox/useMailAction.test.tsx src/features/mailbox/MailboxList.test.tsx src/features/mailbox/actions.test.ts src/features/mailbox/useOptimisticMailMutation.test.tsx` | Nonzero tests pass |
| Web static | `npm --prefix apps/web run typecheck` and `npm --prefix apps/web run lint` | Exit 0 |
| Build | `cargo build -p mxr` | Exit 0 |
| Browser | From `apps/web`: `npm run e2e -- e2e/mutations.spec.ts e2e/mutation-preview-parity.spec.ts` | Pass with isolated harness |

These are future commands. New test names must contain the filters shown, or update commands to match actual test names; zero tests is not success. Use the existing `.playwright` real-daemon/FakeProvider harness and require free ports; never access real email, publish mutations externally, or terminate unrelated daemons. Standard commit example: `fix: share mutation preview validation across clients`; no attribution or publishing without instruction.

## Steps

### 1. Prove the preview discrepancies at daemon boundaries

Use `handler/tests/mutations_and_delivery.rs` fixtures (e.g. `undo_archive_restores_inbox_label`) and the CLI archive/Undo journey in `cli_journey.rs:588`. Add an isolated CLI test asserting unknown Undo dry-run ID fails, rather than printing “Would undo.” Add fixtures for valid, expired, consumed, and locally missing-message Undo. Supply explicit time or set stored expiry relative to injected time; do not sleep a minute in tests.

Add a web test selecting multiple explicit IDs, pressing each bulk shortcut, and asserting zero apply requests before review/confirmation. Add MCP preview tests for a missing message and an unavailable/disallowed account through the real daemon request path, not only `FakeRequester` returning Pong.

Put the real MCP journey in `crates/daemon/tests/cli_journey.rs`, whose existing `DaemonGuard`/temporary config helpers already isolate a fake account. Construct `mxr_mcp::MxrMcpServer::new(mxr_mcp::UnixDaemonRequester::new(socket_path))`, invoke the public preview/apply methods using the SDK's `Parameters` wrapper, and inspect daemon state afterwards. `UnixDaemonRequester` connects with `ClientKind::Mcp`, so this exercises real source-profile enforcement. Root already depends on `mxr-mcp`; a test-only dependency on the existing workspace `rmcp` is permitted for this wrapper. Do not make the MCP crate depend on the daemon crate.

**Verify:** current CLI/web regressions fail for the intended reasons. Protocol additions required to express daemon preview tests may be skeletal, but the failures must concern eligibility/effect behavior, not an unrelated compile error.

### 2. Resolve a typed operation in the daemon without side effects

Introduce additive read-only requests such as `PreviewMutation { mutation: MutationCommand }` and `PreviewUndoMutation { mutation_id }`, with DTOs carrying the typed action/payload, exact ordered/deduplicated selection consistent with execution, per-account availability, local validation issues, and Undo expiry when applicable. Preserve existing executing request shapes and backwards-compatible serialization.

Extract only the local preparation needed by both preview and execution from `mutation` and `undo_mutation`. It resolves the supplied IDs, account/provider availability through traits, actual local label targets/flags, and relevant permission scope. Execution consumes that prepared operation immediately within the same request rather than selecting again. Do not make preview call provider mutations, write undo rows, delete expired rows, emit successful mutation events, or change stored mail. Local preparation cannot promise a remote provider will accept an action; represent that boundary honestly.

For Undo, preview eligibility is time-sensitive. Execution must read/revalidate the entry and current time again even after a successful preview. Unknown/expired/consumed IDs remain failures. Do not cache a preview as authority to bypass expiry or account checks. Reuse existing undo application behavior; if truthful preview requires fixing partial-Undo reporting beyond this scope, stop and surface that requirement rather than claiming all messages will restore.

Update exhaustive request category/classification/account-scope/activity matches. Preview remains subject to account allowlists, but read permission must never permit the corresponding write. Return structured local rejection reasons; do not expose full bodies or credentials in previews/activity.

**Verify:** protocol, handler preview/Undo, and permission-routing tests pass. Compare store mail state and undo-entry count before/after every preview; assert provider mutation call count is zero. Existing executing archive/Undo behavior and old serialized requests pass unchanged.

### 3. Use daemon previews in existing CLI and MCP flows

Change CLI Undo dry run to request the daemon preview, render actual eligibility/targets/expiry, and fail nonzero for ineligible entries. Keep existing machine-output fields where possible; additive fields must be documented and tested. Never print a successful dry-run claim from an arbitrary ID alone.

In `run_simple_mutation`, build the typed request from the resolved `MutationSelection` before the dry-run/confirmation branch. Derive preview action and execution from that same command; `--yes` skips confirmation and must not choose a different verb. Keep existing query limits, account scope, batch job threshold, and exact selected IDs. CLI preview and a later separate CLI invocation are not a transaction: report that eligibility is checked again, and do not silently widen explicit IDs to all query results.

Replace MCP `mutation_preview` envelope-only lookup with the new daemon preview built through the same `build_mutation` used by `mutate`. Keep `confirm=true` write gating and existing supported action list. The MCP execution request must still undergo daemon permission checks even if preview was allowed. Do not add an MCP Undo tool as part of this repair; there is no existing one to fix.

**Verify:** CLI and MCP tests pass. A table/JSON/JSONL preview identifies the exact verb and targets; malformed/unknown/ineligible Undo fails; preview sends no provider mutation. Execute valid reviewed explicit IDs against FakeProvider and compare actual changes to the preview.

### 4. Expose preparation through bridge and TUI without changing intent

Add bridge preview routes for existing typed mutations and Undo using the same dispatcher and authentication as executing routes; register OpenAPI and regenerate TS. Avoid approximate web-only counts. The bridge should return the daemon's structured result, including local eligibility and expiry.

For TUI bulk confirmation, enrich the existing `PendingBulkConfirm` request with the daemon preview via the existing async request/result flow. Loading/failure must be visible, and Enter cannot execute while preview is unavailable. Keep the frozen request IDs/payload while the dialog is open. A later selection change must not replace those IDs. For existing Undo action, use the same preparation validation inside execution; an explicit preview surface may show eligibility without adding a blocking confirmation to the deliberate `z` action. Never imply preview extends the Undo window.

**Verify:** bridge integration tests verify auth and DTOs through real dispatcher; TUI tests prove preview loading/failure/cancel/confirm behavior and exact request preservation. API generation/typecheck pass. Preview expiry between review and execution returns the daemon failure rather than applying stale authority.

### 5. Funnel existing web input methods through one review/apply controller

Use one web action controller for the supported typed mailbox mutations. It freezes `{action, messageIds, payload}` at invocation, uses daemon preview for batch/destructive review, and calls Plan 020's shared mutation/completion path only after confirmation. Toolbar, `MailboxList` keyboard handlers, and applicable registry actions must use this controller. Reuse Plan 021's origin/target resolution. Remove the direct read-and-archive bypass as part of this funnel.

Keep deliberate single-message shortcut behavior consistent with the project's existing policy; do not add blanket modal friction. For multi-message selections, use the same preview dialog independent of keyboard/mouse/palette. For currently previewed destructive single actions, retain the established review path. The displayed verb, labels/destination, IDs/count and account restrictions must come from the frozen operation, not a later live selection. Cancel clears only the pending review, not the user's selection. A pending preview error prevents apply and offers a safe retry of preview only.

Use actual mutation response/Undo availability: show Undo only when the daemon supplies it; do not promise Undo for Move/ModifyLabels/Star merely because a generic description says “label mutation.” Browser Undo can refresh eligibility for its display, but execution must still revalidate. Expired Undo must show a concise reason and preserve authoritative cache state.

**Verify:** web unit/browser commands pass for all matrix rows. Assert exactly zero apply calls before confirm and exactly one after; successful calls go to the real bridge/FakeProvider. Recheck Plan 020 mutation consistency and Plan 021 keyboard/focus journeys where the controller touches them.

## Test matrix and done criteria

| Case | Required result |
|---|---|
| Unknown/expired/consumed Undo | Dry run rejects without writes; execution rejects too |
| Valid Undo preview | Exact local targets/expiry; no mutation or entry consumption |
| Expiry between preview/apply | Execution rejects; preview grants no extra window |
| Account denied/unavailable | Preview does not leak disallowed mail or promise local eligibility |
| Missing local message/label | Same preparation validation in preview and execution |
| Remote rejects after eligible preview | Honest execution error; no claim preview guaranteed provider acceptance |
| Keyboard/mouse/palette bulk action | Same frozen verb/IDs/payload; no apply before confirm |
| Selection changes while reviewing | Confirm still uses reviewed IDs, or requires explicit new preview; never silent replacement |
| Cancel/retry preview | No write; visible focus returns; only read operation retried |
| Partial mutation result | Existing completion handling reports actual counts and reconciles truth |
| JSON/JSONL/stdout | Parseable output; no human progress mixed into records |
| MCP preview allowed, write denied | Write remains denied; preview never grants authorization |

Done requires all commands to pass with nonzero tests, preview side-effect counters at zero, actual FakeProvider effects matching reviewed explicit IDs/verb, and no duplicated preview query/verb interpretation. Record exact commands/results and share the client matrix with Plan 025. This closes the relevant Plan 009 B4/confirmation work only; other legacy items remain separate.

## STOP conditions and maintenance

Stop if a fix needs to expand selection/query scope, change message actions into thread-wide actions, create a durable preview service, grant new permissions, add unsafe retries, or invent client-specific provider logic. Stop after two repeated failures, missing intended test collection, unexplained prerequisite drift, or required edits outside scope. If execution performs a hidden side effect during purported preparation, extract that side effect before declaring preview read-only; report a larger boundary instead of running it in dry-run mode.
