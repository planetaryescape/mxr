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

The CLI's Archive request continues to honor `MXR_IPC_TIMEOUT_SECS`. In the
browser, Archive refreshes mail queries before settling the early Undo claim on
both success and partial failure, so Undo does not appear ahead of the refreshed
mailbox.

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

The final integrated candidate currently under review is
`f3986183c92f532e125304cb15c3a70eb9c44c7c`, tree
`d2ccd484e206b414be52668954104a0075e3082a`. It is not merged or released.
The [candidate evidence record](evidence/s04-f3986183-integrated-acceptance.md)
pins the binary, runtime checks and still-pending release gates. Root owns
integration, independent review, release and published-artifact acceptance.

Focused daemon tests covered lost replies, concurrent submission, command and
identity conflicts, stale item binding, acceptance/dispatch/success receipt
failures, partial rejection, interrupted Pending/Unknown items, retention,
local reconciliation failure and intervening-label Undo. Client tests consumed
a lost submission response and recovered the same journal identity. The named
older-daemon fixture is v0.6.68 Status without Archive capability; it refuses
new requests while existing draft and send surfaces still proceed. This is not
a claim about arbitrary version compatibility.

The candidate's isolated CLI/IPC/HTTP harness passed seventeen recorded checks,
including concurrent replay, changed-payload refusal, restart recovery,
unknown outcomes, and receipt failures. Its actual MCP harness passed three
checks for preview confirmation, restart recovery and no-repeat handling of
unknown outcomes. Both used the same candidate binary and synthetic provider
acceptance witnesses. The process-crash probe killed only its owned daemon; the
CLI and MCP harnesses reported their owned processes stopped. These checks do
not establish recovery after an operating-system crash or power loss. The
candidate evidence record names each check and its source manifest.

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
reconciliation workflow are tracked in
[`archive-operation-recovery-followups.md`](../../issues/archive-operation-recovery-followups.md).
That issue also records the current per-account batch behavior after an unknown
result, panic cleanup, and the SQLite power-loss durability boundary.

The final candidate binary build took 88 seconds, and the SPA build took 3.95
seconds. The focused verification passed fourteen unit tests and two E2E tests;
typecheck and lint also passed. These measured command durations are not a total
wall-clock or agent-time estimate. CI, whole-diff review, final TUI/browser
acceptance against the release candidate, OpenAPI regeneration and published
artifact checks remain separate gates; see the evidence record for the current
status.
