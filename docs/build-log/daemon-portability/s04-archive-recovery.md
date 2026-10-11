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

In the mailbox and Reading readers, `e`, `]` and `[` keep their navigation
direction through delayed Archive completion and the resulting list update.
Undo returns to the affected message; successful Archive from All Mail advances
while retaining Undo, and cancelling review leaves the reader route in place.
The corrected and original Reading journeys both passed. `PlaceLayout` now
supplies the live `isListed` and `expectRemoval` context; hook tests cover both
completion orders. No timing race was reproduced. Partial Archive failures
preserve their mutation response for the caller.

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
same-identity submission cannot recreate a consumed or expired Undo entry. A
runner that starts after completion and finds no pending items returns without
rearming an Undo entry the user already consumed.

A successful acceptance compare-and-swap records `mail.archive` through the
existing local activity recorder, with the original client source and
`phase: accepted`. This is user intent, not a provider-completion claim. Replayed
submissions and queries do not record another activity. The recorder remains
best effort and respects `MXR_ACTIVITY=off`.

## v0.6.77 is published and installed journeys passed

Release `v0.6.77` came from `14de28f1187d150e05a9f96f6eec12db60c7c9ab`,
tree `48e865128b372f567ec220a682782b5dadeafdea`. Release workflow
[38172695106](https://github.com/planetaryescape/mxr/actions/runs/38172695106)
completed successfully. The GitHub Release job published the assets at
2026-10-11 20:55:52 UTC; the workflow finished at 20:58:23 UTC. The mxr binary extracted from the macOS
archive matches the installer and Homebrew binaries at
`sha256:6489d22c430a3d42b3aadaf64f347e90a6abf6601f25cfaee3bcb1d37a042e6a`.

The [installed acceptance packet](evidence/s04-installed-v0.6.77/README.md)
records 17 CLI/IPC/HTTP scenarios and 3 actual MCP scenarios against that
binary, plus installed TUI and browser journeys. The TUI previewed, submitted,
closed and reopened the app, then queried the same operation identity with
one provider dispatch and acceptance. The browser confirmed Archive, used Undo,
queried the receipt and reloaded to the same identity with one provider effect.
Both flows used synthetic mail. No effect occurred before browser consent.

The release workflow skipped its live Gmail smoke because test credentials
were unavailable. Linux artifacts were checksum-verified; installed UI journeys
ran on macOS. Process-crash checks do not establish recovery after an operating
system crash or power loss. The [earlier f398 packet](evidence/s04-f3986183-integrated-acceptance.md)
is historical and does not verify v0.6.77.

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

The corrected and original Reading E2E journeys passed (jobs 5706 and 5709),
along with typecheck (5698), implementation lint (5716) and formatting (5718).
Hook tests force both completion orders. API-reference regeneration and the
docs build passed. BK waived independent reviews until Anthropic is available
again. Release, CLI, MCP, TUI and browser evidence is in the installed
acceptance packet.
