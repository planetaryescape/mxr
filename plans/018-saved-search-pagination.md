# Plan 018: Make saved-search results fully pageable

> Execute in order and run each verification gate. This file is an implementation plan, not a record of completed changes or passing tests. The coordinator owns `plans/README.md` while this batch is dispatched.

## Status

- **Priority:** P2
- **Effort:** M–L
- **Risk:** MED — protocol compatibility, page boundaries, and thread grouping
- **Depends on:** `011-safe-verification-and-ci.md`
- **Category:** bug
- **Planned at:** fetched `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- **Consumers:** Plan 019 search coverage and Plan 025 journey verification

## Why this matters

Saved searches are a primary mailbox navigation tool. The web client currently stops after its initial 200-message page, and `mxr saved run` always requests 50 results. Repair paging through the existing daemon capability so users can reach every match without silently changing the query, account scope, or search mode.

## Current state

All excerpts below were read at the SHA above.

`crates/protocol/src/types.rs:772` has no saved-search offset:

```rust
RunSavedSearch {
    name: String,
    limit: u32,
    #[serde(default)]
    account_id: Option<AccountId>,
},
```

`crates/daemon/src/handler/diagnostics/mod.rs:1137` delegates to the ordinary execution engine, but fixes offset to zero and uses the current saved-run ordering:

```rust
let execution = execute_search(
    state,
    &saved.query,
    limit as usize,
    0,
    account_id.or(saved.account_id.as_ref()),
    saved.search_mode,
    SortOrder::DateDesc,
    false,
)
.await?;
```

Its response already contains `total`, `has_more`, and `next_offset`. `crates/web/src/envelope_list.rs:85` discards them with `ResponseData::SearchResults { results, .. }`; `chrome.rs:444` receives only envelopes. `crates/web/src/lib.rs:170` consequently excludes saved searches from pagination.

`apps/web/src/features/mailbox/useMailboxQuery.ts:77` uses `useInfiniteQuery`, but its `getNextPageParam` returns `undefined` for every saved search at line 86. Its `mergeMailboxPages` deduplicates `row.id`; bridge `mailbox_thread_rows` groups envelopes within each page, so one thread may have different representative message IDs on different pages.

`crates/daemon/src/commands/saved.rs:104` always requests `limit: 50`. Existing JSON output is a results array; preserve that shape. `SavedAction::Run` in `crates/daemon/src/cli/mod.rs:1739` has only a name. The HTTP saved-run body in `crates/web/src/routes_v6.rs:579` has name and limit. The TUI already opens saved searches through ordinary search: `app/sidebar_helpers.rs:249` creates `Action::SelectSavedSearch(search.query, search.search_mode)`; `app/search_helpers.rs:64` applies actual `has_more`.

Project contract: SQLite is canonical, the daemon owns reusable behavior, and clients share IPC. `.agents/skills/mxr-development/SKILL.md` says, “Read/list/status/search/export surfaces must keep structured output pipeable.” Do not put provider queries or query-language execution in React.

## Lessons carried forward

- **The Last Page Can Be Empty:** termination comes from pagination metadata, not whether the last payload contained rows.
- **Parser Parity Is Not Execution Parity:** test saved-query execution against the ordinary daemon search over the same data; accepting an offset argument alone proves nothing.
- **Same-Code-Path Preview:** paging must not widen the selection used by a later mutation. Select-all remains the existing loaded-message selection, not all matches.

## Scope

Modify only:

- `crates/protocol/src/types.rs` and `crates/protocol/src/lib.rs` for existing request constructors/category/serialization tests
- `crates/daemon/src/handler/mod.rs`, `handler/platform.rs`, `handler/diagnostics/mod.rs`, `handler/diagnostics/search_execute.rs`
- `crates/daemon/src/handler/tests/routing_and_search.rs`
- `crates/daemon/src/commands/saved.rs`, `crates/daemon/src/cli/mod.rs`; `crates/daemon/src/lib.rs` only if its existing `Command::Saved` dispatch needs a signature update
- `crates/daemon/tests/cli_journey.rs`, saved-run help snapshot under `crates/daemon/tests/snapshots/`
- `crates/web/src/envelope_list.rs`, `chrome.rs`, `lib.rs`, `routes_v6.rs`, `openapi.rs`; `crates/web/tests/integration.rs`
- `apps/web/src/features/mailbox/useMailboxQuery.ts` and a new `useMailboxQuery.test.tsx`; `features/mailbox/types.ts` only for metadata already returned by the bridge
- `apps/web/src/api/generated.ts` through generation only
- New `apps/web/e2e/saved-search-pagination.spec.ts`; `apps/web/scripts/e2e-server.mjs` only to make the existing FakeProvider message count configurable for this fixture
- `site/src/content/docs/guides/labels-and-saved-searches.md` for changed saved-run arguments; follow the existing help snapshot generation for CLI reference output

Out of scope: changing saved query syntax, fixing stored-sort semantics, widening account scope, changing default CLI output arrays, a new saved-search model, general search engine rewrites, or making a row action affect a whole thread instead of its current explicit message IDs. Plan 019 owns broader search coverage reporting. Do not mark Plan 009 complete.

## Commands and environment

Run from a disposable implementation worktree, not the user's active checkout. Use Node 24 as CI does; `node --version` must report `v24.*`. `npm ci --prefix apps/web` installs the locked web dependencies. Use the repaired `scripts/cargo-test` only after Plan 011 passes; never use its old process-killing implementation. Cargo package `mxr` is defined by root `Cargo.toml`; there is no `crates/daemon/Cargo.toml`.

| Purpose | Command | Success |
|---|---|---|
| Inventory | `rg -n 'RunSavedSearch|SavedAction::Run|run_saved_search' crates` | All constructors/callers accounted for |
| Protocol | `scripts/cargo-test -p mxr-protocol --tests` | All pass |
| Saved-run handler | `scripts/cargo-test -p mxr --lib run_saved_search` | Existing and new named tests execute and pass |
| CLI journey | `scripts/cargo-test -p mxr --test cli_journey saved_search` | Isolated FakeProvider journeys pass |
| Help | `scripts/cargo-test -p mxr --test cli_help saved` | Reviewed snapshots pass |
| Bridge | `scripts/cargo-test -p mxr-web --tests` | All pass |
| Web unit | `npm --prefix apps/web test -- src/features/mailbox/useMailboxQuery.test.tsx` | Nonzero new tests pass |
| Generated API | `npm --prefix apps/web run gen:types` | Exit 0; generated diff reviewed |
| Web static | `npm --prefix apps/web run typecheck` and `npm --prefix apps/web run lint` | Exit 0 |
| Build | `cargo build -p mxr` | Exit 0 |
| Browser | From `apps/web`: `npm run e2e -- e2e/saved-search-pagination.spec.ts` | Pass against harness daemon |

These are future verification commands, not tests run while authoring this plan. Read `apps/web/scripts/e2e-server.mjs`: it creates isolated `.playwright` state and uses `target-cli/debug/mxr` by default. Check ports 5173, 17777, and 17778 are free; do not kill their owners. Use Playwright's installed Chromium or `npx playwright install chromium` in `apps/web` if absent. The fixture must not read the user's real mailbox.

## Git workflow and drift gate

Branch `codex/018-saved-search-pagination`. Fetch `origin main`, record its SHA, then run:

```sh
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/protocol crates/daemon/src crates/daemon/tests crates/web apps/web
git status --short
```

Compare the excerpts with fetched/current code and account for completed prerequisite changes. Never switch/reset/clean a working checkout. Use conventional commits such as `fix: page saved searches through the daemon`; no attribution footers. Do not push or publish without instruction.

## Steps

### 1. Lock the failing execution and paging cases

Extend `routing_and_search.rs` using its `dispatch_run_saved_search_returns_results` fixture pattern. Seed at least 205 uniquely dated matches, including a thread crossing a page boundary and some nonmatches. Exercise the real store/index/dispatcher. Add assertions for first/middle/last/empty-final pages, explicit versus saved account precedence, and mode preservation. For ties, use the ordering already guaranteed by ordinary search; do not invent a new sort in this plan.

Create `useMailboxQuery.test.tsx` using actual TanStack `QueryClientProvider` and the production hook, with MSW at the HTTP boundary. Assert that a saved-search response with `has_more: true, next_offset: 200` requests offset 200. A mock hook returning flattened groups is insufficient.

**Verify:** the handler and web unit commands above must expose the named pagination failure before implementation. If new protocol fields are needed to express the test, introduce only the additive field/signature plumbing with zero default before running the red test; keep the behavioral bug until the assertion fails. Record the failure, then proceed.

### 2. Thread offset through saved-search execution

Add `#[serde(default)] offset: u32` to `RunSavedSearch`; old serialized requests must continue to mean offset zero. Thread it through dispatch/platform/diagnostics and every constructor identified by inventory. Call the same `execute_search` with this offset. Keep account precedence, `saved.search_mode`, and existing DateDesc behavior. Do not fetch every preceding page into memory.

