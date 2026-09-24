# Plan 020: Make web mutations immediate and keep every mail view consistent

> Execute each step with its verification gate. Commands below are future checks, not claims that implementation exists or tests passed. The coordinator owns the plans index during this batch.

## Status

- **Priority:** P1
- **Effort:** L
- **Risk:** MED — concurrent optimistic state and rollback
- **Depends on:** `011-safe-verification-and-ci.md`, `018-saved-search-pagination.md`
- **Category:** bug
- **Planned at:** fetched `origin/main` `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- **Followed by:** Plan 021 navigation, then Plan 022 preview/dispatch consistency; Plan 025 consumes the journey tests

## Why this matters

The mailbox uses TanStack infinite-query data, but its optimistic mutation code handles a flat object. Actions therefore wait for network completion and refetch. Search results and their grouping counts also miss invalidation after mutations and Undo. Repair the existing action path so feedback is immediate, failed work recovers honestly, and a user can trust the view after each action.

## Current state

`apps/web/src/features/mailbox/useMailboxQuery.ts:77` stores `{ pages, pageParams }` through `useInfiniteQuery`. Its `select` merges pages for rendering; that does not change the stored cache shape.

`apps/web/src/features/mailbox/useOptimisticMailMutation.ts:85,115` currently does this:

```ts
function isMailboxResponse(value: unknown): value is MailboxResponse {
  return typeof value === "object" && value !== null && "mailbox" in value;
}

