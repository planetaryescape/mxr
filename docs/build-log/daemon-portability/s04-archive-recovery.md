# Archive keeps the reviewed selection and per-message outcome

Preview an Archive selection, retain its operation ID, then submit that exact
review. If the response disappears, query the retained ID. A query reads state;
it never dispatches mail. Submission with that identity resumes only items whose
dispatch intent has not been committed.

```sh
mxr archive MESSAGE_ID --dry-run --format json
mxr archive-operation OPERATION_ID --submit --format json
mxr archive-operation OPERATION_ID --format json
mxr archive-operation --format json
```

List JSON is one array, including an empty array. JSONL emits one receipt per
line. Submission requires an explicit operation ID, so listing retained previews
cannot submit abandoned reviews. Normal Archive uses the same preview and
retention path; `--async` returns its recoverable operation immediately. CSV is
refused before preview or submission. Synchronous submission prints the retained
receipt and exits nonzero when no item has confirmed completion; queries,
lists and asynchronous acceptance preserve their success status.

The TUI command palette offers **Preview Archive selection**, then query or
submit the retained ID. Normal mailbox Archive also retains its identity before
submission and does not automatically create a retry identity. Desk Done and
Updates/Reading Done keep their existing meaning. The browser retains reviews
in local storage before submission, offers Archive recovery after reload, and
uses the same route for selections above the old 200-message job threshold.
MCP exposes `mxr_archive_preview`, `mxr_archive_submit` with explicit confirmation,
and side-effect-free `mxr_archive_outcome`.

## The dispatch receipt commits before the provider effect

Migration 075 stores workspace identity and recovery generation. Reviewed
Archive records live in the existing mutation-job table with immutable caller,
operation, review, command selection, account and provider ownership. Account
scopes apply to preview, submit and outcome queries. The generation boundary
refuses identities from another workspace or an explicitly rotated generation.
Restore tooling must rotate this generation when it creates a new recovery
lineage; the store exposes that operation, and this slice adds no restore flow.

Each item starts `pending`. Under the provider/account guard, the runner checks
its account, provider ID and Inbox membership again. A stale item becomes
`not_executed` before dispatch. Unrelated flags and labels do not invalidate the
review; the Undo snapshot captures the labels present at dispatch.

A compare-and-swap commits `unknown` before calling the provider. Provider
success commits `provider_succeeded` before local label reconciliation, then
`completed`. Only an adapter's explicit non-acceptance becomes `not_executed`.
Generic errors, timeout, interruption and failure to retain success stay
`unknown`; same-identity submissions never call the provider again for those
items. Runtime runner claims avoid redundant workers, while durable per-item
transitions enforce effect ordering across restart.

Accepted and recoverable job rows survive the old pruning window. Reviewed
rows remain visibly unaccepted. Client journal failure prevents submission.
Existing Undo links use the internal mutation ID, and browser partial results
retain that link while naming uncertain effects explicitly. The TUI retains
partial Undo for confirmed completed items even when other items fail. A terminal
same-identity submission cannot recreate a consumed or expired Undo entry.

A successful acceptance compare-and-swap records `mail.archive` through the
existing local activity recorder, with the original client source and
`phase: accepted`. This is user intent, not a provider-completion claim. Replayed
submissions and queries do not record another activity. The recorder remains
best effort and respects `MXR_ACTIVITY=off`.

## The checks use synthetic provider acceptance witnesses

Initial implementation base: `35c4f7232125cd595bc2169abbf951152a7aa06d`, tree
`ebda2a4070d577e4a690fedf7b21bfd23e1e538c`. The implementation was rebased onto
`49e40f036ad83d68e09cf3d9de6e016d877d6ccc`; root owns the final integrated source,
independent review, release and published-artifact acceptance.

Focused daemon tests covered lost replies, concurrent submission, command and
identity conflicts, stale item binding, acceptance/dispatch/success receipt
failures, partial rejection, interrupted Pending/Unknown items, retention,
local reconciliation failure and intervening-label Undo. Client tests consumed
a lost submission response and recovered the same journal identity. The named
older-daemon fixture is v0.6.68 Status without Archive capability; it refuses
new requests while existing draft and send surfaces still proceed. This is not
a claim about arbitrary version compatibility.

Root's isolated CLI/IPC/HTTP harness passed twelve recorded checks with persistent
fake-provider dispatch and acceptance witnesses. It killed only its own daemon
for the crash probe and stopped owned processes afterwards. The immutable first
binary's SHA256 was
`55650c365da08d5189161a61024c8d22d50a474b36a868df07cf33f86de402d7`.
Runtime evidence is retained under `target/s04-evidence/`, including
`root-fourth-cli-ipc-http.json`; final TUI/browser evidence and binary provenance
are added to the handoff after their checks complete.

The fake spool opt-in uses `mutation-reject`, `mutation-uncertain`, and
`mutation-pause-after-accept`. Empty controls apply to all messages; otherwise
file contents select one provider message ID. Pause lasts at most 30 seconds.
`mutation-dispatched-*.json` and `mutation-accepted-*.json` contain internal
mutation, account and provider message identifiers, with no mail content.

## Unknown effects require reconciliation

The slice does not infer non-acceptance from provider error strings or replay
uncertain effects. `provider_succeeded` reports incomplete local reconciliation;
query and resubmission do not repeat its effect. There is no generic repair,
rollback, scheduling, Send-and-Archive atomicity or provider idempotency claim.
Reviewed rows have no cleanup policy yet; retention limits and a later explicit
reconciliation workflow are follow-up candidates.

Effort comes from the parent dispatch and tool transcript timestamps. The exact
wall-clock, agent/wait split and final source/runtime identities belong to the
completion packet; worker time is a delivery lower bound. Initial exporter build took 28s,
first binary build 56s, focused incremental daemon verification 17s, and the SPA
build 4s. Failed checks exposed fixture/API mismatches and lint issues; they did
not justify broader local workspace runs. CI owns the workspace gate.
