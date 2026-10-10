# Draft revisions preserve both editors' text

S02 adds durable local draft revisions across daemon IPC, CLI, TUI, browser and
MCP. The completion check is one stable draft saved, restarted and read, followed
by concurrent edits where one succeeds and the stale editor keeps its text.

The local revision starts at 1 and advances through SQL compare-and-set updates.
It is separate from the provider revision. Save and update return the canonical
saved draft; list and read expose its revision. Repeat creation with the same
identity and content is safe, while different content conflicts. Update, delete
and existing-draft provider sync check the expected revision before provider
effects. Provider sync uses saved canonical content.

All content/status writers take the draft lock before the account provider lock.
Reconciliation re-reads the link under that order and advances the local revision
when it pulls provider content. Send and orphan recovery share the draft lock;
cached reply headers use the current revision without advancing content state.

CLI editor files have unique names; conflicts report their preserved path.
Browser autosave retains one identity in private session metadata. The bridge
persists each submitted snapshot before replacing its shared scratch file. A
conflict preserves the browser buffer and writes a unique private recovery file.
Accounts can be selected before first persistence; a saved draft keeps its
account. Stored HTML stays read-only in the browser composer, and existing HTML,
inline assets and reply metadata remain covered by the draft tests.

New clients negotiate `draft_revision_supported` before draft writes. A new daemon
rejects unconditional edits/deletes. Reads remain compatible. An older binary can
read the database but cannot enforce revision safety, so downgrade is unsupported;
use a forward fix. Close and reopen clients around an upgrade: already-running
old clients cannot safely write, and old draft-creation handlers may reject the
canonical response after the creation commits. Pre-upgrade browser windows lack
session metadata; retain their text and copy it into a newly opened compose
window before saving. No revision is inferred for the old window.
Daemon-wide reviewed-send tokens and idempotency remain S03. S02 checks the
observed revision on revised browser sends sharing a compose path.

## Verification uses synthetic content

Run the built or released binary without real accounts:

```sh
scripts/smoke-draft-revisions /path/to/mxr
```

It prints revision 1 after restart, the accepted editor text at revision 2, the
stale revision error, the preserved editor text and a deletion preview. Its
configuration, database and socket live in a temporary directory. Readiness
requires the owned daemon PID and a live Unix socket; shutdown targets that child.

Focused checks for the candidate:

```sh
scripts/cargo-test -p mxr -p mxr-web -p mxr-store -p mxr-client -p mxr-compose -p mxr-mcp -p mxr-tui --tests draft
scripts/cargo-test -p mxr-web --lib compose_and_scheduling
scripts/pre-pr-rust-gate
cargo build -p mxr
npm run typecheck --prefix apps/web
npm test --prefix apps/web -- src/features/compose src/features/drafts --maxWorkers=4
npm run gen:types --prefix apps/web
```

The race tests hold the account provider lock, wait until draft-lock contention is
observable, and then queue a send or stale deletion. This checks ordering before
provider effects rather than assuming a task yield reached the lock.

The inline code-simplifier pass removed a pass-through saved-response helper and
redundant provider-save insertion/fallback branches. TypeScript review keeps
compose sessions and save/schedule responses derived from generated OpenAPI
schemas; accepted own autosaves advance the local token without discarding newer
in-memory edits. Generated request/success/conflict contracts cover touched compose
and stored-delete routes. The anti-slop checker reports generator-owned response
header dictionaries from openapi-typescript. The final autosave tests use the
existing API module mock convention; lint passed on ea37aa63 and on the round 3 correction. Execution checks remain pending.

## Parent integration remains the next action

The candidate worktree is `mxr-portability-stage-01/mxr`, on
`codex/mxr-portability-draft-revisions`. Rebase uses freshly fetched HTTPS main.
The parent owns independent Claude review, CI, merge, serialized release and the
released-binary smoke. PR335 is open for this branch. It has not been released,
and no real mail has been sent. The shared orchestration ledger remains
parent-owned.

