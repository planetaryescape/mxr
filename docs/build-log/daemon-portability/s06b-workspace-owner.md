# Private pending work keeps its original workspace

S06b is a candidate pending root checkpoint, runtime acceptance, CI and publication. The last pinned source checkpoint is `3d0da421d6f3916f49955bb49d3f96ff6cd43154`, rebased on `cab8c4c65feef2489fecaa0172783f7476ee0d90` (v0.6.87). Changes after that checkpoint are the worker-owned delta; this entry does not claim they are shipped.

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

Root's actual CLI, stdio MCP, native held-editor/cached-draft and browser two-daemon acceptance fixtures still require the final pinned binaries. CI and installed publication checks remain required. No personal mail or live provider participates.

## Fixture failures were corrected at the transport boundary

Node 26 shadows jsdom localStorage unless configured. Browser tests now install the same small private Storage fixture when browser storage is absent; product persistence remains strict. The web fake daemon needed to answer identity/capability discovery separately from the business request under test. Two hung owned jobs were cancelled and those fixtures corrected; all bridge tests subsequently passed. Canonical store snapshots avoid comparing in-memory subsecond timestamps with persisted second precision.

Root review found a mechanical capability-pattern insertion that rejected a modern daemon; the positive modern client test now covers it. Root Clippy found the central validator's large error result, fixed by boxing only the error response, and two missed benchmark message initializers, corrected without changing benchmark behavior. CLI help gained the owner option and outcome surface; its fixed inventory count is 266.

Inline TypeScript review and code simplification inspected the same source delta. Anti-slop reports generic boundary-parsing and module-mock findings that collide with established repository patterns; no schema dependency, type escape or blanket rule suppression was added. Rust/TypeScript checks validate the actual contract independently.

## Runtime acceptance and restored internal effects remain separate gates

S06b fences external scheduled-send and cancellation requests. S09 owns a future production generation rotation and the persistent pause for all restored internal provider effects, including direct internal dispatch. No production restore caller rotates the generation in this slice; rotation acceptance uses a controlled fixture. Ordinary legacy schedules are not disabled on upgrade.

The whole web formatter reports nine failures also reproduced from baseline `cab8c4c6`: eight untouched UI/test files and the raw generated schema. Changed manual TypeScript files pass the scoped formatter. Preserve the generator's existing raw output and leave unrelated formatting outside this slice.

The scoped Rust gate's existing mapper omits `crates/daemon` because that folder has no Cargo manifest. Explicit root Clippy fills this check for S06b; a bounded follow-up should map that path to `mxr`. No mapping refactor belongs to this slice.

Worker implementation started at 04:18 UTC on 12 October. The first 45-minute round included approximately three to four minutes for the damaged shared Git store's transplant, followed by a separate root checkpoint/rebase pause. Subsequent fixes and check waits overlap; transcript evidence does not support an exclusive active-agent/wait split. The final source pause is approximately 05:35 UTC (77 minutes wall from the worker start). Root records the exact final freeze time and final runtime/CI/publication receipts. The work exceeded the first round because of the Git environment recovery, fixture/capability reconciliation and concrete queue/owner review seams; no intelligence escalation or extra review agent was used.

Next action: root checkpoints the complete source delta, reviews the remaining correction diff, builds `mxr`/`mxr-mailmerge` and the embedded web runtime, and executes the final acceptance fixtures beside CI. Root owns Git, the orchestration ledger and release closeout. Preserve the S05b evidence and current fixture helpers.