Expose optional `--limit`/`--offset` on `saved run`, preserving defaults 50/0 and existing JSON/JSONL/CSV/IDs result shapes. Show a continuation hint for human table output. For JSONL, page metadata may go to stderr following `commands/search.rs`; never interleave metadata with result records. Do not silently replace JSON arrays with envelopes. Add old-input serialization and CLI parsing/help coverage.

**Verify:** protocol tests, saved-run handler tests, CLI saved-search journeys, and help tests pass; no new test is filtered out. Parse CLI stdout as its documented format, with stderr captured separately.

### 3. Preserve bridge metadata and page real saved lenses

Return an explicit envelope-page value from the bridge saved-search helper, carrying `has_more`, `next_offset`, and total from the daemon. Propagate it through `MailboxSelection`/HTTP instead of deriving completeness from rendered thread count. Keep Inbox/All Mail/label behavior unchanged. Add optional offset to the existing saved-run HTTP body and update OpenAPI metadata/types.

Remove the web saved-search early stop. Prefer daemon continuation metadata; treat `has_more: false` as final even if the page is full, and stop cleanly when a final page is empty. Reject a nonadvancing next offset as an error instead of requesting the same page forever.

For thread view, reconcile a repeated thread across adjacent pages using thread identity while retaining the current representative message ID/selection meaning. Do not add page-local counts as if they were complete thread counts. The test must prove no duplicate visible conversation while message view can still enumerate every message.

