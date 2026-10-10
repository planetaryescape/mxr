# Each portability stage ships a usable workflow

Status: S01 merged as `f783d937` after independent Claude review and final CI. Release-please PR 332 prepares v0.6.62; publication and installed-client verification remain pending. Token persistence is in independent review, supervised shutdown has a tested candidate, and draft revision work can proceed independently. The user authorized implementing the full readiness sequence and releasing each stage on 2026-10-10. [Blueprint 23](../../blueprint/23-daemon-portability.md) owns the requirements; this file owns delivery order. The program ledger is `/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-daemon-portability-20261010/ledger.md`.

The outcome is safer existing clients across disconnects, restarts and different daemon hosts, leaving a future native SwiftUI client and personal VPS deployment possible with one active daemon owner. The iOS app, Hostinger provisioning and public HTTP exposure remain outside this readiness run.

The fetched base is `origin/main` at `80606f4833b651bf9b07975ddd49083d7cb2568a`, also the source of [v0.6.61](https://github.com/planetaryescape/mxr/releases/tag/v0.6.61). Its [release workflow](https://github.com/planetaryescape/mxr/actions/runs/37952667487) completed successfully. Preserve the dirty primary checkout; implementation uses isolated worktrees.

## Ship workflows instead of blueprint components

Each row includes domain behavior, persistence where needed, client changes, CLI JSON/JSONL, error handling, schemas, relevant documentation and tests. P04 schema/compatibility work accompanies its consuming workflow; it is never a standalone foundation stage. The released artifact must demonstrate the promise without depending on undeployed later work.

| Stage | After release, the user can | Release-defining journey | Depends on | Blueprint |
| --- | --- | --- | --- | --- |
| S01 | Diagnose the daemon they actually selected | With distinct local and target profiles, run status and doctor over cmd://; target fields are truthful and target failure never touches the local store | Existing transports | P06a |
| S02 | Save and resume a draft without silent concurrent overwrites | Save, restart, reopen, then edit from two clients; preserve the stale editor's text while rejecting its stale revision | Existing draft APIs; release after S01 | P01 + P04 |
| S03 | Recover the outcome of sending a reviewed draft | Drop a send response; recover its stored receipt. Inject ambiguous provider acceptance; preserve uncertainty without automatic resend | S02 deployed | P01/P02 + P04 |
| S04 | Retry a previewed mailbox action safely | Lose a batch-action response; retrieve per-item results with the same operation ID and reject changed payload reuse | S03 deployed | P02 + P04 |
| S05 | Attach and download without shared host paths | From disjoint filesystems, upload, save, restart, preview/send, and download identical bytes on the client | S02/S03 deployed | P03 + P04 |
| S06 | Recover mailbox state after an event gap without losing edits | Drop only WebSocket traffic, mutate through CLI, reconnect and refresh; switch workspace/recovery generation without replaying pending commands | Relevant S02-S05 contracts deployed | P05 + P04 |
| S07 | Use the same selected daemon from CLI and TUI | Read, reply through a local editor, attach, preview and mutate over the existing remote connector; prove compatible/incompatible versions | S03-S06 deployed | P06b + P04 |
| S08a | Stop and restart the daemon under supervision | Send SIGTERM and SIGINT to an isolated daemon; observe orderly teardown, restart the profile and query previously acknowledged state | Independent; interrupted-action recovery is verified in S08c after S03/S04 | P07a |
| S08b | Retain a complete disk token cache if replacement is interrupted | Inject interruption before/after atomic replacement with synthetic tokens; reload the complete old or new cache | Independent | P07b |
| S08c | Run unattended with useful dependency diagnostics | Start isolated Linux service without GUI secrets access, interrupt/restart and remove optional model service; inspect mail readiness | S08a/S08b and S03/S04 deployed | P07c |
| S09 | Restore useful state without accidentally executing old work | Inspect/preview a secret-free backup, restore into another profile, recover drafts/bytes/search, and verify effectful work remains paused | Durable resource and lifecycle stages deployed | P08 |

S02 uses the existing draft APIs and has no contract dependency on S01 diagnostic output. Its implementation can proceed during S01 publication; release order remains S01 first. S03 still requires the deployed S02 draft revision contract. Independent S08a/S08b releases may land before S02. S08a reuses existing teardown; it does not claim recovery of ambiguous sends before S03/S04. Build S08a/S08b beside S01 because they touch separate files; the orchestrator serializes integration and release. Keep one agent slot free for review. Narrow any stage that cannot fit one working session while retaining a useful, releasable promise.

R01 is conditional on selecting a remote HTTP consumer and network policy. It must precede that consumer's first off-machine HTTP release. This readiness run does not silently turn a loopback bridge into a public service.

## S01 preserves the local contract while fixing target ownership

Use existing `GetStatus`, `GetDoctorReport` and transport connectors. Keep local-default JSON fields and behavior compatible. Distinguish client-local metadata from daemon-owned values; never present local paths as remote facts. A reachable explicitly selected Unix, TCP or command target supplies its own doctor report. Only the actual local-default profile may use offline local-store diagnostics.

Reject unsupported remote local repair before any effect, including mixed flags. An unavailable target cannot start or diagnose another local daemon. Do not print tokens or full command arguments. Exact-build mismatch must not produce misleading advice to restart an unrelated local process. Protocol incompatibility must fail clearly. Cover JSONL and watch behavior as well as table/JSON output.

Worker ownership: status/doctor commands, focused targeting tests and their source docs. Existing server/client helpers only where necessary. The orchestrator owns blueprint, ledger and release integration. New IPC types, draft work and broader client transport changes are outside S01.

## Full verification follows the exact candidate

Every listed implementation stage is Full tier because it changes persistence, credentials/privacy, a public contract or shared mutable state. Each needs worker tests and applicable adversarial self-checks, inline code-simplifier, TypeScript review when relevant, orchestrator adversarial review and independent Claude verification through OpenCode. A changed candidate loses previous review status. Findings block only for concrete contradictions, data loss/duplication, failed user journeys/checks or security/privacy leaks.

Current routing, discovered from the host model catalog: GPT-6-Astra/xhigh orchestrator; GPT-6-Luna/high workers by default; GPT-5.6-Terra is the newest balanced-family model; GPT-6.1-Sol is the current strong workhorse. Escalate effort before tier only after the skill's repeated-failure diagnosis. No worker escalation has occurred. S01 reuses existing diagnostic contracts; no named reasoning demand justifies starting above Fast.

Claude verification uses the installed OpenCode 1.18.35 binary at `~/.opencode/bin/opencode`, with Opus 5.5 and maximum effort. A fresh session successfully read an isolated probe file; its exported metadata confirms the model. The custom reviewer allows only file reading/searching, denies external directories and runs with external plugins disabled. [Routing evidence](../../build-log/daemon-portability/evidence/verifier-routing.json) records the probe. The default OpenCode 2.0.20 returned no routes; no global configuration or saved credentials were changed to work around it.

## Each release proves the consuming-client journey

Before merge, fetch the base again, verify PR head/base/file list and resulting tree, and rerun checks invalidated by movement. Merge only the candidate that passed its required gates. Record the actual merged SHA/tree after squash.

Use the established release-please chain for every usable increment: feature/fix PR, successful required CI, merge, current release PR, its checks and merge, tag release workflow, published macOS/Linux artifacts and matching manifest. Never create or move release tags manually. A failed immutable release needs a forward fix/version; do not replace its artifacts.

Verify the installed release binary through the stage's consuming CLI/UI journey using isolated profiles. Record release version, source SHA/tree, workflow URL, asset/install check and the observable result. Keep the previous release artifact available as the known-good comparison. Restore the user's normal installation only through the approved release workflow; never use personal mail as a test fixture or kill unrelated daemons. A headless/Linux promise also requires a Linux run, not a macOS-only inference.

## Keep evidence and timing with the stage

Worker round budget: 45 minutes. Build/snapshot: 3 minutes. Checkpoint: 5 minutes. Independent review: 20 minutes. Release gate: 15 minutes. An overrun triggers timing/concurrency/tool-call inspection and a fix or measured explanation. It does not waive a correctness gate. Scoped local tests use agent-work with at most four workers; CI owns whole-workspace checks.

Each stage's durable build-log entry records outcome, run commands, exact source/review/release provenance, acceptance results, decisions, failures, next action and blocker. Record wall time, agent time lower bound, waiting, user attention and review rounds from transcript timestamps. Keep evidence under the project's working folder. Stage closure says what the user could do before, what they can now do, and the next capability.

## Unresolved choices affect later stages

- What tested API compatibility window should be advertised before cross-version clients are supported?
- Which remote Mac workflows beyond read/reply/attach/mutate belong in the first remote release?
- What network and real migration destination/credential re-provisioning policy should apply when those deployments are selected?
