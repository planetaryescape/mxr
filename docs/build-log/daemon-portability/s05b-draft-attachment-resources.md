# Saved drafts retain the bytes the client uploaded

Status on 2026-10-12: S05b is complete in published v0.6.86, built from
`b651ee69ae01a1e08ecc021fa8d7767aa68cf12f` (tree
`9b220eca0fb79c13eaaec66a4f4bdfee5603d962`). Release workflow 38201282820
succeeded. PR413 merged as `6d6fe2da39178681de396f8f807d4d7bc3020961` (tree
`f6325003fe616e9957d634feac80a5a59fb3687f`); CI 38199485371, including Web
E2E, and OpenAPI run 38199485380 passed. Release PR417 produced v0.6.86. All
four GitHub asset digests and both archive checksum files
matched; the installer and Homebrew binary SHA-256 matched
`619427b3e25e14b6588cc3f24fc21fcf1d397643b2cc703ef686e4ff3d675d06`.
Homebrew was present only at `/opt/homebrew/bin/mxr`; the personal daemon was
not restarted. The final documentation closeout deployment remains pending.
The standing independent Claude review waiver remains in effect until Anthropic
is funded.

Published CLI, MCP, TUI and browser acceptance passed. CLI verified previews
and stale revisions make no resource, then saved, reopened and sent the exact
576,000-byte file; retained outcome replay did not send twice. MCP verified
read/write isolation, preview-without-write, ownership and integrity checks,
old-client review-marker fencing, restart and receipt replay with the exact
193,900-byte file. Native TUI verified save under daemon-file denial, client
relaunch with the resource token after deleting the original, a cross-client
CLI send of the 384,800-byte resource, and a separate direct TUI send of
349,200 bytes. The saved-draft TUI relaunch kept its fake-provider daemon alive
because that provider's drafts are in memory; CLI, MCP, browser and HTTP
recovery each separately proved daemon restart. Browser UI uploaded
1,495,000 bytes, modified and deleted the client file, reloaded and restarted
the daemon, reopened the draft, and sent the original filename, bytes and SHA
once. The browser journey made no filesystem-denial claim. Root's separate real
HTTP bridge/daemon recovery run retried a failed save, deleted the source file,
restarted the daemon and delivered the exact 432,000 bytes once. Its UUID-prefixed
legacy filename remains a nonblocking follow-up; normal managed browser upload
preserved the original filename. The live Gmail step in release CLI smoke was
skipped because CI lacked `MXR_GMAIL_TEST` credentials.

The installed evidence packet is in
[`evidence/s05b-installed-v0.6.86/`](evidence/s05b-installed-v0.6.86/).
Candidate source, root review and earlier receipts remain in the historical
[`evidence/s05b-candidate-140225cd/`](evidence/s05b-candidate-140225cd/)
packet; candidate `0ea54be1` CLI/MCP/TUI/browser proofs are not current release
acceptance. The `compose.md` guide already describes chunked client upload,
resource metadata, browser reload and TUI edit behavior; no guide rewrite was
needed for this closeout. S05b raises the program tally to ten complete slices;
S06b is active, with S07 and S09 planned.

