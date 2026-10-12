# Private pending work keeps its original workspace

S06b is complete: source merged, v0.6.89 published, and the published CLI/raw IPC/MCP/mailmerge, native held-editor and same-origin browser journeys passed. The installed packet records source, binary hash, acceptance, cleanup and limits. Docs PR431 is deployed at `mxr.sh` (deployment `dpl_7zua4ZbLw8BQfDwGwXH9hjhpxqSf`); this follow-up command-example correction awaits production deployment. Independent review remains waived until Anthropic is available again.

| Release timing | Elapsed |
| --- | ---: |
| Full v0.6.89 workflow, 07:43:32–08:06:16 UTC | 22m44s |
| macOS build | 20m12s |
| Linux build | 14m32s |
| Release CLI smoke | 10m02s |

Build and smoke jobs overlapped; these workflow durations do not represent exclusive agent effort. See the [published release receipt](evidence/s06b-installed-v0.6.89/release-acceptance.json).

S06b merged through [PR422](https://github.com/planetaryescape/mxr/pull/422) at `45f917b53e26cef3b9fdde8ff3fad025084d15e7`; reviewed candidate `2460aba5f459dc15d9e9861be17043dd215650ce` (tree `32b104d5eeeb8373dfd5ac7a813cddf29965747c`) exactly matches the merged tree. CI [38214298080](https://github.com/planetaryescape/mxr/actions/runs/38214298080) and OpenAPI [38214298085](https://github.com/planetaryescape/mxr/actions/runs/38214298085) passed. The v0.6.88 release attempt built macOS in 22m23s and Linux in 13m47s, but its CLI smoke failed independently at raw `save-draft`: `v1_launch_proof.sh` omitted `expected_workspace`, so the daemon returned `workspace_owner_required`; MCP send/outcome calls also omitted the owner. Homebrew and GitHub Release jobs were skipped. PR425 fixed release fixtures without product-code changes. Release metadata PR426 merged as source `1894c8b67dff7789f675a3bfe80023db4c98bba3` (tree `002a3967334458f69892e5a86c7bdea0e431f960`); Release Please [38219297920](https://github.com/planetaryescape/mxr/actions/runs/38219297920) passed. At the 2026-10-12 07:56:14 UTC snapshot, v0.6.89 was still a draft and release workflow [38219348976](https://github.com/planetaryescape/mxr/actions/runs/38219348976) was in progress; its CLI smoke had passed at 07:53:57 UTC while both platform builds remained active, and the latest published tag was v0.6.87. Publication and installed-client acceptance were pending at that snapshot. The later published-release and client receipts are in the [installed packet](evidence/s06b-installed-v0.6.89/README.md).

The [integrated evidence packet](evidence/s06b-integrated-2460aba5/README.md) links merge, review, CI, OpenAPI and cleanup receipts. Runtime client proofs used frozen source `0b0591884de84695d5cd494433a19e9489716a16` (tree `753f713905cdeb338dbbe4d05554e469aff282a5`, binary SHA-256 `8be64bceb411e97ebf16cfc373c40064d8a36d5477ad97b90eb66572761a4e8d`). The 91ca follow-up changed tests and user-facing help/error strings; the final 2460 follow-up changed only two TUI test modules. These follow-ups left runtime control flow and the wire contract unchanged. Do not attribute those proofs to a published binary.

The completion check uses isolated synthetic A and B workspaces with the same account, draft ID, revision, content and finalized resource bytes. Queue private work under A, replace the target with B or rotate its recovery generation, and confirm no replay or provider effect. The text, attachment references and pending outcomes must survive reload. Return to A, explicitly recover, and observe one send. An ordinary A restart must keep the same durable owner usable.

## Every command carries the owner captured before preparation

IPC protocol 11 adds optional `expected_workspace` to the existing message envelope. `workspace_owner_supported` defaults false. The daemon validates the durable Archive workspace ID and recovery generation before dispatch. Owner-sensitive commands without identity fail closed; an explicit stale identity also fences ordinary commands and reads. Request classification remains unchanged.

`crates/daemon/src/handler/workspace_owner.rs` owns the central check. `crates/client/src/lib.rs` verifies the original owner before private attachment preparation and retains it through reconnect and nested upload requests. Missing identity fails before file access. Capability checks prevent older daemons from ignoring the envelope. Failure is nonretryable and asks for explicit reopen/review or return to the original workspace.

The web bridge carries immutable request ownership in `crates/web/src/workspace_owner.rs`. It validates supplied ownership before body consumption and checks private compose sidecars before reading or deleting files. Sidecars preserve `compose_recovery_source` and S05b resource tokens/body whitespace. CORS exposes only the non-secret owner response header, including conflicts. Missing owner cannot create a compose session. Ordinary authenticated read responses carry observed identity; discovery and malformed unowned requests keep their existing local validation paths.

The browser captures target and owner before handshake, retry and queued execution. Late responses cannot restore an old selected owner or populate new mailbox caches. The existing query coordinator and S06a refresh machinery cancel stale reads and refetch the selected owner while retaining compose state. Active compose journals now retain the actual text, frontmatter, attachment references and original target/owner. Corrupt or unwritable storage is retained and reported. Completing A removes A alone. Legacy path-only records require a deliberate saved-draft reopen and retain their original reference.

The TUI retains one original session owner through reconnect, so cached draft-list operations and standalone edit tasks cannot recapture B. Editor files and queued mutations carry original owner. A session whose initial ownership discovery failed cannot later adopt a workspace implicitly. CLI editor work captures ownership before private input; JSON/JSONL receipts and previews return `expected_workspace`. Reviewed send and send-outcome recovery accept that original identity. MCP echoes ownership through its per-call connections and reviewed DTOs.

Mailmerge records creation and reviewed ownership, passes it to preview/send/outcome subprocesses, and preserves legacy attempted commands without replay when ownership is missing. It still talks only to the CLI and adds no internal crate dependency.

## Focused checks exercise effects and retention

Evidence lives under `target/s06b-evidence` in the assigned worktree. Earlier candidate evidence is deliberately separate from the final frozen runtime:

- `baseline-collision/evidence.json`: published v0.6.86 reproduced a colliding B send (B spool 0 to 1).
- `candidate-collision/evidence.json`: the earlier local candidate refused the stale owner (B spool stayed 0). Its binary predates the final source delta and cannot establish final runtime acceptance.
- `client-full-tests2.log`: 41 client tests passed, including positive modern-capability save/delete, missing/stale ownership before nonexistent attachment-file access and S05b bounded byte upload.
- `web-full-tests6.log`: 131 bridge unit tests and seven integration tests passed. Wrong-owner sidecars preserve private files and forward no body; cross-origin success/conflict expose the owner header. Recovery/resource regressions remain covered.
- `mailmerge-tests4.log`: 101 companion tests passed, including retained owner in send/outcome argv and legacy record/review refusal without any subprocess.
- `mcp-tests4.log`: 24 MCP tests passed before the final unchanged MCP checkpoint.
- `tui-compose-tests3.log`: eight focused native compose tests passed. `tui-ipc-tests4.log`: three IPC/event tests passed; startup discovery forwards events while waiting for identity.
- `web-final-focused4.log`: 49 browser unit tests passed across ownership/auth retry, private snapshot reload, explicit legacy recovery, autosave, pending outcomes, Archive recovery, queues and authoritative refresh.
- `typecheck-final5.log`, `web-lint-final4.log`, `web-format-scoped.log`: TypeScript and lint passed; final lint uses four threads.
- `schema-generation3.log`: OpenAPI/TypeScript generation completed on v0.6.87 and protocol 11.
- `daemon-outcomes-final.log`: nine send-outcome tests passed; `cli-help-tests3.log`: both normal-stack help/completion tests passed (266 help cases). `tui-ipc-tests5.log`: final three native IPC/event checks passed.
- `scoped-rust-gate6.log`, `root-rust-clippy3.log`: mapped touched-crate hygiene and explicit root all-target/all-feature Clippy passed.

Root's CLI/stdio MCP/mailmerge, native held-editor and actual browser attachment owner-switch fixtures passed against the frozen binary. The final native test-only correction passed all nine TUI suites (908 library tests and 80 integration tests, 988 total). The two browser runtimes' six owned roles were verified stopped with tokens absent, sockets and ports closed, and owned records removed. No personal mail or live provider participated. Published installer, Homebrew and all four client acceptance paths are verified in the installed packet; S06b is complete. Production deployment of this follow-up documentation correction remains pending.

The focused TUI run occupied task 6258 for 7.20 seconds: 0.20 seconds queued and 7.00 seconds executing. Initial CI caught stale native fakes; the 988-test scoped run confirmed their corrections before the final CI rerun. That makes the scoped suite a useful cheaper gate before rerunning whole-repository CI. Earlier worker timing combined another task's queue time and is superseded by this measured split.

## Fixture failures were corrected at the transport boundary

Node 26 shadows jsdom localStorage unless configured. Browser tests now install the same small private Storage fixture when browser storage is absent; product persistence remains strict. The web fake daemon needed to answer identity/capability discovery separately from the business request under test. Two hung owned jobs were cancelled and those fixtures corrected; all bridge tests subsequently passed. Canonical store snapshots avoid comparing in-memory subsecond timestamps with persisted second precision.

Root review found a mechanical capability-pattern insertion that rejected a modern daemon; the positive modern client test now covers it. Root Clippy found the central validator's large error result, fixed by boxing only the error response, and two missed benchmark message initializers, corrected without changing benchmark behavior. CLI help gained the owner option and outcome surface; its fixed inventory count is 266.

Inline TypeScript review and code simplification inspected the same source delta. Anti-slop reports generic boundary-parsing and module-mock findings that collide with established repository patterns; no schema dependency, type escape or blanket rule suppression was added. Rust/TypeScript checks validate the actual contract independently.

## Runtime acceptance and restored internal effects remain separate gates

S06b fences external scheduled-send and cancellation requests. S09 owns a future production generation rotation and the persistent pause for all restored internal provider effects, including direct internal dispatch. No production restore caller rotates the generation in this slice; rotation acceptance uses a controlled fixture. Ordinary legacy schedules are not disabled on upgrade.

The whole web formatter reports nine failures also reproduced from baseline `cab8c4c6`: eight untouched UI/test files and the raw generated schema. Changed manual TypeScript files pass the scoped formatter. Preserve the generator's existing raw output and leave unrelated formatting outside this slice.

The scoped Rust gate's existing mapper omits `crates/daemon` because that folder has no Cargo manifest. Explicit root Clippy fills this check for S06b; a bounded follow-up should map that path to `mxr`. No mapping refactor belongs to this slice.

Worker implementation started at 04:18 UTC on 12 October. The first 45-minute round included approximately three to four minutes for the damaged shared Git store's transplant, followed by a separate root checkpoint/rebase pause. Subsequent fixes and check waits overlap; transcript evidence does not support an exclusive active-agent/wait split. The first source pause was 05:35:47 UTC (77 minutes wall from the worker start). Root records the exact final freeze time and final runtime/CI/publication receipts. The work exceeded the first round because of the Git environment recovery, fixture/capability reconciliation and concrete queue/owner review seams; no intelligence escalation or extra review agent was used.

## Close and reopen waits for the original owner's queued save

The full browser suite on `653ac81d` passed 949 tests and failed 47 in five files, with two rejected promises. Most fixtures constructed sessions without the new original binding and omitted the registry API from their mocks. Those fixtures now carry original ownership and continue asserting private text, revisions, resources and send behavior.

The bound fixtures exposed a real regression: the local snapshot returned before a pending Close/unmount save settled, and the older conflict snapshot could replace newer text in the journal. `activeDrafts.ts` now settles and rereads only when the retained owner/target is the known selected owner, then checks the original binding again before a recovery refresh. Unknown or changed owners immediately retain the private snapshot without network work. `useComposeAutosave.ts` writes the latest editor body and mapped attachment references inside the owned callback before its queue tail settles; the late-response guard runs before revision/journal mutation. The close/reopen and unmount assertions pass without weakening their expected text or token.

The bridge's existing independent recovery copies can relocate local attachment paths. Matching body/revision and attachment count alone did not establish that the source references matched the retained private selection. The existing path request DTO now accepts optional `expected_attachments`; automatic bound recovery refresh sends the snapshot's exact array. After sidecar ownership validation and while holding the compose-file lock, the bridge compares those references before cloning or attachment materialization. A mismatch returns `compose_attachment_conflict`, leaving the source and client snapshot intact. Explicit older callers remain decodable.

`web-all-recovery-fix6.log` passes all 168 browser files and 999 tests, with no unhandled errors. `web-bridge-recovery-fix5.log` passes 132 bridge tests and seven integration tests; the strengthened direct guard fixture in `web-bridge-recovery-fix7.log` also passes. The final seven focused bridge recovery tests in `web-bridge-recovery-fix9.log` pass after reading the source once and cloning those same checked bytes. Positive remapping/resource independence remains asserted, and negative same-count identity changes preserve the private references. `typecheck-recovery-fix6.log`, `lint-recovery-fix6.log`, `schema-recovery-fix5.log` and `scoped-gate-recovery-fix9.log` retain the final checks for this delta. Anti-slop findings on these lines are four established module-mock patterns and an unknown catch value at the promise boundary; no rule or type contract was weakened.

Root's `target/s06b-evidence/clients-653ac81d/result.json` reports all ten raw IPC/CLI/stdio MCP/mailmerge cases passed against binary SHA `be4be7b730cdeaea2b84c5e2c80351fdb6a700435e3be16cc800c33ae46624cc`, with both owned daemons stopped and sockets removed. These receipts precede the browser recovery correction and its DTO guard; the final embedded build and runtime acceptance must use the next frozen source. The recovery correction began after source returned at 05:43 UTC, followed by the attachment assertion round from 05:53 UTC. The worker releases source ownership at approximately 06:02 UTC; all owned checks have completed and no daemon or build remains active. Root owns the exact next checkpoint and release receipts.

Next action: wait for the v0.6.89 release workflow and assets, then rerun installed CLI, MCP, native TUI and browser acceptance. Add installed receipts only after those checks pass. Root owns Git, the orchestration ledger and release closeout. Preserve the failed v0.6.88 attempt in the integrated packet and keep it separate from the historical `bb64d37` candidate packet.
