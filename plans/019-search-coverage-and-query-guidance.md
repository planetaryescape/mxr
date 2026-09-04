# Plan 019: Explain search coverage and exact query semantics

> Execute after the prerequisites in a clean worktree. Commands in this plan are **future executor commands, not verification performed during authoring**. Confirm each gate; do not add mailbox scans or repair work to search. The reviewer may maintain `plans/README.md`; otherwise update only this plan's row.

## Status

- Priority: P2
- Effort: L
- Risk: MED. additive response metadata must survive CLI/TUI/web/MCP pagination without disclosing excluded accounts.
- Depends on: `plans/011-safe-verification-and-ci.md`, `plans/015-authoritative-gmail-resync.md`, `plans/016-search-metadata-consistency.md`, `plans/017-complete-threading-and-index-publication.md`, and `plans/018-saved-search-pagination.md`. Do not execute against their pre-fix contracts.
- Category: dx, bug
- Planned at: fetched `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04.
- Evidence: code inspection; no search/provider experiment or test run during plan authoring.

## Why this matters

An empty local result does not prove that no matching email exists remotely. During initial sync, recovery, or stale indexing, clients currently have no response field explaining the difference. Familiar `from:alice` syntax is also an exact address filter in mxr, which can produce a surprising empty result. Explain these existing boundaries without broadening mutation selection or changing the parser.

## Current state

`crates/protocol/src/types.rs:2042` defines:

```rust
SearchResults {
    results: Vec<SearchResultItem>,
    #[serde(default)]
    total: u32,
    #[serde(default)]
    has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_offset: Option<u32>,
    explain: Option<SearchExplain>,
},
```

- `crates/daemon/src/handler/diagnostics/mod.rs:129` constructs this response from `execute_search`; that execution already tracks the executed mode. Plans 016–018 may evolve these fields and must be read first.
- `crates/daemon/src/commands/search.rs:110` emits results/paging/explanation; table output says `No results found.`
- `crates/daemon/src/handler/status_helpers.rs` already derives account sync phase/cursor/progress and lexical health. `collect_doctor_report` also scans/counts and inspects disk: **do not call that full report from search**.
- `crates/search/src/query_builder.rs:221` constructs exact lowercase term queries for `From`, `To`, `Cc`, `Bcc`, and `DeliveredTo`; the mapping points `From` at email, not display name.
- `crates/tui/src/search_ipc.rs:49` reads `results, has_more, ..` and currently discards other metadata. `apps/web/src/features/search/SearchResultsRoute.tsx` uses infinite queries with 100-result pages and has existing help/empty-state affordances.
- `crates/daemon/src/handler/tests/routing_and_search.rs`, protocol serde tests, and `apps/web/src/features/search/SearchResultsRoute.test.tsx` are the structural exemplars for daemon results, backward compatibility, and visible search behavior.

## Contract and lessons

- **Parser Parity Is Not Execution Parity:** preserve the parser/AST/backend boundaries. Do not turn an address hint into a substring query or claim Gmail execution parity.
- **Hot Reads Should Not Repair Cold State:** search must not rebuild indexes, scan bodies, reconcile provider state, or invoke an LLM to explain coverage. Derive metadata from existing bounded runtime/status state. Missing evidence is `unknown`.
- **First Run Is the Launch Surface:** distinguish no matching local rows from a still-incomplete initial mailbox.
- [Thunderbird documents body availability and search coverage separately](https://support.mozilla.org/en-US/kb/imap-synchronization); [Shortwave explicitly documents substring sender matching](https://www.shortwave.com/docs/references/search/). These are reasons to state mxr's own contract, not to copy another query implementation.

Target an additive `SearchCoverage` value shared by direct and saved searches. It should identify the observed time, selected-account sync state, lexical freshness, and a conservative completeness state (`complete_local_snapshot`, `partial`, or `unknown`). A complete local snapshot does **not** assert provider-global completeness forever: carry last successful sync and explain the time boundary. Reuse existing mode/fallback/total metadata from 016–018 instead of inventing competing booleans.

For initial/rescan completion, use the authoritative state established by 015. Neither `idle` alone nor `local_count == index_count` proves full remote history. If no trustworthy completion marker exists, return `unknown`; do not add a speculative history-complete heuristic.

## Scope and drift

Behavior changes are limited to:

```text
crates/protocol/src/types.rs
crates/protocol/src/lib.rs
crates/daemon/src/handler/diagnostics/mod.rs
crates/daemon/src/handler/status_helpers.rs
crates/daemon/src/handler/tests/routing_and_search.rs
crates/daemon/src/commands/search.rs
crates/daemon/src/commands/saved.rs
crates/daemon/tests/cli_journey.rs
crates/tui/src/search_ipc.rs
crates/tui/src/async_result.rs
crates/tui/src/app/mod.rs
crates/tui/src/app/search_helpers.rs
crates/tui/src/ui/search_page.rs
crates/tui/src/runner/tests.rs
crates/web/src/lib.rs
crates/web/src/tests.rs
crates/mcp/src/lib.rs
apps/web/src/api/generated.ts (generated only)
apps/web/src/features/search/api.ts
apps/web/src/features/search/SearchResultsRoute.tsx
apps/web/src/features/search/SearchResultsRoute.test.tsx
apps/web/src/lib/searchSyntax.ts
site/src/content/docs/guides/search.md
plans/README.md
```

Only mechanical field forwarding/default updates are allowed in these additional known response consumers: `crates/daemon/src/handler/diagnostics/search_execute.rs`, `crates/daemon/src/handler/triage.rs`, `crates/daemon/src/commands/selection.rs`, `crates/daemon/src/handler/tests/mutations_and_delivery.rs`, `crates/tui/src/client.rs`, `crates/tui/src/runner/tests/mutations_and_bulk.rs`, and `crates/web/src/envelope_list.rs`. Do not change their selection/mutation behavior. Regenerate `apps/web/src/api/generated.ts` through the existing `npm run gen:types` command when the bridge schema changes; do not hand-edit generated definitions.

Run `git fetch origin main` and `git rev-parse HEAD origin/main`. In a worktree containing prerequisites, run:

```bash
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- crates/protocol/src/types.rs crates/protocol/src/lib.rs crates/daemon/src/handler/diagnostics/mod.rs crates/daemon/src/handler/diagnostics/search_execute.rs crates/daemon/src/handler/status_helpers.rs crates/daemon/src/handler/triage.rs crates/daemon/src/handler/tests/routing_and_search.rs crates/daemon/src/handler/tests/mutations_and_delivery.rs crates/daemon/src/commands/search.rs crates/daemon/src/commands/saved.rs crates/daemon/src/commands/selection.rs crates/daemon/tests/cli_journey.rs crates/tui/src/search_ipc.rs crates/tui/src/async_result.rs crates/tui/src/app/mod.rs crates/tui/src/app/search_helpers.rs crates/tui/src/ui/search_page.rs crates/tui/src/client.rs crates/tui/src/runner/tests.rs crates/tui/src/runner/tests/mutations_and_bulk.rs crates/web/src/lib.rs crates/web/src/tests.rs crates/web/src/envelope_list.rs crates/mcp/src/lib.rs apps/web/src/api/generated.ts apps/web/src/features/search/api.ts apps/web/src/features/search/SearchResultsRoute.tsx apps/web/src/features/search/SearchResultsRoute.test.tsx apps/web/src/lib/searchSyntax.ts site/src/content/docs/guides/search.md
```

Reconcile the expected prerequisite changes; stop on unrecognized changes to selection, initial-sync ownership, or response meaning. Branch `codex/019-search-coverage-and-query-guidance`. No commit/push/PR unless requested; authorized subject `fix: expose search coverage and exact address guidance`, user author, no attribution footer.

## Steps and gates

### 1. Define metadata using the prerequisite contracts

Read plans 015–018 and their landed code. Add the smallest shared optional coverage field to the daemon's `SearchResults`, including saved-search execution through that response. Preserve the CLI saved-search JSON array established by plan 018; daemon metadata does not authorize changing that public array to an object. Serde omission from an old daemon must decode as unknown, never complete. Define stable reason codes for initial sync, recovery, stale/unavailable lexical index, provider error, and unknown state only when those distinctions are actually known. Keep account identifiers filtered to the request's effective allowed scope. No credentials, provider cursors, query content, or full bodies in coverage data.

Add `search_coverage` serde/daemon tests for missing legacy metadata, selected account A while B is incomplete, initial paging, interrupted rescan, successful completion, stale index, optional semantic disabled, fallback mode, and unknown state.

**Verify:** `scripts/cargo-test -p mxr-protocol --tests search_coverage` and `scripts/cargo-test -p mxr --lib search_coverage` each run tests and pass; do not accept a zero-test run.

### 2. Attach one bounded snapshot to each result page

At the existing daemon search completion path, derive coverage from request scope plus already tracked sync/lexical state. Factor only the small pure mapping out of `status_helpers`; do not call `collect_doctor_report`, `count_messages`, a provider, or a repair routine on search. Apply the same metadata production to saved searches after plan 018. Metadata failures must not turn known coverage into success; return unknown and retain readable local results unless the actual search itself failed.

Test that a fake provider's call counter remains zero during local search, a read does not mutate sync/index state, and excluded accounts are absent. Use synthetic state and a controllable clock where available; do not sleep for a real sync.

**Verify:** `scripts/cargo-test -p mxr --lib search_coverage` passes all result/scope/no-side-effect tests; `scripts/cargo-test -p mxr --lib routing_and_search` passes existing search behavior.

### 3. Carry state through the existing clients

Direct-search CLI JSON includes coverage additively; JSONL keeps message records on stdout and page metadata on stderr following current paging behavior. Saved-search CLI JSON remains an array; put its coverage beside paging hints on stderr. Table/ID output sends warnings where existing paging hints belong. Empty output says no matching **local** results when coverage is partial/unknown. Reuse metadata formatting where formats permit, without changing existing stdout shapes.

TUI must retain coverage through search segments and render one concise existing search-page status line. Web forwards the field through its bridge/search API and existing result header/empty state. Do not mark an accumulated set complete because one later page has better metadata; show the latest observed time/state and preserve any existing truncated-total warning from the prerequisites. Metadata does not alter result order, IDs, page offsets, or mutation selection.

Add a `DaemonRequester` mock assertion to existing MCP tests proving `mxr_search` returns the daemon's coverage intact and missing metadata remains unknown. Do not add an MCP search implementation or tool. Assert that saved CLI stdout is still an array and its coverage appears on stderr.

**Verify:** `scripts/cargo-test -p mxr --test cli_journey search_coverage`, `scripts/cargo-test -p mxr-tui --tests search_coverage`, `scripts/cargo-test -p mxr-web --tests search_coverage`, and `scripts/cargo-test -p mxr-mcp --tests search_coverage` pass with real rendering/bridge/pass-through assertions. In `apps/web`, run `npm run gen:types` if schema regeneration is required, then `npm test -- src/features/search/SearchResultsRoute.test.tsx` and `npm run typecheck`; all exit 0. Every filtered Rust command must execute new tests.

### 4. Explain exact addresses without changing query execution

Use the already parsed query/AST, not regex reinterpretation, to detect an incomplete `from:`/recipient address term where safe. Add a bounded guidance code/text such as `from: matches a full email address; use from:alice@example.com`. Do not execute a second search, enumerate all contacts, rewrite queries, or make exact address filters fuzzy. Use existing help content/empty state for the explanation. Valid full addresses must not trigger the hint. Keep quotes, multiple operators, and case handling consistent with `mail-query`.

Add `search_query_guidance` tests asserting identical returned IDs and mutation-preview selection with and without guidance, including `from:alice`, a full address, quoted display-name-looking values, and multiple filters. Update local semantics docs from verified outputs only.

**Verify:** `scripts/cargo-test -p mxr --lib search_query_guidance`, `scripts/cargo-test -p mxr-search --tests`, and `scripts/cargo-test -p mxr --test cli_journey` pass. Run `cargo build -p mxr` and `git diff --check`; both exit 0.

## Done criteria

- [ ] Direct/saved search responses distinguish partial/unknown coverage from a complete observed local snapshot.
- [ ] Legacy metadata absence, initial sync, interrupted recovery, stale index, and account scoping have behavior tests.
- [ ] Search never performs repair, full mailbox counting, provider requests, or LLM calls for coverage/guidance.
- [ ] CLI, TUI, bridge, web, and MCP show or preserve state without altering result IDs/order/paging or mutation selection; saved CLI JSON stays an array.
- [ ] Exact address guidance does not silently change backend semantics; valid complete addresses are unaffected.
- [ ] Focused tests, web checks, final build, diff check, and scope review pass; index/reviewer records outcome.

## STOP conditions and maintenance

Stop if prerequisites do not expose authoritative completion/freshness, scope filtering would reveal excluded accounts, metadata requires an unbounded read/repair, or a gate fails twice. When completion evidence is unavailable, `unknown` is a valid implementation result; document its reason instead of inventing certainty. Future providers must define what completion means before their state can report complete. Full responsiveness/load journeys belong to plan 025.