| Work | Observed duration |
| --- | ---: |
| [PR413 Web E2E](https://github.com/planetaryescape/mxr/actions/runs/38199485371) | 15m28s (03:22:56–03:38:24 UTC) |
| [Release PR417 CI](https://github.com/planetaryescape/mxr/actions/runs/38200620112) | 6m06s (03:39:44–03:45:50 UTC) |
| v0.6.86 release workflow | 23m32s (03:49:35–04:13:07 UTC), within 30m gate |
| Linux release job | 17m45s, including 16m33s compile |
| macOS release job | 20m17s, including 18m34s compile |
| Release CLI smoke | 9m03s; live Gmail step skipped |
| Homebrew Formula job | 2m09s, including 1m46s install verification |
| Published browser UI runtime | 75s, from fixture initialization through cleanup |
| Local SPA build task 6123 | 4.61s execution; 0.08s queue wait |
| Local embedded binary task 6125 | 1m42s execution; 0.13s queue wait |
| Final documentation site build | 36.4s runtime; 0.06s queue wait |

Release jobs overlapped; their durations do not sum to the full workflow time.
The checkpoint notes below retain the chronology of earlier reviews and runs;
pending statements refer to those snapshots and are superseded by this status.

Initial worker checks used root checkpoint `a457130dce40687d06c02872b92079437b715a4a`
(tree `eda8a404b930062401af19e7e6e9f6e4d0f48fc1`), integrated onto fresh main
`74a151c5c41d2d3f6b780fdbeb92c74db535ab57`. Migration 077 is personal
categories, 078 is to-do resolution suggestions, and 079 adds draft resources.
The attachment enum variants require IPC protocol 9; the web compatibility
gate matches it. Root owns Git, CI and release.
The initial worker base was `5007eb3f95484ec3a067fe5d3126d3aa8d1fa9d3`;
earlier checkpoint and verification history remains below.

## Uploads have a bounded daemon resource contract

`Draft.managed_attachments` stores immutable resource metadata separately from
legacy `Draft.attachments` paths. Resources carry a typed opaque ID, owner draft
and account, filename, MIME type, exact length and lowercase SHA-256. Begin
requires a saved, editable owner. Preview validates a declaration without
creating an owner, file or metadata row. Raw chunks are at most 256 KiB; MCP
accepts at most 64 KiB. A resource is limited to 25 MiB. This is a local disk
policy; it does not set provider limits or change legacy path behavior.

```sh
mxr compose --to alice@example.com --subject 'Portable attachment' \
  --body 'Please read the attachment.' --attach ./notes.txt --draft --format json
mxr drafts --format json attach DRAFT_ID ./another.txt --dry-run
mxr drafts --format json attach DRAFT_ID ./another.txt --expected-revision REVISION
mxr send DRAFT_ID --dry-run --format json
```

Shared Rust client preparation capability-checks before opening a new client
file, saves the owner, uploads bounded chunks, then CAS-updates its resource set.
CLI compose/reply/forward select their send identity after the resource save.
TUI preparation selects and retains that canonical revision before submission.
The provider receives a resolved clone containing daemon-local paths; stored
drafts and reviewed command hashes retain the portable metadata.

## Omission cannot silently remove managed attachments

An absent managed field identifies an old-wire caller; an explicit empty set
removes resources. Updates and safety checks from a caller omitting a stored,
nonempty set fail with an upgrade instruction. Stored send and scheduling
requests carry a default-false review marker, so an old caller cannot transmit
attachments invisible in its review. Download and upload capabilities remain
independent: v0.6.82 downloads still work against a daemon advertising upload
support as absent/false.

Legacy path drafts remain readable and sendable. Their path-backed bytes stay
mutable. Opening a migrated store with an old binary is unsupported: old
binaries predate the managed field and cannot enforce this contract. Keep the
current binary and forward-fix problems; downgrade only with a pre-migration
backup and its matching binary.

## File publication precedes ready metadata

The per-draft operation guard covers upload begin/write/finalize, set validation
and CAS, cleanup rechecks, and sending through Sent ingestion. No SQLite
transaction remains open during client input or provider waits. Chunks append
sequentially, and a repeated committed range must contain the identical bytes.
Finalize verifies length and digest, syncs the file, publishes it under a private
resource directory, syncs the directory, then marks metadata ready. A retry
recognizes and verifies a published file left by a crash before the ready write.
Finalized files are read-only on Unix; their directories are private.

Preview and send resolve the same resource metadata. Send rehashes actual files
before claiming submission. Missing or corrupt bytes have no provider effect.
Scheduled revisions become stale when the resource set changes. Sending,
uncertain and accepted-but-not-ingested drafts remain owners and retain bytes.

Cleanup runs on new upload begins. It considers at most 32 resources older than
seven days whose owner draft is absent and which have no unknown or accepted-not-ingested
durable send outcome, then rechecks both references under that draft's guard
before removing files and metadata. Existing owners retain even
abandoned partial uploads. A stopped daemon leaves files for the next begin;
there is no new cleanup scheduler.

```sh
scripts/cargo-test -p mxr --lib draft_attachment_resources
scripts/cargo-test -p mxr-client --lib portable_upload
scripts/cargo-test -p mxr-mcp --lib draft_attachment_tools
```

## The first daemon proof observed provider bytes

Pueue task 6004 queued at 02:07:55+01:00 and started at 02:08:53+01:00
(58 seconds waiting). Cargo compilation took 3m07, seven seconds over the
assigned three-minute build budget. All five resource tests passed in 2.81s:

- `draft_attachment_resources_chunks_retries_cas_and_provider_bytes`
- `draft_attachment_resources_bounds_ownership_and_incomplete_finalize`
- `draft_attachment_resources_empty_duplicate_corrupt_and_review_invalidation`
- `draft_attachment_resources_published_file_crash_retry_and_cleanup_retention`
- `draft_attachment_resources_restart_sends_original_client_bytes`

The restart test reconstructs AppState and SQLite from the same persisted store,
changes and deletes the synthetic client file, previews the saved resource, then
sends it. The fake provider reads the actual attachment file at its send
boundary; its witness digest and length match the original bytes. The durable
Sent receipt is queried afterward. This proves daemon persistence and provider
input, and does not replace root's actual released CLI/TUI/browser/MCP journey.

The fake spool acceptance JSON now records per-attachment filename, length and
SHA-256, with no bytes or body. Production providers continue using
`MailSendProvider`; existing whole-buffer MIME construction remains a limit.

## Root checkpoint still owns release verification

Next, verify the final source tree after the scoped client/MCP/browser tests,
regenerate web types, OpenAPI and CLI help, run the scoped pre-PR gate, and build
`mxr`. Root integrates freshly fetched upstream, reviews that exact checkpoint,
and owns CI, release-please and installed-client acceptance. No personal mail,
existing daemon restart, external deployment, Git operation or shared ledger
edit occurred in the worker.

## Browser and client checks reached the checkpoint

Task 6007 compiled in 2m43 after about one second in the queue. It passed
`portable_upload_rejects_old_daemon_before_opening_client_file`,
`portable_upload_sends_bounded_bytes_and_preserves_selected_send_identity`, and
`draft_attachment_tools_preview_without_writes_and_bound_raw_chunks`. Its
`draft_attachment` filter matched no compose or web test; it is not browser
coverage. Task 6013 separately passed
`compose_attachment_upload_returns_managed_resource_and_revision`, including
reload of the finalized revision and resource token, in 2.76s after 51.79s
compilation. TypeScript typecheck, targeted strict oxlint, and anti-slop checks
passed.

The browser hook regressions cover bounded chunks, the new revision surviving
the next edit, completion after switching drafts, and an unsupported daemon
refusing before reading client bytes. The final compose suite is run under
bundled Node 24.19.0, matching CI's Node 24. On PATH, Node 26.9.0 failed twelve
existing ComposeHost send tests; the first reproduced test passed under the
bundled runtime. The test probe was restored verbatim. No product or test
compatibility workaround was added for that runtime difference.

Root requested a pause before the final build to integrate fresh main once.
The queued final resource regression run is task 6021. Its new
`draft_attachment_resources_pre_upgrade_hash_replay_preserves_unknown_and_accepted`
test persists the pre-S05b field-omitted, struct-order command hash and checks
None and explicit-empty resource sets against both retained outcomes, with no
provider invocation. Generated web types/OpenAPI/help, editor and safety
focused checks, scoped pre-PR hygiene and the final binary build remain for
the freshly integrated tree. Root's `s05b_cli_accept.py` owns the actual
reciprocal-filesystem-denial CLI journey.

Task 6021 passed all six resource tests in 2.01s after 3m16 compilation and
108 seconds waiting (02:27:20+01:00 queued, 02:29:08+01:00 started). The
pre-upgrade hash regression passed both outcome states and both empty-field
representations. All 42 compose JavaScript tests passed under Node 24.19.0.

A final retention correction excludes owner-absent unknown/accepted-not-ingested
outcomes from cleanup and rechecks the outcome under the draft guard. This
covers an owner row removed by an account cascade; the outcome is still a
durable reference. The added
`draft_attachment_resources_cleanup_retains_owner_absent_inflight_bytes` and
`draft_attachment_resources_concurrent_sets_have_one_cas_winner` tests are not
yet run; they belong to the final integrated resource gate. No compilation is
running at this handoff. Root must checkpoint and integrate upstream before
the remaining gates.

The replay choke point returns a matching retained outcome before consulting
resource liveness. Completed resources may have been cleaned after seven days;
that cannot turn an accepted replay into a missing-resource failure. The
existing provider-byte test now also removes completed resource metadata/files
and asserts the same `SendDraft` identity returns its receipt without another
provider invocation. That extension awaits the integrated resource gate.


## Integration preserves categories and requires protocol 8

Root rebased the worker checkpoint onto v0.6.84 source
`98bc13f574da5cfc93fec1dfc1dddab4d6e996b9`. The resumed checkpoint is
`3a30079e6c008aec6c1e1d2746e39ff5584bb1c7` (tree
`bd8974940c2c7ad72b32865f319130fedf660167`). Migration 077 remains personal
categories; 078 adds draft resources. New request/response variants require
IPC protocol 8 under the integrated protocol policy. The web compatibility
gate and fixtures follow that version.

Root's review found generic HTTP and MCP stored-send handlers deriving review
from old request shapes. Their inputs now carry an explicit default-false
`managed_attachments_reviewed` field. Compose sessions still derive review
from rendered resource tokens. Raw preview/begin/chunk/finalize requests also
check upload capability on their connection before sending the new variant,
so MCP receives upgrade guidance from the shared transport.

The upload endpoint returns schema-backed begin, chunk and finalize response
shapes. The browser overloads use generated response types. The unused generic
browser stored-send helper now requires its caller to supply a review marker;
no current browser journey invokes that helper.

Task 6038 was queued at 02:48:21+01:00 and started at 02:48:28+01:00 (seven
seconds waiting). It runs the integrated daemon/client/MCP/store/web
`draft_attachment` filter. The upgrade test preserves legacy paths, draft
revision, categories and a durable unknown outcome across 077→078, then
reopens a partial 078 with its column already present and resource table absent.
Its outcome and remaining gates will be recorded before the next checkpoint.


Task 6038 stopped on a test-only digest formatting compile error; Pueue recorded
02:48:28–02:51:07+01:00 (2m39). Task 6040 corrected the fixture and passed:
nine daemon resource tests, three shared-client upload tests, two MCP tests,
one store migration test and one generic HTTP send/schedule marker test.
Cargo took 2m06; test runtimes were 2.37s, 0.03s, 0.01s, 0.28s and 2.53s.
Pueue recorded 02:51:44–02:54:11+01:00, with less than a second waiting.

The browser now retains each finalized resource and owner revision immediately.
A later upload failure in the same batch preserves acknowledged attachments.
All 66 focused compose/protocol/mailbox JavaScript tests passed under Node
24.19.0 in 5.97s; the four upload hook tests passed again after their fixture
contracts were aligned with generated response types.

The code-simplifier and TypeScript review use the scoped diff against integrated
base `98bc13f5`. They removed anonymous unknown-return/dictionary fixture types
in favor of generated upload responses and a named request fixture. Three
anti-slop findings remain by repository convention: two `vi.mock` boundaries
and the React `SetStateAction` callback check. Adjacent compose tests use these
patterns; changing production dependency boundaries just to satisfy the generic
lint would widen this slice. Strict project oxlint reported zero findings.
Task 6044 owns the remaining focused Rust checks and source-generated snapshot
artifacts. The scoped hygiene gate and binary build follow that task.


Task 6044 passed 212 small-crate tests, 112 TUI compose tests and seven TUI
send-recovery tests, then the web upload/reload test and OpenAPI summary
snapshot. Cargo compiles took 4m08, 1m14, 0.35s, 1m23 and 0.67s. Test
runtimes were 0.33s (client), 0.08s (compose), 0.11s (protocol), 0.04s
(safety), 0.15s (each TUI filter), 4.18s (web upload) and 4.76s (snapshot).
The first compile exceeded the three-minute budget by 68 seconds. The CLI
help test compiled in 1m46 and stopped on its fixed case count: adding
`drafts attach` changes 263 to 264. That assertion is now reconciled with
the command vector.

Task 6051 narrowed the retry to CLI help. It compiled in 1m06, then the debug
binary overflowed its main stack on `--help` at the default macOS shell limit
8176 KiB. The same binary returns valid help with `ulimit -s 32768`.
LLDB stops in Clap's generated `augment_subcommands`; its debug source mapping
reports `triage.rs:53`, while the command declaration is in `crates/daemon/src/cli/mod.rs`. Root is comparing the unchanged S08 debug
build before deciding on a source fix; this is an unresolved check, not a
settled environmental exception. Evidence is retained in
`/tmp/mxr-s05b-help-stack-backtrace.log` and
`/tmp/mxr-s05b-help-stack-probe.txt`. No entry-point edit occurred.
Task 6056 regenerates help and contracts at the measured 32 MiB shell stack.
CI and published optimized acceptance must use normal defaults.


## Generated contracts are ready for root's stack fix

Task 6056 passed generation at the measured 32 MiB process stack. It queued at
03:09:44+01:00, started at 03:12:10+01:00 (146 seconds waiting), and ended at
03:13:48+01:00. CLI help compiled in 39.86s and ran in 8.26s across 264 cases.
It updated the drafts parent snapshot and added the attach snapshot. The current
source exporter regenerated `apps/web/src/api/generated.ts` and
`site/public/openapi.json`; the site generator regenerated 114 command pages.
The OpenAPI summary snapshot is current. The selected upload API uses generated
response types; the three exact shapes form a non-overlapping schema union.
Generic HTTP/MCP review markers default to false and have explicit input fields.

TypeScript checking, targeted strict project oxlint, formatting and `git diff
--check` passed after generation. Source inputs are stable. There is no active
worker queue/session, daemon or fixture at this pause.

Root's baseline comparison establishes the debug stack overflow as a regression:
S08 succeeds at the normal stack and S05 fails there. Root owns the measured
entry-frame fix. Keep the raised stack confined to artifact preparation; the
existing CLI help test must pass at the normal stack after the fix.

Exact next actions: root checkpoints this worker delta, applies and verifies the
entry-frame fix, starts PR CI, then signals the worker to resume the scoped
pre-PR Rust gate against `98bc13f5` and `cargo build -p mxr`. The expected binary
is this worktree's `target-cli/debug/mxr`. Those two gates remain pending; no
final build has been claimed. Root's CLI/MCP acceptance scripts then exercise
the frozen candidate, and root owns release-please and actual published-client
acceptance. The MCP send input field is `managed_attachments_reviewed`; capable
callers set it after reviewing canonical metadata, alongside `confirm=true`.

## CLI parsing no longer shares the dispatch frame

The root debugger measured the original `run_cli` poll frame at 4,002,736
bytes and Clap's outer generated command frame at roughly 4 MiB. The main
function frame was about 50 KiB, so moving future construction in main would
not address the dominant allocation. Separate parsing from the large async
dispatch function. Task 6065 passed all 264 help cases at the inherited
8,372,224-byte process stack (146.8 seconds queue wait, 107.9 seconds execution;
1m36 compile, 8.39 seconds tests).

An actual `completions bash` run then reproduced the same nested Clap frame
inside dispatch. Its synchronous generator now runs on Tokio's existing
blocking pool, and a process-level regression test exercises that command
with an isolated profile. The process stack setting remains unchanged. Task
6072 owns the final normal-stack help and completions check; its outcome is
pending at this source checkpoint.

The candidate browser check must use a freshly built SPA embedded with
`cargo build -p mxr --features web-ui`; a default build omits that interface.
Task 6073 builds the current SPA under Node 24.19.0. Scoped Rust hygiene, the
embedded binary build, client runtime acceptance and release remain pending.


## Recovery saves publish canonical resource references

Root committed the recovery correction at `3e6ba18c`, then integrated upstream
at `a457130d`. The normal update, explicit recovery retry and shared persistence
paths now replace copied filesystem entries with the accepted draft's resource
tokens before writing its sidecar. Only the YAML header changes; editor body
whitespace, context and other frontmatter remain intact. A failed commit keeps
the private recovery and reports the accepted draft where applicable.

The shared test byte fixture owns decoded chunks and verifies their exact
payload, declared owner, length and digest before readiness. Saved and unsaved
recovery tests compare canonical metadata and tokens with those ready bytes,
discard the private copy, and verify bytes remain. The saved test adds a normal
local-file edit, deletes that file, then edits with tokens and checks that no
new resource is uploaded. The unsaved retry returns canonical revision 2 from
owner save plus resource CAS; a lost create response still cannot overwrite
the first saved snapshot. Task 6099 passed all 129 web tests before integration
with 40.82s total job execution and 3.17s test runtime.

The earlier embedded candidate `0ea54be1` passed root's actual CLI, MCP, TUI
and browser byte witnesses, but the recovery bug was reproduced against that
candidate and required this correction. Those receipts are prior evidence;
the final integrated candidate and published artifact still require acceptance.

Current next actions: regenerate OpenAPI, TypeScript and help on protocol 9;
verify migrations through 079, normal-stack CLI help/completions, the integrated
web package, protocol compatibility/typecheck and scoped Rust hygiene. Pause
before the binary build so root can checkpoint the generated source. Root then
resumes a current SPA production build and `cargo build -p mxr --features web-ui`
on the exact checkpoint. No personal daemon or mail participates.