The store's revision guard assumes one active daemon owns the local store; SQL
CAS protects content writes, while process-local draft locks serialize provider
effects. A separate binary that ignores revisions is outside that contract.
Provider-effect/local-SQL crash recovery remains the existing behavior; S02 does
not add an effect journal. Managed draft/attachment lifecycle remains S05.

## Final review corrections need the focused checks repeated

Parent review found that a committed autosave can be marked stale by the browser
request coordinator. Successful queued tasks now advance the accepted revision
before UI result filtering, guarded by session path and draft identity. Two hook
tests cover overlapping saves and a session switch while a save is in flight.

Send and schedule validate the same parsed file snapshot used to construct their
submitted draft. Autosave preserves incomplete drafts; explicit save uses the
existing save validator, including provider draft copies. Invalid submissions
return 422 before effectful daemon IPC. CLI transport errors also report the
unique preserved editor path. The smoke isolates HOME, XDG and MXR settings,
disables the bridge and copies its binary into its private fixture.

Before these review corrections, the root draft suite passed 95 tests, browser
compose/draft tests passed 86 tests, focused store/client/web revision tests and
the Rust gate passed, and the synthetic two-editor smoke passed. Those runs used temporary logs and do not certify the final corrections.
Durable final evidence belongs with the parent integration record.

Repeat the two hook tests, frontend typecheck, full affected
`compose_and_scheduling` suite, focused draft suites, Rust gate, root build and
updated isolated smoke on the final pinned source before integration. Job 4902 completed in an earlier round and is historical. The current scoped Rust gate and isolated dependency install remain queued while the global default group is paused. No queue policy or unrelated tasks were changed.

## Review round 2 preserves recovery paths

TUI editors use a private unique scratch file per invocation. Opening the same
draft again leaves an earlier conflict file intact. CLI editor, read, parse,
validation, transport and daemon errors report the retained file path.

Discard confirms deletion whenever the compose session has a saved revision,
including after autosave clears the dirty flag. Its copy names the local draft
and linked provider copy. Delete-word chords inside editors keep their native
text-editing behavior. Close continues to save and close.

Explicit provider saves serialize with autosave and adopt the canonical revision
before a queued edit runs. A session switch cannot receive another session's
token, and edits typed during the save stay in the editor. Scheduling persists
that same canonical token into the private session metadata before dispatch;
a failed schedule leaves the file recoverable. The browser adopts only the
accepted token on failure before its next queued edit.

Earlier round 2 typecheck passed at a dirty candidate; it does not certify the
amendment. The resumed worker registered worktree-local dependency installation
as task 4917 at 2026-10-10 21:33:21 UTC. At that checkpoint, final Rust/TypeScript checks, regenerated
contracts and consuming browser acceptance remain pending while the queue is
paused. Installation runs from a persistent private staging directory with
lifecycle scripts disabled, then replaces only this worktree's dependency symlink.

