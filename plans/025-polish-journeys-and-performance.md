# Plan 025: Verify the polished mail workflow at realistic scale

> Executor: read this file fully. Reuse the product's real daemon, fake provider, fixtures, and client tests. Record measurements and failed journeys before changing behavior. Update the index only after review and integration.
>
> Drift check: fetch `origin main` and record its SHA. Compare `git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..origin/main -- crates/daemon/tests crates/test-support crates/reader crates/tui/tests crates/mcp apps/web/e2e apps/web/scripts/e2e-server.mjs apps/web/playwright.config.ts benches/daemon_burst.rs .github/workflows/ci.yml .github/workflows/demo-e2e.yml`. Earlier plans will intentionally change some paths. Verify their recorded results and current contracts instead of restoring old behavior.

## Status

- Priority: P2
- Effort: L
- Risk: MED
- Depends on: 011 for baseline work; 012 through 024 for final acceptance
- Category: tests, perf, product polish
- Planned at: `origin/main`, `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- Execution: TODO. All commands below are future verification gates. No implementation tests or performance measurements were run while writing this plan.

## Why this matters

Passing unit tests does not establish that someone can set up an account, find a message, read it, reply, archive, and recover a mistake across clients. The existing demo and fake provider can make those journeys repeatable. Use them to close the audit's reading, responsiveness, keyboard, discoverability, and workflow-consistency recommendations without adding product features.

## Current state

`crates/daemon/tests/demo_cli.rs:32` already exercises the real binary with a substantial fixture:

```rust
const DEMO_MESSAGES: usize = 2_000;
```

It asserts streamed seed progress and uses a 30-second seed regression budget. That is an existing tripwire, not a general interaction latency target. Its `DemoTeardown` and `mxr_test_support::daemon` helpers isolate the instance and stop the owned daemon. Reuse the existing test setup; never point commands at the user's default profile.

`.github/workflows/demo-e2e.yml` is manual and defaults to 50,000 messages. It tests seed/readiness on Linux, not every client interaction. `benches/daemon_burst.rs:25` already sends 16 concurrent status/search requests while enqueuing semantic ingest, using a 128-message fixture. These checks answer different questions.

`apps/web/scripts/e2e-server.mjs:110` supplies:

```js
MXR_FAKE_DATASET: "demo",
MXR_FAKE_MESSAGE_COUNT: "120",
```

This server owns a disposable runtime and exposes stop/restart control endpoints. `apps/web/playwright.config.ts` runs one worker against it with `reuseExistingServer: false`. `apps/web/e2e/smoke.spec.ts` proves shell rendering and account presence; its assertions alone do not prove archive, undo, pagination, or recovery.

`crates/test-support/src/lib.rs` lists 12 standards fixtures, including `encoded-words.eml`, `folded-flowed.eml`, `nested-multipart.eml`, and `rfc2231-attachment.eml`. Existing reader tests are `crates/reader/tests/standards.rs` and `integration.rs`; TUI rendering tests include `crates/tui/tests/snapshots.rs` and `search_streaming_render.rs`. Extend these rather than creating another rendering stack.

Vault lessons: **Demo Data Is a Product Fixture** requires believable data through real IPC. **Keyboard Parity Is a Product Contract** includes finding an action and recovering from it. **Measure the Thing, Not the Measurer** requires probe overhead and machine/build context. **Zero Change Can Be Success** and **The Last Page Can Be Empty** require explicit completion evidence. **Hot Reads Should Not Repair Cold State** rules out making every search repair its index.

## Scope

Modify only these test/measurement locations and minimal fixture support:

- `crates/daemon/tests/cli_journey.rs`, `demo_cli.rs`, `stdio_server_cli.rs`
- `crates/test-support/src/lib.rs`, `daemon.rs`, and synthetic standards fixtures
- `crates/reader/tests/integration.rs`, `standards.rs`, and their snapshots
- `crates/tui/tests/snapshots.rs`, `search_streaming_render.rs`, `saved_search_tabs_render.rs`, and their snapshots
- Tests inside `crates/mcp/src/lib.rs`
- `apps/web/e2e/polish-journey.spec.ts`, create; existing e2e helpers if needed
- `apps/web/scripts/e2e-server.mjs`, `apps/web/playwright.config.ts`
- `benches/daemon_burst.rs`
- `.github/workflows/ci.yml`, `.github/workflows/demo-e2e.yml`
- `docs/verification/polish-journeys.md`, create a reproducible result ledger
- `plans/README.md`, status only

Behavioral corrections belong in the narrow owning plans 012–024, with their checks rerun. If a measured defect falls outside those scopes, record a bounded follow-up specifying its observed failure, exact files and acceptance test before changing code. This plan is not complete until release-blocking failures in the existing workflow are resolved or explicitly accepted by the operator. Do not silently convert failures into deferred items.

No analytics service, telemetry, benchmark product UI, new mail features, UI redesign, mail provider, or mailbox architecture rewrite. Use synthetic `example.com` identities. Never copy private mail or personal vault examples into fixtures.

## Steps

### 1. Establish the baseline and expected outcomes

After 011, record SHA, OS/CPU/memory, Rust and Node 24 versions, build features/profile, fixture seed/count, semantic enabled/disabled state, and each command/result in the ledger. Use the same settings for before/after comparison. The supplied fixture must include multiple accounts, long threads, attachments, HTML newsletters, Unicode, quotes, empty folders and more than one saved-search page.

Write a compact journey matrix with the exact expected IDs/counts, action and inverse, account scope, client coverage and owning plan. Use synthetic labels `Review` and `Project`: completing a queue item removes `Review` and preserves `Project`; archive removes Inbox membership. These are different operations. Preview and undo must retain that distinction. Include mailbox growth between preview and confirmation to prove no silent widening.

Verify: `scripts/cargo-test -p mxr --test cli_journey --test demo_cli` exits 0 after 011. Existing failures get recorded, with captured errors. Do not turn baseline failures into new expected snapshots.

### 2. Exercise the real workflow through the existing clients

Add deterministic scenarios to the existing test locations. Share fixture IDs and expected daemon results; keep client interaction tests native to each client. Require parity only where that action already exists, and label unsupported actions explicitly.

| Journey | Required observable result | Owning plans |
|---|---|---|
| Empty profile, failed auth, explicit repair | A truthful next step; background checks do not repeatedly prompt; other accounts remain usable | 012 |
| Find, page, open, back | All matching IDs are reachable once; route, focus and query survive returning | 018, 019, 021 |
| Read rich/quoted/Unicode mail and attachment names | Useful plain text, visible links, intact Unicode and headers; no terminal-control interpretation | Existing reader contracts |
| Edit, fail validation, interrupt, reopen | Unsaved text remains recoverable; no silent replacement of the saved draft | 013 |
| Send and lose the provider response | Visible unresolved outcome and preserved draft/receipt; no automatic duplicate transmission | 014 |
| Preview queue removal/archive, execute, undo | Identical resolved action and target set; invalid undo cannot claim success | 020, 022 |
| Restart during batch mutation | Truthful interrupted status and persisted progress; no unsafe replay | 023 |
| Resync with deletion and empty final page | Stale IDs removed only after authoritative completion; completion works with zero changes | 015 |
| Read/reply-later/rethread during hydration | SQLite and search agree, including messages after position 10,000 | 016, 017 |
| Agent discovery followed by restricted operation | Advertised effective policy agrees with enforcement and account scope | 024 |

Reuse the web server's owned stop/restart controls. Add `polish-journey.spec.ts` to the normal smoke command once stable. Ensure setup waits for an explicit fixture-ready condition when a test needs completeness; first nonempty page is insufficient. Make message count configurable for a separate scale run while retaining a small default. Avoid fixed sleeps; wait for visible states or daemon completion events with useful failure output.

Verify: under Node 24, `npm ci`, `npm run typecheck`, and `npm run e2e -- e2e/smoke.spec.ts e2e/polish-journey.spec.ts` in `apps/web` exit 0 with the built fake-provider daemon. Do not run two copies against the same test runtime/ports. Verify `scripts/cargo-test -p mxr --test cli_journey --test stdio_server_cli`, `scripts/cargo-test -p mxr-mcp --tests`, and `scripts/cargo-test -p mxr-tui --tests` exit 0. Assert stdout remains parseable JSON/JSONL where supported; progress and diagnostics must not corrupt piped data.

### 3. Review reading and keyboard behavior with meaningful assertions

Use the standards fixtures to assert decoded content, quote/signature separation, link destinations, attachment names and truncation boundaries. Snapshot representative TUI widths and web viewports, including an empty state and long subject. Review changed snapshots; an updated snapshot is not proof of correctness. Avoid inventing stricter MIME semantics than the shared reader/provider contracts.

In the browser, drive the journey with Tab, Shift-Tab, Enter, Space and Escape. Assert focus returns to a useful row/control after modal close, archive, undo and back navigation. Nested buttons must perform their own action without opening the thread. Visible and keyboard actions must share the same confirmation policy. Use existing accessibility tooling where it adds a concrete assertion; automated scans do not replace keyboard interaction.

Verify: `scripts/cargo-test -p mxr-reader --tests`, the focused TUI tests above, and `npm run e2e -- e2e/polish-journey.spec.ts` all exit 0. Record reviewed screenshots/snapshots in the ledger. Any rendering correction gets a targeted failure test and a small owning change, not a blanket snapshot rewrite.

### 4. Measure responsiveness during the 50,000-message workflow

Run the existing manual Linux demo workflow at its shipping-scale input once execution is authorized. Record its URL, inputs and artifacts. Extend that existing job with post-ready status/search/open checks and a restart check using its isolated profile. Keep its seed/readiness budget separate from interaction measurements. Do not make the expensive run mandatory on every edit.

Extend `daemon_burst` only enough to compare foreground requests with idle and active sync/ingest workloads. Validate every response's correctness before recording timing. Record sample count, median/tail latency, CPU/RSS where measured, time to first usable page and remaining background work. Separate cold and warm caches, debug and release, semantic on and off. Do not label 128-message results as 50,000-message evidence. Measure observer overhead with probes on/off and repeat paired runs; use existing Criterion output before writing custom statistics.

Verify: `cargo bench --bench daemon_burst` exits 0 and produces measurements with valid responses. The manual demo run must pass its existing completion assertions and the new journey checks. Set any new regression threshold from repeated baseline measurements on the named runner, record the rationale and tolerated noise, and verify the final change against that threshold. Do not invent a universal latency target from this planning session. A slow/failing interaction remains an open owning-plan item until corrected and remeasured.

### 5. Close the execution ledger

For each scenario, record baseline failure or baseline pass, owning change, final result and evidence location. Record each plan as implemented, reviewed, integrated and verified separately. Run `cargo build -p mxr`, applicable focused tests above, and the normal web checks. Rerun only checks affected by changes since their last successful run. `git diff --check` must exit 0.

Verify the ledger contains no unresolved release-blocking journey, no missing plan result, and no claims based only on a mocked client cache or a worker's completion message. Record an explicit operator decision for any accepted limitation. Do not claim provider live tests ran when only the fake provider was used.

## Done criteria

- [ ] Every audit finding has an owning plan and regression evidence.
- [ ] Core existing journeys pass across the applicable CLI, TUI, web and MCP clients.
- [ ] Reader fixtures and actual keyboard interactions pass; snapshots were reviewed.
- [ ] 50,000-message readiness and representative foreground interactions pass on the documented environment.
- [ ] Before/after measurements include sample count, build/fixture context and probe overhead.
- [ ] Observed in-scope failures are fixed and reverified; any accepted limitation has an explicit decision.
- [ ] Build and focused checks pass; no real mail, secrets or telemetry enter artifacts.

## STOP conditions

Stop and report if test isolation could touch the user's account/daemon, fixtures require live credentials, a repeated failure cannot be reproduced with captured evidence, or passing a gate would require widening product scope. Do not change standard environment variables such as `HOME` in new manual recipes. Reuse established test isolation and application-specific `MXR_*` paths. Stop a run only through the child/instance that run owns.

## Maintenance notes

Branch: `codex/025-polish-journeys-and-performance`. Load `.agents/skills/mxr-development/SKILL.md`; use the repository's email standards skill for MIME fixture changes. Conventional commits, no attribution footers. No push, PR or workflow dispatch without authorization. Keep the fixture version and expected result matrix together as the product evolves. Treat local benchmarks as local evidence; a passing fake-provider journey cannot prove a remote service's availability.
