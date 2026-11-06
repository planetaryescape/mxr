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


## Daemon recovery preserves the original profile selectors

Both CI test profiles on `7d2bcf16060af61a94dea6b993ffb82cdddd2629` failed `daemon_lifecycle::status_recovers_running_daemon_when_socket_path_disappears` in run `42155904933`. Logs are `/tmp/mxr906-7d2-fast.log` and `/tmp/mxr906-7d2-semantic.log`. The child launcher injected explicit instance/socket defaults while its parent left those selectors absent. Recovery compares raw profile selectors exactly, so it refused to identify its own daemon after the socket and PID discovery file disappeared. Process ownership checks remain unchanged.

The existing failure reproduced locally in `/tmp/mxr-pool3-missing-socket-repro-20261106.log`. A new real-process regression with no instance override and an explicit socket also failed before the fix, retained in `/tmp/mxr-pool3-default-instance-recovery-red-20261106.log`. CLI spawning now inherits the original selectors. TUI spawning does the same when its selected socket matches the environment, retaining an explicit socket override only for a different target.

All seven `daemon_lifecycle::` integration tests passed in 56.77 seconds after a 1m16s compile, including both recovery cases. Retained log: `/tmp/mxr-pool3-daemon-lifecycle-20261106.log`, exact workflow label `mxr-backlog-pass2:d46d841c6ff34b71bb191a5d9db44f59`, exit 0. The build compiled both CLI and TUI changes. Formatting and diff checks passed. Scoped root/TUI clippy (`cargo clippy -p mxr -p mxr-tui --all-targets --locked -- -D warnings`) passed in 3m36s, retained `/tmp/mxr-pool3-recovery-clippy-20261106.log`, exact workflow label `mxr-backlog-pass2:f06b6118496e4134b648a3d5e8ab29e6`, exit 0. Exact next action: commit, fresh-fetch/rebase, push PR906 and handle its full CI results. No owned heavy command remains. No duplicate job, full local workspace suite or browser acceptance rerun is needed for this selector correction. Full ready-PR CI remains required.


## PWA integration awaits the owner's offline receipt

The friendly-domain/PWA owner supplied local branch `fix/iphone-token-and-input-zoom`, head `00cac3bd9d991b51aa3f61d5dede73e343afbf5f`, directly based on `3683eb6b1131a5486e6b4133068ae848a2b1f835`. Local Git objects confirm that ancestry and the new `src/pwa/` worker/read-cache implementation. `offlineReads.ts` derives its cache name from `EXPECTED_IPC_PROTOCOL_VERSION`, preserving this candidate's IPC 29 namespace when integrated. These commits have not been integrated into PR906.

Preserve the earlier manual-token validation and touch/input changes when combining `TokenSection.tsx`; all form hooks must precede the hosted-session early return. Preserve the PWA owner's additive changes to API error handling, offline banners, query network mode, private-cache invalidation, availability and rejected-session handling. No Rust/schema/IPC change is supplied by this PWA slice. Keep the existing 12-hour browser-session lifetime; the owner's question about 30-day trusted sessions remains unanswered.

The owner reports passing typecheck, 58 focused tests plus six follow-ups, five-thread lint, 173 precache entries and empty React Doctor diagnostics, with logs `/tmp/mxr-pwa-validation-20261106T0336.log` and `/tmp/mxr-pwa-validation-20261106T0346.log`. These are owner-reported receipts, not independently rerun checks. Their actual offline/reconnect fault-check receipt is pending against frontend-only deployment on daemon 0.6.223; installed Home Screen cookie authority still depends on releasing the compatible gateway. Next PWA action: consume that receipt, then integrate both commits while preserving the agreed shared-file cases. No additional build or review was requested for this coordination update.


## Supervised incompatibility errors retain protocol versions

Fast-profile CI on `d32ad3f539338b7dec1335fc02de715daf789515` reached `selected_daemon_diagnostics::serve_old_local_daemon_reports_upgrade_before_new_request_or_remote_override` and failed its error-detail assertion. Run `42158997640`, job `127699967237`, retained `/tmp/mxr906-d32ad-fast.log`, reports 2497 passed and one failed before fail-fast; it is not a completed green suite. The old daemon received only GetStatus and was rejected, but the supervised connection returned generic incompatibility text instead of the expected required/observed IPC versions and upgrade guidance.

The supervised client now distinguishes protocol mismatch from version/build mismatch in its diagnostic while preserving the same rejection and request boundary. Its existing older/newer protocol regression also requires both protocol values and service-manager guidance. All nine selected-daemon diagnostics integration tests passed in 5.03 seconds after a 2m56s compile; `/tmp/mxr-pool3-selected-diagnostics-20261106.log`, label `mxr-backlog-pass2:f954262260d14ae0a3c8d86afb8c458e`, exit 0. All four supervised-client tests passed in 0.06 seconds after a 29.63s compile; `/tmp/mxr-pool3-supervised-client-20261106.log`, label `mxr-backlog-pass2:8337c06c9b3c4bbba1db2aa419c1049a`, exit 0. Formatting and diff checks passed. Client-only all-target clippy passed with warnings denied in 59.03 seconds; `/tmp/mxr-pool3-client-diagnostic-clippy-20261106.log`, label `mxr-backlog-pass2:5c9553728d7b4bcc864a6dc14aad6405`, exit 0. Exact next action: commit/fresh-fetch/rebase/push PR906, then handle full CI. No owned heavy command remains. No live daemon, hosting state or remote target is changed by these isolated tests.


## Schema 110 can be replayed before its version stamp exists

On `051255ba298ddbabc45c89ad688d194bfe2897b6`, fast-profile CI passed. Semantic-profile run `42160372902`, job `127704436309`, failed `pool::tests::pre_versioning_db_is_backfilled_into_schema_migrations`: SQLite reported `table remote_browser_codes already exists`. Retained log `/tmp/mxr906-051255-semantic.log` reports 4836 passed and one failed before fail-fast; it is not a completed green semantic suite.

`apply_migration` records the version after applying the SQL and explicitly requires SQL migrations to be rerunnable. Migration 110 omitted the existing repository convention of `IF NOT EXISTS` on its two tables and two indexes. Those four declarations now follow that convention without dropping or rewriting browser-session data. The existing pre-versioning regression exercises this replay after removing version stamps from a fully migrated database; no new test duplicating the SQL was added.

All 20 store pool/migration tests passed in 69.09 seconds after a 46.37s compile, including pre-versioning backfill and repeated migration. Retained `/tmp/mxr-pool3-migration-tests-20261106.log`, exact workflow label `mxr-backlog-pass2:1dea4451f46740a3b303f9f088a5b7ae`, exit 0. Diff checks passed. Exact next action: commit/fresh-fetch/rebase/push PR906, then handle full CI. No owned heavy command remains. PWA source remains separate while its owner finishes the offline-banner correction; no PWA source or live service changes accompany this migration correction.