The correction pass serializes bridge access to each compose path and constructs
updates from the request's immutable rendered text. Rejected updates retain a
unique private recovery file with their observed revision; the winning shared
path retains its own accepted text and token. Explicit provider save and failed
scheduling return `accepted_draft` only for persistence that this operation
confirmed. Browser error handling reads that evidence (including a connection
error's cause), keeps current editor text, and never refreshes a shared sidecar to
infer acceptance. Revised send/safety requests also carry their observed revision
so a shared-path update cannot substitute another tab's message before sending.
The existing old-running-client upgrade boundary still applies.


On a rejected browser update, the editor adopts only the recovery path carrying
its submitted draft identity and observed revision. It retains newer typed text,
dirty state and the original error. A stable queue key follows that editor's
recovery destination, including an already queued close save. Reopening waits
for the queue to settle and then reads the latest active entry, so it resumes
the last preserved snapshot rather than the shared winning file.


## Recovery preserves one private destination and permits Close

The bridge marks recovery ownership in its private draft metadata. Further edits
reuse that destination and persist locally without canonical or provider writes.
Explicitly reopening a recovery makes another private editable copy, so two
tabs opening the same remembered path cannot edit or discard each other's
file. Earlier preserved copies remain intact; user-driven opens can therefore
leave additional private copies. Autosave creates no further copies after
adopting its destination. Recovery Discard deletes only the marked file and its own attachment directory.
Send, safety, scheduling and provider save reject recovery sessions even when the
caller supplies no revision. Ordinary saved draft deletion still checks its CAS.

The browser tracks locally preserved snapshots separately from canonical saved
content. Confirmed local preservation permits Close and stops unchanged autosave
retries. Unconfirmed connection errors keep text dirty; a confirmed daemon
recovery or explicit save retries it. Queued Close and unmount snapshots follow
the operation's actual recovery path, including after a session switch.

Open saved draft explicitly restores current canonical content while retaining
an Open preserved edits action and its path in the active entry. Both reload
actions recheck identity and text after their final response so intervening
keystrokes cannot disappear. Provider sync confirmations through either MCP
name require the revision returned by the preview and preserve that token in
the daemon request.

Round 3 adds timer, queued snapshot, real Close/Discard/reopen and bridge/MCP
coverage. CI at f8b326ea generated and checked the final contracts, checked all targets,
and typechecked/linted the web client. The affected tests require another run
after the CI corrections below; local gates, build and the two-tab browser
journey remain pending while the shared queue is paused. Two unsaved views may both observe a null token. After the first canonical save,
a stale null token conflicts against the accepted revision before send, safety,
schedule or provider-copy effects. Pre-upgrade sessions without revision metadata
remain an upgrade boundary; S02 does not add the S03 effect journal.


## Recovery copies own bridge-managed attachment bytes

Conflict preservation and explicit recovery opens copy uploaded attachments into
their new private compose directory. Frontmatter, draft metadata and the response
name those copies in the submitted order. Rewriting the header keeps the raw
body and context suffix verbatim, including editor whitespace. Equal basenames retain distinct bytes;
repeated references reuse a copy. The browser applies only this operation's path
evidence, including attachments added while a conflict and Close save are queued.
Discarding an earlier recovery therefore leaves each reopened editor's bytes
intact. User-managed external paths keep their existing lifetime: compose Discard
never removes those files. Failed copying removes the new partial files and keeps
the source payload.

A first-save outage can preserve a draft before any canonical row exists. Save
explicitly retries that recovery with its original DraftId and absent revision.
It first preserves the latest recovery, then prepares a separate compose payload
and owned attachment bytes before submitting the existing SaveDraft request.
Only its accepted response advances the editor to the canonical copy. A lost
creation response followed by different content conflicts without overwriting
the stored row; Open saved draft can then restore it. An uncertain retry retains
its extra candidate privately because canonical storage may reference its bytes.
Close and automatic saves continue to preserve locally without creation retries.

The attachment and unsaved recovery correction has focused bridge and hook
regressions. The exact CI contract run and remaining execution gates are recorded
below; local tests/build and the real browser journey remain pending while the
shared queue is paused.


## CI corrections keep recovery state and test isolation intact

The parent pushed f8b326ea to draft PR335. OpenAPI Conformance run 38092169420
passed bridge integration, mounted routes, the summary snapshot and spec lint.
CI run 38092169276 checked all targets and typechecked/linted the web
client. Its web suite reported 875 tests: 872 passed, three failed, plus one
unhandled teardown error. Those results describe f8b326ea and do not certify
this correction or full draft runtime coverage.

The exact logs are `s02-ci-f8b326ea-clippy.log` and
`s02-ci-f8b326ea-web.log` in the parent research worktree's
`docs/build-log/daemon-portability/evidence/` directory.

- MCP fixture mutex acquisition used denied unwraps. Explicit poison messages
  retain the same failure behavior.
- The deletion-chord fixture queried the Message body label shared by a group
  and textarea. It now targets the textbox role without changing the assertion
  that editing keys never request Discard.
- TanStack passes mutation context after the update payload. The timer fixture
  now checks the first payload directly, retaining path, revision and text
  invariants.
- Confirmed recovery state survived a wrapped connection error, but its visible
  sentence only described daemon availability. That validated branch now also
  states that edits are preserved locally; generic error adaptation and its
  original cause remain unchanged. The fixture uses a real-shaped 503/connect
  wrapper and still checks preserved state, text, stopped retries and Close saves.
- MessageBody's pre-existing iframe measurement timers at 100/500 ms survived
  HtmlDraftNotice test teardown. Production MessageBody is unchanged from main;
  the preview fixtures now own fake timers and clear them after unmounting.

Scoped format/lint checks pass. This correction needs affected execution repeated
in CI and through the paused local queue; local build and browser acceptance
remain mandatory before integration.


## Explicit Save retries the original recovery revision

The review of 75e9225f identified that an existing saved draft could become
stranded after a transient failure: its recovery held an accepted revision, while
the explicit retry only allowed null. Explicit Save now retries either observed
token through the existing request. Some uses UpdateDraft CAS; null uses the
idempotent SaveDraft identity. Automatic saves and Close still only preserve.
The latest private text and independent attachment payload survive conflicts and
ambiguous responses. A successful retry adopts only its own response, guarded
against edits or a session switch during the request. Save reports canonical
success only when the current editor matches its confirmed saved fingerprint.

The same revision equality check now covers shared-file send, safety, schedule
and provider-copy candidates. A fresh null-token editor cannot use another tab's
first saved snapshot for an effect. Both-null unsaved sessions retain their
existing behavior. Fixtures carry revisions from their own update responses.
Bridge, hook and keyboard-Save regressions cover retry success, an intervening
writer, independent attachment bytes, delayed typing and false success feedback.
These regressions await execution; the shared local queue remains paused.

Web CI 38092599600 at 34b2bf3f reported 871 of 875 tests passing, with four
failures at one assertion expecting the old recovery sentence and no unhandled
exception. That assertion now includes the confirmed-preservation sentence.
Those results do not validate the material retry and revision corrections here.
The two request schema descriptions changed without changing fields or shapes;
generated descriptions require the exact next CI OpenAPI artifact. Local Rust,
web, build and two-tab browser gates remain pending.

The review's remaining findings belong to S03 or later followups: deleted-draft
send resurrection, ambiguous persistence/discard outcomes and CLI deletion using
a freshly read revision rather than the preview token. Preserved explicit copies
and the compose lock map retain their existing lifetime trade-offs. None of
these adds an effect journal or changes the settled mixed-version boundary in S02.


## Recovery status and visible Retry use the explicit save path

The UI correction based on review of 5b490ac2 keeps a private recovery distinct
from a saved draft across daemon disconnect and reconnect. Local preservation
clears the canonical save timestamp; clean recoveries report preserved edits.
The visible Retry button now invokes the existing explicit Save action with the
editor's original observed revision. Send and schedule failures point to Retry
or Open saved draft without automatically sending. Mounted fixtures cover
reconnect, visible retry success and conflict, retained latest text and no false
saved label. Hook fixtures retain the recovery warning across reconnect.

Source be6a947f passed both workspace CI suites at run 38094341689: 3979 fast
tests and 4025 semantic-local tests passed, including the corrected override-token
fixture. Nine fast tests and ten semantic-local tests were skipped.
Those results describe the preceding source, not this UI correction; affected
web execution and local build/browser gates remain pending. API callers must
carry their accepted expected_revision after the first save: a null token
conflicts against revision metadata once a canonical save has landed. No Rust,
protocol, provider or persistence code changed in this correction, so the queued
Rust gate at be6a947f covers the same Rust inputs. The parent owns current CI,
independent review and integration.