function snapshotAndMutate(qc: QueryClient, ids: string[], action: MailAction): MutationContext {
  const idSet = new Set(ids);
  const snapshots: MutationContext["snapshots"] = [];
  for (const [queryKey, data] of qc.getQueriesData({ queryKey: ["mailbox"] })) {
    if (!isMailboxResponse(data)) continue;
    snapshots.push([queryKey, data]);
    qc.setQueryData(queryKey, mapMailboxRows(data, idSet, action));
  }
  return { snapshots };
}
```

An in-memory audit reproduction invoked this real function against an actual `QueryClient.fetchInfiniteQuery` cache. Archiving `m1` produced `{ "cacheShape": ["pages", "pageParams"], "snapshotCount": 0, "remaining": ["m1", "m2"] }`. The existing test at `useOptimisticMailMutation.test.tsx:112` seeds a flat `{mailbox}` object and misses the bug.

Other load-bearing facts:

- `useOptimisticMailMutation.ts:97` removes archive/trash/spam/move/route/label-remove matches from every cached lens, irrespective of membership. Fixing the shape alone would expose incorrect removal from All Mail and unrelated labels.
- `:126` restores whole cached snapshots; overlapping actions would overwrite each other's effects once optimistic changes work.
- `:269` serializes request execution through `requestCoordinator.enqueueMutation`; `:276` optimistic `onMutate` runs outside that queue. Serialized HTTP alone does not serialize optimistic state.
- `:322` and `performUndo` at `:31` invalidate mailbox/thread/shell only. Search keys are `search`, `search-groups`, and `search-palette`.
- `hooks/useDaemonEventInvalidation.ts:18` lists `MailUpdated`/`MailRemoved`, which are not current protocol variants. `LabelCountsUpdated` refreshes shell only. Do not invent a new event just to paper over missing mutation invalidation.
- `features/mailbox/actions.ts:175` calls `readAndArchiveMessages` directly and toasts success without inspecting the mutation result. Plan 022 will funnel these invocation surfaces; this plan provides the shared result/reconciliation mechanism it must reuse.

Project rules: clients use the bridge/IPC; provider logic remains in provider crates. `.agents/skills/mxr-development/SKILL.md` requires structured, pipeable read surfaces and shared daemon behavior. Keep React Query for server data and the existing request coordinator for mutation scheduling; do not install another state or query library.

## Lessons carried forward

- **Optimistic Retries Preserve User Intent:** pending is distinct from rejected. Preserve pending feedback while an already-supported safe retry is running; rollback only after rejection/exhaustion. This plan does not introduce generic retries or retry send/unsubscribe.
- **Keyboard Parity Is a Product Contract:** every invocation surface must eventually share success, failure, and recovery. Plan 022 connects the dispatch layer after this one stabilizes it.
- **Parser Parity Is Not Execution Parity:** tests must populate production cache shapes and observe actual rendered/query behavior; flat mock data cannot prove an infinite-list contract.

## Scope

Only these source paths and their named tests:

- `apps/web/src/features/mailbox/useOptimisticMailMutation.ts`, `.test.tsx`, `useMailboxQuery.ts`, `useMailboxQuery.test.tsx`, `types.ts`
- New `apps/web/src/features/mailbox/mutationProjection.ts` and `.test.ts` if projection cannot remain clearly bounded in the existing hook
- New `apps/web/src/features/mailbox/invalidateMailQueries.ts` and `.test.ts`
- `apps/web/src/features/search/SearchResultsRoute.tsx` and `.test.tsx`; `features/search/api.ts` only to reuse existing query-key constructors
- `apps/web/src/hooks/useDaemonEventInvalidation.ts` and `.test.tsx`
- `apps/web/src/lib/requestCoordinator.ts` and `.test.ts` only if joining existing reconciliation scheduling is required
- `apps/web/src/state/selectionStore.ts` only for conditional selection restoration on failure
- `apps/web/e2e/mutations.spec.ts`, `search-live.spec.ts`; new `mutation-consistency.spec.ts`

Do not change daemon/provider behavior, query syntax, thread-versus-message selection, retry policy, action confirmation rules, route formats, compose/send, or stylesheet design. Plan 021 owns focus/navigation; Plan 022 owns action dispatch and previews. Update no generated APIs unless an unexpected schema issue is reported and the scope is reviewed.

## Environment, commands, and Git

Use a disposable branch `codex/020-web-mutation-consistency`; do not reset or switch the user's active checkout. Fetch `origin main`, record fetched and working SHAs, and compare expected Plan 018 changes before editing:

```sh
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- apps/web/src/features/mailbox apps/web/src/features/search apps/web/src/hooks apps/web/src/lib/requestCoordinator.ts apps/web/src/state/selectionStore.ts apps/web/e2e
git status --short
```

Use Node 24 (`node --version` reports `v24.*`) and locked dependencies (`npm ci --prefix apps/web`). `package.json` defines the commands below; do not use a global/latest test runner. The repaired test wrapper from Plan 011 is required for Rust verification; never run the old global process-killing script.

| Purpose | Command | Success |
|---|---|---|
| Hook and projection tests | `npm --prefix apps/web test -- src/features/mailbox/useOptimisticMailMutation.test.tsx src/features/mailbox/useMailboxQuery.test.tsx src/features/mailbox/mutationProjection.test.ts src/features/mailbox/invalidateMailQueries.test.ts` | All relevant files collected; nonzero tests pass |
| Event/search tests | `npm --prefix apps/web test -- src/hooks/useDaemonEventInvalidation.test.tsx src/features/search/SearchResultsRoute.test.tsx src/lib/requestCoordinator.test.ts` | All pass |
| Typecheck | `npm --prefix apps/web run typecheck` | Exit 0 |
| Lint | `npm --prefix apps/web run lint` | Exit 0 |
| Build daemon | `cargo build -p mxr` | Exit 0 |
| Browser | From `apps/web`: `npm run e2e -- e2e/mutations.spec.ts e2e/mutation-consistency.spec.ts e2e/search-live.spec.ts` | All pass against isolated harness |

If projection/helper tests stay colocated, remove nonexistent new filenames from that command and record where each case lives; do not accept a zero-test success (`vitest.config.ts` allows it). Playwright uses `.playwright` FakeProvider state and `target-cli/debug/mxr`; inspect `scripts/e2e-server.mjs` before running. Require free test ports; do not kill unrelated processes. No real email or external mutations.

Use conventional commits, e.g. `fix: reconcile web mail mutations across cached pages`. No attribution footers, pushing, or publishing without instruction.

## Steps

### 1. Reproduce using production data flow

Replace flat-only optimistic fixtures with a `QueryClient` populated by `fetchInfiniteQuery`, or render the real `useMailboxQuery` hook with HTTP intercepted by MSW. Keep existing partial-failure coverage. Hold the mutation response unresolved and assert the rendered list updates before the response resolves. Use two pages and two cached lenses, not just one row.

Add failed-action A followed by successful-action B tests on different rows and the same row. Add an intervening refetch/daemon event. Assert no response or rollback reverts B. Add a test that mark-read removes the item from `is:unread` after successful reconciliation without a later sync event.

**Verify:** run the hook/event/search commands; capture the expected existing-code failures. New failures must assert user-visible state/cache shape, not private helper call counts.

### 2. Project actions onto real infinite pages without whole-cache rollback

Use TanStack's existing mutation state as the source of pending action intent, with one shared mutation key and an ordered operation descriptor containing action, explicit IDs, payload, and client operation identity. Use built-in arrays/maps and existing query state; no second global mail store. Derive the optimistic mailbox view by applying outstanding descriptors to the authoritative page data in the production hook's selection path. Refetches become new authoritative bases; pending actions are projected over them. Removing a failed descriptor naturally removes only that action's projection, avoiding a stale whole-cache restore.

Keep each descriptor active until the authoritative invalidation/refetch for that completed request settles; otherwise a success can briefly reappear between clearing pending state and the fetch. Failed descriptors stop projecting and show the existing error. Do not cancel subsequent legitimate user actions when an earlier one fails. Preserve pageParams, paging metadata, and selection meaning from Plan 018. If a mutation has partial results, show the existing count/error and reconcile daemon truth rather than pretending every ID succeeded.

The pure projection function must understand the query's lens and the affected fields:

- Read/unread/star/unstar can update known row flags without inventing membership.
- Archive removes an Inbox member; it does not remove the message from All Mail or an unrelated label.
- Label removal hides a row only when removing the currently viewed label; moves/routes use their actual resolved labels and documented flags.
- When membership of an arbitrary saved/search query cannot be evaluated from available data, keep the row pending and let authoritative search determine inclusion. Do not recreate the search parser in TypeScript or claim a speculative removal.
- A thread row still represents the current explicit message ID. Do not turn one selected representative into all messages in the thread.

Expose pending state using an existing affordance if needed; do not announce final success before acknowledgement. Restore a failed selection only if the user's selection generation/scope has not changed since invocation; otherwise preserve their newer selection.

**Verify:** hook/projection tests pass for all matrix cases below; inspect actual `InfiniteData` before and after to prove pagination metadata survives. Typecheck/lint pass. If current TanStack APIs cannot express the descriptor lifecycle without a second canonical store, stop and review a bounded alternative instead of adding an ad hoc state framework.

### 3. Centralize invalidation and completion handling

Add one narrowly scoped invalidation helper for mailbox, thread, shell, search, search-groups, and search-palette query families. Use it in mutation settlement, Undo, and corresponding existing daemon events. Include inactive cached views so revisiting one cannot show stale successful mutations. Avoid waiting on unrelated long-running analytics queries.

Deduplicate/coalesce bursts if the existing coordinator provides a clean place; correctness comes first and no benchmark claim is required. Reconciliation failure must refresh the affected view even when a toast was already shown. An unavailable Undo must stay an error, not emit “Undo applied.” Keep the existing partial-results checks and `silentSuccess` semantics. Remove reliance on nonexistent event variants only in this helper's affected path; do not redesign protocol events here.

**Verify:** event/search tests pass with background sync disabled and no synthetic `MailUpdated` event. A successful mutation and Undo each converge the open search and group counts. Failures show one actionable error; no false success toast.

### 4. Verify real bridge behavior and failure recovery

Extend browser tests against the actual FakeProvider bridge. Use a test request barrier to delay or fail a specific response, forwarding successful requests to the real bridge rather than hardcoding successful mutation payloads. Observe immediate local projection and query back through the bridge after settlement. For failure cases, distinguish “request never sent” from “server applied but response was lost”; the latter requires authoritative refresh and must not assert a rollback that the daemon disproves.

**Verify:** build and browser commands pass, then targeted unit tests/typecheck/lint pass. `git diff --check` exits 0. Hand the shared invalidation/result entry points and test names to Plan 022's owner; do not modify its dispatch consumers ahead of that plan.

## Test matrix and done criteria

| Case | Required outcome |
|---|---|
| Two infinite pages | Correct row projection; pages/pageParams preserved |
| Inbox plus cached All Mail | Archive leaves All Mail membership intact |
| Label remove/move/route | Only provably excluded lens membership changes |
| A fails, B succeeds; same/different IDs | B intent survives; final view matches daemon |
| Refetch during pending mutation | No transient undo of pending intent |
| Partial provider failure | Counts/error honest; final rows match actual successes |
| Lost response after server apply | Refetch recovers authoritative outcome |
| Search and grouped counts | Mutation and Undo refresh without later sync/navigation |
| Selection changed while pending | Failure does not overwrite newer selection |
| Empty resulting page | List and paging remain valid |

Done requires all targeted tests and browser scenarios to pass, no flat-only cache fixture as the sole optimistic coverage, and no success path relying on later sync to refresh search. Record actual commands/results. Plan 025 consumes these cases; Plan 009 A10/A11 are addressed only after this plan passes, not the rest of Plan 009.

## STOP conditions

Stop if a fix requires changing server mutation semantics, expanding selected thread IDs, evaluating search queries in the browser, adding unsafe retries, using a second canonical mail store, or touching out-of-scope paths. Stop after two failures of the same verification approach, missing named test collection, or unexplained code drift. Preserve any current unrelated user changes.
