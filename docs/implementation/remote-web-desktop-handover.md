# Remote browser acceptance passes; native installation remains separate

Continue `fix/remote-web-desktop-parity` in `/Users/bhekanik/code/planetaryescape/.worktrees/mxr-pool-3`. [PR 906](https://github.com/planetaryescape/mxr/pull/906) owns the candidate. BK asked to fix the hosted web and desktop gaps; the completion check is two isolated paired clients sharing one synthetic workspace through login, read/search, previewed triage, uploaded draft/send, restart/reconnect and parent revocation while the second client continues.

The candidate implements approved triage/AI grants, parent-bound browser sessions and assistant tools/history, portable inline images, desktop launch packets, leased presence, bounded drain, supervised hosting and selection/pairing/cleanup recovery. IPC is 29 and schema 110. [Runtime notes](remote-hosting-runtime.md) describe supervision; [the issue](../issues/remote-hosting-and-client-lifecycle.md) retains live installation and release gates.

## The real browser journey passed on 514389ba6

The accepted source is `514389ba61641ba902e749666bbab641029aac0a`, based on `origin/main` `f4ab9360a275f7917177c4fd5af7c91c557ce495`. `/tmp/mxr-pool3-final-build-20261106-r3.log` ended exit 0. The immutable binary `/tmp/mxr-remote-candidate-514389ba6/mxr` has SHA-256 `16e7a2dec98e61f51df4ff03cf326cd96ca1924b2f272f328fdf238bcb0d3055`.

Default browser-only acceptance i passed 57 recorded checks in 53.353 seconds, with exactly one fake-provider send and no diagnostic overrides. Receipt: `/tmp/mxr-remote-browser-20261106-acceptance-i/result.json`; retained output: `/tmp/mxr-remote-browser-20261106-acceptance-i.log`.

It verifies two real HTTPS pairings and independent cookie sessions, one-time codes and cookie replacement, redacted model status, SPA read/search, preview-equal archive, persistent draft/upload, exact sent attachment bytes, accepted-outcome continuity, live WebSocket reconnect after restart, Origin and administration boundaries, and revocation while the second client continues. The browser driver exited cleanly. Later source changes are mechanical Rust lint fixes, test fixture typing/mocks, generated website API output and documentation; no product behavior changed.

The fixture uses real CA-aware REST pairing with mode-0600 synthetic selected-state seeding, scoped CA trust for native WSS, and isolated Chrome contexts. It does not prove native CLI HTTPS pairing, `mxr web` issuance or the actual macOS WebView: the platform HTTP verifier rejects its temporary CA without OS trust changes. Native WSS accepted/unknown send recovery was observed in earlier run g; the browser-only final run does not repeat that path. Historical g/h receipts remain incomplete; h's fixture EOF failure was fixed before i. No failed receipt was relabelled.

## Scoped validation is recorded in retained logs

| Check | Outcome | Evidence |
| --- | --- | --- |
| Full scoped Rust hygiene | Passed fmt, touched-crate all-target/all-feature Clippy, architecture boundaries and cargo-deny policy | `/tmp/mxr-pool3-pre-pr-rust-20261106-r4.log` |
| Daemon/store/TUI regressions | 45 / 18 / 13 passed | `/tmp/mxr-pool3-final-scoped-rust-tests-20261106-r2.log` |
| Client/config/protocol libraries | 59 / 75 / 102 passed | `/tmp/mxr-pool3-final-client-config-protocol-20261106.log` |
| CLI help and examples | Six passed; snapshots regenerated | `/tmp/mxr-pool3-cli-help-20261106-r2.log` |
| Focused Rust web security | Six passed | `/tmp/mxr-pool3-web-focused-20261106.log` |
| LLM policy/cache and route panel Vitest | Six passed | `/tmp/mxr-pool3-llm-web-tests-20261106.log` |
| Mailbox connection Vitest | Six passed | `/tmp/mxr-pool3-mailbox-connection-tests-20261106.log` |
| Availability and hosted-token Vitest | Ten passed | `/tmp/mxr-pool3-final-availability-tests-20261106.log` |
| Web TypeScript | Passed | `/tmp/mxr-pool3-final-web-typecheck-20261106-r2.log` |
| Project web lint and motion/theme/shape checks | Passed | `/tmp/mxr-pool3-final-web-lint-20261106-r2.log` |
| API generation and SPA build | Passed | `/tmp/mxr-pool3-api-generation-20261106.log`, `/tmp/mxr-pool3-final-spa-build-20261106.log` |
| Native desktop | Compilation and 24 parser/navigation/generated-JavaScript harness checks passed earlier | `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --locked`; desktop worker's source harness result |

The generic anti-slop diagnostic exited 1 after project lint passed. It flagged module mocks that follow the existing Vitest convention, runtime event/JSON field validation, two test casts and a conditional object spread. The casts now use an explicit typed fixture and the spread is removed. Keep the established mocks and runtime validation; do not introduce dependency injection or a new schema library to satisfy the generic rules. The diagnostic is not reported as a passing check. The final rerun reports six findings: those two existing mock conventions and four runtime boundary checks. No rules were silenced.

The checked-in website OpenAPI document now comes from the same matching exporter as the browser's generated types and is deeply equal to that schema. Docs validation passed against 273 OpenAPI paths. It was regenerated through `site/scripts/openapi-artifact.mjs`, not patched by hand. CI on published `514389ba6` passed Secret Scan; its failures were the Clippy findings now corrected, that stale website artifact, and the mailbox mock now corrected. The other 1,983 web tests passed in that run; that is not a whole-suite pass. Raw logs: `/tmp/mxr-pr906-{openapi,web-tests,clippy}.log`.

## Finish the PR without changing live hosting

Follow-up `64f611122acd7354ebe4280592ec5108fddd0411` contains the mechanical Rust cleanup, typed test fixtures, corrected mailbox mock and regenerated website OpenAPI artifact. Final source checks passed before this documentation commit. A fresh fetch still resolved main to `f4ab9360a` with IPC 28.

Exact next action: push this branch, mark PR 906 ready, then handle its full CI and review outcomes. Inspect the PR's own diff and use the T3 PR watch for full CI. Second-opinion review is waived by BK; self-review and security regressions remain required.

No live supervisor, Tailscale mapping, system trust store, provider mailbox or migration changed. Actual desktop WebView with trusted TLS, live macOS/Linux installation, crash/reboot and personal mailbox migration remain separate acceptance work. No release, installed supervision or real-mail migration is claimed. The retained stash `3c1454c3f3a1806f4c2b8ca921d2a14694f64b56` is an older backup; the branch commits own later fixes. Do not drop unrelated stashes or touch the original checkout's unrelated untracked reports.

## Global admission has no remaining fairness hold

Pool-3 root is the sole launcher. `/tmp/mxr-heavy-admit.py` is a logging adapter around `/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-blueprint-gaps-20261021/tools/resume_heavy_gate.py`. The exact host advisory lock `/Users/bhekanik/code/planetaryescape/.orchestrate/host-heavy-admission.lock` spans GLOBAL Running + Queued < 3 and uniquely labelled enqueue visibility, then releases before waiting for completion. One command per owner, maximum five workers. Match label, cwd, source and time; Pueue IDs are reused. Retained logs establish outcomes.

The R3, quality #120 and doctor one-admission handoffs are all satisfied. Doctor actually started at `2026-11-06T02:12:02.429323Z`, label `mxr-backlog-pass2:6f8caed6bba1499e836da7d286f23b8e`, source `f5159008d807df9ca46fa5b3f94771aede81f831`. Normal canonical admission resumed immediately. All three pool-3 workers and all four active MXR backlog callers received the release reminder directly. Disregard stale yield messages; no later fixture/proof/root/merge hold, sentinel, queue configuration change or admitted-job cancellation applies.

## CI shutdown conformance follows the new notice contract

Both ready-PR test profiles failed on `59426e6af` at the same existing scenario: `serve::ipc_conformance::carriers::scenario_13_shutdown_closes_connections`. It expected immediate EOF; the observed first frame was the intended `ServerStopping` notice. Fast-profile evidence is `/tmp/mxr906-fast.log`; semantic-profile evidence is `/tmp/mxr906-semantic.log` (GitHub run `42152115409`). Four carriers failed before fail-fast stopped each profile, so neither suite completed.

The scenario now checks the notice's unsolicited envelope, current boot ID and exact shared drain deadline, then retains the strict clean-EOF assertion, rejecting duplicate notices and dangling/error frames. Production code is unchanged. Focused validation passed all five carriers in 0.77 seconds after a 7m10s compile, retained session 5985, `/tmp/mxr-pool3-shutdown-conformance-20261106.log`, exact workflow label `mxr-backlog-pass2:d89bea2062384fad9fb20a0cd7b6e380`. Exact next action: commit the test and receipt, fresh-fetch/rebase before pushing, then let full CI rerun. The PR remains watched; do not create a duplicate watch or retry the obsolete run.

Pool-3 sent its actual test-completion notice directly to R3 and copied Botroom root/UI. BK subsequently confirmed R3's separate World check started at 03:00:29Z and completed exit 0 with 60 passing tests. That ONE handoff is satisfied and normal admission resumed; no later R3 gate hold exists. All seven coordinated MXR callers received the release directly. Numeric task 1519 was reused; the R3 receipt is `.orchestrate/botroom-dead-ends-review/evidence/resume-20261105/world-unit-after-lounge.log`, not the earlier pool-3 command.

## The direct shutdown acknowledgment test also checks the notice

Ready-PR CI on `3cafd5e03` reached an adjacent stale assertion: `serve::tests::client_connection_acknowledges_shutdown_before_exiting` assumed the first frame was the acknowledgment. Both profiles instead observed the intended stopping event. Logs are `/tmp/mxr906-3cafd-fast.log` and `/tmp/mxr906-3cafd-semantic.log` (run `42154007511`). The prior five-carrier correction remains passing; neither full profile completed because of fail-fast.

The direct test now requires a valid current-boot/shared-deadline notice, the acknowledgment correlated to request 44, and strict clean EOF. This is test-only. Remaining shutdown assertions in direct handlers, the native requester, lifecycle integration and the transport corpus were inspected; they either bypass framing or already handle events before responses. All 16 named shutdown tests passed: `scripts/cargo-test -p mxr --lib shutdown --locked` (1.93 seconds after a 2m23s compile), retained session 3743 and `/tmp/mxr-pool3-all-shutdown-tests-20261106.log`, exact workflow label `mxr-backlog-pass2:197f8ca8dc8247e99cbdc601f5d348d3`. Theme142's ONE control handoff is satisfied: BK reported actual npm/Vitest start on frozen `1ad497729bdc8c13fa8c2e2d9cd017f4160ecc95`, receipt `/tmp/botroom-theme142-release/face-control-admission.json`. Its result was pending at admission; no pass is claimed. All seven coordinated MXR callers received the immediate resume notice; no later theme gate hold exists. The shutdown check completed under normal canonical admission; pool-3 has no remaining heavy command or waiter. BK subsequently reported the theme control finished 24/24 with exit 0; no later theme gate hold exists. All earlier handoffs remain cleared; this is the separate two-file face/theme control at frozen source1ad.

Exact next action after the 16-test result: commit this test-only correction and receipt, fresh-fetch/rebase, push PR906 and handle the full CI rerun. Runtime and the browser-accepted artifact remain unchanged. The earlier five-carrier-only rerun missed this adjacent acknowledgment assertion; the combined shutdown filter now covers both plus direct handler, requester and runtime-task paths.