**Verify:** bridge tests and production-hook unit tests pass, including exact-page-size completion and boundary-thread cases. `npm --prefix apps/web run gen:types`, typecheck, and lint pass; inspect generated diff.

### 4. Prove completion through the running clients

Add a browser test using the real bridge/daemon/FakeProvider. Let the harness opt into a fixture message count above 200 without altering its normal 120 default. Create the saved search using the bridge, open its lens, scroll through every page, and compare unique results with an ordinary search of the same query. Do not use real provider accounts or replace the API with hardcoded rows for this test.

**Verify:** build and browser commands pass. Rerun CLI saved-search journeys. `git diff --check` exits 0 and `git status --short` contains only approved paths and generated runtime artifacts excluded from commits.

## Test matrix and done criteria

| Case | Required observation |
|---|---|
| Zero matches | Empty state, no follow-up loop |
| 199/200/201/400/405 matches | Every expected result reachable; termination follows metadata |
| Empty terminal page | Loading clears, previously loaded results remain |
| Thread crosses pages | One visible conversation; message IDs are not silently expanded for mutation |
| Multiple accounts | Saved scope retained; explicit override has current precedence |
| Lexical/semantic/hybrid | Existing execution mode/fallback behavior preserved |
| Old serialized request | Deserializes with offset zero |
| CLI formats | Existing stdout shape remains parseable; paging hints do not corrupt records |
| New mail while viewing | Refresh starts from a coherent page sequence; no endless duplicate-page fetch |

Done requires every command above to pass with nonzero relevant tests, no unreviewed snapshot/type generation changes, and completion across both message and thread lenses. Supply Plan 019/025 owners with fixture names and actual paging contract. Update only this plan's status through the coordinator.

## STOP conditions and maintenance

Stop and report if completing paging requires changing query meaning, stored sort behavior, selection from messages to threads, default JSON shape, or account scope. Report a search-engine limit instead of increasing it blindly. Stop after the same verification failure twice, an unexplained fetched-code mismatch, a missing expected test target, or required out-of-scope changes. Keep this work separate from legacy Plan 009's other UX findings; it addresses saved-search completeness and the relevant page-boundary issue only.
