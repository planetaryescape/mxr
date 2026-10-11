# Each portability stage ships a usable workflow

Status: S01 is released in [v0.6.62](https://github.com/planetaryescape/mxr/releases/tag/v0.6.62), S08b in [v0.6.63](https://github.com/planetaryescape/mxr/releases/tag/v0.6.63), and S08a in [v0.6.64](https://github.com/planetaryescape/mxr/releases/tag/v0.6.64). S02 is integrated in [PR341](https://github.com/planetaryescape/mxr/pull/341) at `a631583c8fd8438abc6892fdf10fb49dc09e3748`, with exact-candidate CI complete. Claude Opus 5.5/max reviewed whole-diff candidate `010ae951` GREEN; root reviewed and accepted the documented trivial post-triage five-file delta to `3d338beb` under the explicit exception below. [Release PR344](https://github.com/planetaryescape/mxr/pull/344) merged as `7cfb0c38cf10b975eb76232a3e53ad6eb3fc2f54`, setting the manifest to 0.6.65. Its release artifacts are verified and S02 installed CLI and browser journeys pass. S03 PR347 merged as `d6ba93ba7082a644e027b331e67da44d8e6dd4e8` (tree `c66ddc36`); exact-candidate CI and root-triaged Opus review passed. S03 is released in [v0.6.68](https://github.com/planetaryescape/mxr/releases/tag/v0.6.68), with installed CLI, companion, TUI and browser acceptance complete. [Blueprint 23](../../blueprint/23-daemon-portability.md) owns requirements; this file owns delivery order. The program ledger is `/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-daemon-portability-20261010/ledger.md`.

The outcome is safer existing clients across disconnects, restarts and different daemon hosts, leaving a future native SwiftUI client and personal VPS deployment possible with one active daemon owner. The iOS app, Hostinger provisioning and public HTTP exposure remain outside this readiness run.

The initial fetched base was `origin/main` at `80606f4833b651bf9b07975ddd49083d7cb2568a`, also the source of [v0.6.61](https://github.com/planetaryescape/mxr/releases/tag/v0.6.61). Its [release workflow](https://github.com/planetaryescape/mxr/actions/runs/37952667487) completed successfully. S03 implementation was branched from `a928c63f9a0fd3c2c9e3e78fdbb157695ebf8b80`, after Updates PR343 advanced main to `8c8d069d81faf4603075c65afc5246dc33bf79fd` and release PR345 advanced it to `a928c63f`. The verified S03 release source is `621e9e49739eeb80abbccea0916578420941c3fc` (tree `6d391d9d90db51339d9a8d100e50915732a895eb`), after workflow fix PR357 and release metadata PR358. Keep S02 source pinned to tag commit `7cfb0c38cf10b975eb76232a3e53ad6eb3fc2f54` and tree `d8b8463eebbd1eaa4e5f47ed75d69b30cbadc531`; do not infer S02 behavior from later release metadata. Preserve the dirty primary checkout; implementation uses isolated worktrees.

## Ship workflows instead of blueprint components

Each row includes domain behavior, persistence where needed, client changes, CLI JSON/JSONL, error handling, schemas, relevant documentation and tests. P04 schema/compatibility work accompanies its consuming workflow; it is never a standalone foundation stage. The released artifact must demonstrate the promise without depending on undeployed later work.

| Stage | After release, the user can | Release-defining journey | Depends on | Blueprint |
| --- | --- | --- | --- | --- |
| S01 | Diagnose the daemon they actually selected | With distinct local and target profiles, run status and doctor over cmd://; target fields are truthful and target failure never touches the local store | Existing transports | P06a |
| S02 | Save and resume a draft without silent concurrent overwrites | Save, restart, reopen, then edit from two clients; preserve the stale editor's text while rejecting its stale revision | Existing compose contract; independent of S01 | P01 + P04 |
| S03 | Recover the outcome of sending a reviewed draft | Drop a send response; recover its stored receipt. Inject ambiguous provider acceptance; preserve uncertainty without automatic resend | S02 deployed | P01/P02 + P04 |
| S04 | Retry a previewed mailbox action safely | Lose a batch-action response; retrieve per-item results with the same operation ID and reject changed payload reuse | S03 deployed | P02 + P04 |
| S05 | Attach and download without shared host paths | From disjoint filesystems, upload, save, restart, preview/send, and download identical bytes on the client | S02/S03 deployed | P03 + P04 |
| S06 | Recover mailbox state after an event gap without losing edits | Drop only WebSocket traffic, mutate through CLI, reconnect and refresh; switch workspace/recovery generation without replaying pending commands | Relevant S02-S05 contracts deployed | P05 + P04 |
| S07 | Use the same selected daemon from CLI and TUI | Read, reply through a local editor, attach, preview and mutate over the existing remote connector; prove compatible/incompatible versions | S03-S06 deployed | P06b + P04 |
| S08a | Stop and restart the daemon under supervision | Send SIGTERM and SIGINT to an isolated daemon; observe orderly teardown, restart and query acknowledged state | Independent; interrupted-action recovery belongs to S08c after S03/S04 | P07a |
| S08b | Retain a complete disk token cache if replacement is interrupted | Inject interruption before/after atomic replacement with synthetic tokens; reload the complete old or new cache | Independent | P07b |
| S08c | Run unattended with useful dependency diagnostics | Start isolated Linux service without GUI secrets access, interrupt/restart and remove optional model service; inspect mail readiness | S08a/S08b and S03/S04 deployed | P07c |
| S09 | Restore useful state without accidentally executing old work | Inspect/preview a secret-free backup, restore into another profile, recover drafts/bytes/search, and verify effectful work remains paused | Durable resource and lifecycle stages deployed | P08 |

Independent S08a/S08b releases may land before S02. S08a reuses existing teardown; it does not claim recovery of ambiguous sends before S03/S04. Build S08a/S08b beside S01 because they touch separate files; the orchestrator serializes integration and release. Keep one agent slot free for review. Narrow any stage that cannot fit one working session while retaining a useful, releasable promise.

R01 is conditional on selecting a remote HTTP consumer and network policy. It must precede that consumer's first off-machine HTTP release. This readiness run does not silently turn a loopback bridge into a public service.

## S01 preserves the local contract while fixing target ownership

Use existing `GetStatus`, `GetDoctorReport` and transport connectors. Keep local-default JSON fields and behavior compatible. Distinguish client-local metadata from daemon-owned values; never present local paths as remote facts. A reachable explicitly selected Unix, TCP or command target supplies its own doctor report. Only the actual local-default profile may use offline local-store diagnostics.

Reject unsupported remote local repair before any effect, including mixed flags. An unavailable target cannot start or diagnose another local daemon. Do not print tokens or full command arguments. Exact-build mismatch must not produce misleading advice to restart an unrelated local process. Protocol incompatibility must fail clearly. Cover JSONL and watch behavior as well as table/JSON output.

Worker ownership: status/doctor commands, focused targeting tests and their source docs. Existing server/client helpers only where necessary. The orchestrator owns blueprint, ledger and release integration. New IPC types, draft work and broader client transport changes are outside S01.

Read-only preparation is recorded in the [send outcome audit](../../build-log/daemon-portability/evidence/s03-send-audit.md), [mailbox mutation audit](../../build-log/daemon-portability/evidence/s04-mutation-audit.md), [attachment boundary audit](../../build-log/daemon-portability/evidence/s05-attachment-audit.md), [event recovery audit](../../build-log/daemon-portability/evidence/s06-event-recovery-audit.md) and [analytics recovery audit](../../build-log/daemon-portability/evidence/s08c-analytics-recovery-audit.md). Each pins its source and must be rechecked after prerequisite releases; none marks its stage implemented or tested.

## Full verification follows the exact candidate

Every listed implementation stage is Full tier because it changes persistence, credentials/privacy, a public contract or shared mutable state. Each needs worker tests and applicable adversarial self-checks, inline code-simplifier, TypeScript review when relevant, orchestrator adversarial review and independent Claude verification through OpenCode. A changed candidate normally loses previous review status. The documented trivial post-triage exception permits a root-reviewed bounded delta only when it is limited to triaged fixes, adds no new protocol or ownership design, and its resulting checks and CI pass; S02 used that exception from whole-review candidate `010ae951` to `3d338beb`. Findings block only for concrete contradictions, data loss/duplication, failed user journeys/checks or security/privacy leaks.

On 11 October 2026, BK waived independent reviews for the remaining readiness
sequence until Anthropic is available again. Worker checks, orchestrator review,
exact-candidate CI, merge verification and published-client acceptance remain
required. Restore the independent review gate when that condition ends.

Current routing, discovered from the host model catalog: GPT-6-Astra/xhigh orchestrator; GPT-6-Luna/high workers by default; GPT-5.6-Terra is the newest balanced-family model; GPT-6.1-Sol is the current strong workhorse. Escalate effort before tier only after the skill's repeated-failure diagnosis. No worker escalation has occurred. S02 starts on Strong/high for per-draft concurrency, effect ordering and cross-client revision ownership; other workers remain Fast/high. S01 reuses existing diagnostic contracts; no named reasoning demand justifies starting above Fast.

Claude verification uses the preserved OpenCode 1.18.35 binary at `/Users/bhekanik/.codex/worktrees/ios-architecture-research/mxr/target/portability-tools/opencode-v1.18.35/opencode`. The ledger's `run_verifier.py` export records the fresh read-only session, Opus 5.5 and maximum effort. The custom reviewer allows only file reading/searching, denies external directories and runs with external plugins disabled. [Routing evidence](../../build-log/daemon-portability/evidence/verifier-routing.json) records the verifier setup.

## Each release proves the consuming-client journey

Before merge, fetch the base again, verify PR head/base/file list and resulting tree, and rerun checks invalidated by movement. Merge only the candidate that passed its required gates. Record the actual merged SHA/tree after squash.

Use the established release-please chain for every usable increment: feature/fix PR, successful required CI, merge, current release PR, its checks and merge, tag release workflow, published macOS/Linux artifacts and matching manifest. Never create or move release tags manually. A failed immutable release needs a forward fix/version; do not replace its artifacts.

Verify the installed release binary through the stage's consuming CLI/UI journey using isolated profiles. Record release version, source SHA/tree, workflow URL, asset/install check and the observable result. Keep the previous release artifact available as the known-good comparison. Restore the user's normal installation only through the approved release workflow; never use personal mail as a test fixture or kill unrelated daemons. A headless/Linux promise also requires a Linux run, not a macOS-only inference.

## Keep evidence and timing with the stage

Worker round budget: 45 minutes. Build/snapshot: 3 minutes. Checkpoint: 5 minutes. Independent review: 20 minutes. Release gate: 30 minutes after measured release workflows took 29m06s (v0.6.62) and 20m47s (v0.6.63). An overrun triggers timing/concurrency/tool-call inspection and a fix or measured explanation. It does not waive a correctness gate. Scoped local tests use agent-work with at most four workers; CI owns whole-workspace checks.

Each stage's durable build-log entry records outcome, run commands, exact source/review/release provenance, acceptance results, decisions, failures, next action and blocker. The observed Mac release cache gap is tracked in [its follow-up](../../issues/mac-release-rust-cache.md); no cache fix or savings are claimed. Record wall time, agent time lower bound, waiting, user attention and review rounds from transcript timestamps. Keep evidence under the project's working folder. Stage closure says what the user could do before, what they can now do, and the next capability.

## Current handover

Five slices are complete: S01 in v0.6.62, S08b in v0.6.63, S08a in v0.6.64, S02 in v0.6.65, and S03 in v0.6.68. S02's archive checksums and installer/Homebrew binaries matched. Its isolated CLI draft-revision and shutdown smokes passed through both install paths, and the embedded-browser journey passed against the published installer. The [S02 build log](../../build-log/daemon-portability/s02-draft-recovery.md) records the results; the [sealed packet](../../build-log/daemon-portability/evidence/s02-installed-v0.6.65/README.md) preserves its receipts, screenshots and manifest.

S03 merged as `d6ba93ba7082a644e027b331e67da44d8e6dd4e8`; CI run 38146965743 and the root-triaged Opus review passed. The v0.6.67 release attempt failed in CLI smoke because the job compiled to `target-cli` but invoked `target/debug/mxr`. Its v1 launch proof and provider smoke passed; its companion proof did not run. The v0.6.67 tag remains unpublished. Workflow fix PR357 merged as `86c95992`, and release metadata PR358 set v0.6.68 at source `621e9e49739eeb80abbccea0916578420941c3fc` (tree `6d391d9d90db51339d9a8d100e50915732a895eb`). Release run 38151840433 published v0.6.68. Its archive checksums and installer/Homebrew binary hashes match. Installed CLI, companion, TUI and browser acceptance passed on that release.

The [S03 release and CLI evidence](../../build-log/daemon-portability/evidence/s03-v0.6.68-release-installed.json), [TUI packet](../../build-log/daemon-portability/evidence/s03-installed-v0.6.68-tui/README.md) and [browser packet](../../build-log/daemon-portability/evidence/s03-installed-v0.6.68-browser/README.md) record the verified results and limits. S03 is done.

S04's pending-release candidate is `80457e31199d31f73c9dc891fbb17b343efc27b3` (tree `a754ecb025c4c39d86522979d0552ff314755c8f`), based on `ab2103ad8144e1f8d2b99f5717666fa2edbd5b6d`. Required CI is running. The final independent review attempt stopped before reading the candidate because the review API returned an insufficient-credit error; BK explicitly waived independent reviews until Anthropic is available again. The [S04 build log](../../build-log/daemon-portability/s04-archive-recovery.md) records the evidence limits: 17 CLI/IPC/HTTP and 3 MCP scenarios passed on immutable binary f718, whose production logic matches 804; this is not a build of the exact final candidate. Corrected and original Reading Archive journeys passed, and hook tests cover both completion orders. S04 is not merged or released. The regenerated API reference and docs build passed. Merge, release-please, published-artifact verification and installed-client TUI/browser acceptance remain before stage closure.

## Unresolved choices affect later stages

- What tested API compatibility window should be advertised before cross-version clients are supported?
- Which remote Mac workflows beyond read/reply/attach/mutate belong in the first remote release?
- What network and real migration destination/credential re-provisioning policy should apply when those deployments are selected?
