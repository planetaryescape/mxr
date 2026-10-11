# Reviewed sends retain outcomes after disconnect

The reviewed S03 candidate adds a durable send operation to the saved-draft
pipeline. The completion check uses an isolated fake-provider CLI send whose
response is lost, followed by an outcome query after draft cleanup and daemon
restart with one provider acceptance. The first observed `accepted` query
already had a local message ID;
a separate injected ingest failure proves that `accepted` can remain recorded
without one. A provider acceptance followed by an error, process interruption
or receipt-write failure remains uncertain and blocks another send.

## Review supplies the revision and query identity

```sh
mxr send DRAFT_ID --dry-run --format json
mxr send DRAFT_ID --expected-revision REVISION_FROM_PREVIEW \
  --operation-id OPERATION_ID_FROM_PREVIEW --format json
mxr send-outcome OPERATION_ID --account ACCOUNT_ID --format json
```

The preview supplies the identity and reviewed revision. Submission prints its
query identity before effects. JSON/JSONL stdout remains structured. Scheduling
also requires that revision and refuses to follow later edits silently. Querying
requires the owning account even when the draft no longer exists.

Web retains operation references before HTTP submission and exposes them in
Drafts. The TUI retains its latest operation and per-draft revision atomically in
`tui-send-recovery.json`, independently of delayed palette-recents saves. Edited
retries conditional-update the reviewed saved revision before sending; a newer
saved revision refuses the attempt. The TUI exposes a
palette query. Send, safety preview, provider save and scheduling use the same
compose-file draft identity; those alternate actions cannot clone an uncertain
submission into another resource. Provider save and scheduling keep the local
compose file so an unsuccessful request cannot destroy its recovery text. MCP requires a caller-retained operation ID and reviewed revision
and provides `mxr_send_outcome`. A deliberate new Send press may create another
operation only after querying durable `not_accepted` evidence. Unknown acceptance
cannot be cleared by an orphan reset, startup maintenance, scheduling or a new ID.

## SQLite records uncertainty before provider execution

Migration 073 adds `draft_send_outcomes` without a draft/account cascade. It
backfills accepted legacy receipts and marks legacy Sending rows unknown. It
retains operation identity, account ownership, reviewed revision, a canonical
content command digest and provider receipt identifiers; it stores no mail body.
Migration 074 captures the reviewed scheduled revision.

The draft claim and unknown outcome commit together before the provider call.
Adapter-confirmed non-acceptance may durably return the draft to editable state.
An accepted provider receipt commits before local Sent ingestion, analytics or
cleanup. Those local follow-ups cannot turn acceptance into a retryable failure;
the local message ID is optional. Outcomes have no age-based pruning.

SMTP/Outlook classify local construction failures before submission as definite
non-acceptance. Transport submission errors remain unknown. Gmail submission and
post-HTTP parse failures remain unknown. Adapters own this evidence through
`MailSendProvider::send_classified`; the default is conservative uncertainty.
The fake adapter provides explicit rejection, post-acceptance error and pause
markers, plus synced identifier-only acceptance files observable across restart.

## Mixed versions refuse unsafe send requests

IPC protocol version is 5. The shared client probes `send_outcome_supported`
before send, outcome-query and scheduling requests. An older daemon receives no
such effects. The new daemon refuses legacy send requests without reviewed
identity and schedules without reviewed revision. Existing successful receipts
remain queryable as `legacy:DRAFT_ID`; legacy in-flight rows stay uncertain.

Migrations are additive and rerunnable. The supported recovery posture is an
upgrade or forward fix. Older daemon code that cannot understand these persisted
send outcomes is not a safe rollback target.

## Focused checks establish the failure boundaries

The daemon regressions cover simultaneous submission and replay after cleanup,
command conflict, stale review, missing client identity, provider acceptance
followed by error, claim rollback, acceptance receipt-write failure, optional
local ingest failure, proven rejection followed by explicit retry, and a
scheduled draft edited after review. Store tests reopen persistent SQLite and
retain ownership/outcomes after deletion. Client compatibility fixtures ensure
an unsupported daemon receives only the capability probe.

The consumer reproducer is `scripts/verify-send-outcomes.py`. It takes a built
binary and creates a new isolated runtime; it uses only synthetic addresses and
the fake provider. It owns its daemon PID and port, leaves its runtime/evidence
for inspection, and never reads credentials or a real mailbox:

```sh
python3 scripts/verify-send-outcomes.py --binary target-cli/debug/mxr \
  --runtime /tmp/mxr-s03-new-runtime --evidence /tmp/mxr-s03-new-evidence.json
```

It exercises lost IPC response, an accepted query after local indexing, restart,
authenticated HTTP query after cleanup, uncertain acceptance, a process crash
before receipt persistence, and an unchanged reviewed retry after proven
non-acceptance. A separate injected ingest failure verifies acceptance without
a local message ID. The acceptance counter is external to SQLite and survives
restart. Reminder JSONL remains pipeable, and reminder-setup failure does not
change the recorded accepted outcome.
Root owns the installed-release rerun and release verification.

## Inline review keeps this send-specific

The simplification pass preserves the existing send choke point and provider
trait, with a separate store file and TUI recovery helper. It adds no generic
operation engine. Typed IPC and HTTP decoding cover wrong-type inputs; stale
revisions, operation collisions, lock lifetime, persistence ordering and retries
have focused coverage. Web retains references before dispatch and preserves the
selected revision through the undo window. Explicit unchanged retries after
known rejection are covered; uncertainty never creates a fresh attempt.

The TypeScript review checked JSON narrowing, optional revision refusal, retained
identity through errors, and the undo lock/dispatch lifetime. The changed web
tests follow the repository's established API-module mocking convention; the
anti-slop checker reports that convention once, plus native JSON boundary
narrowing, generated header dictionaries and the established unknown error boundary.
No schema library is installed in the web app; adding one solely for the checker
would change repository conventions. Generated code is regenerated from Rust.
Replacing these conventions across the suite is outside this slice.

Two failed assumptions were corrected: migration files need explicit registration
in this store, and a missing provider is a pre-claim failure rather than evidence
of a claimed send outcome. The first accepted consumer query already contained a
local message ID; the no-local-ID accepted state is established separately by
the injected ingest failure.

The implementation began from freshly fetched main
`a928c63f9a0fd3c2c9e3e78fdbb157695ebf8b80`. Exact candidate identity, gate results,
measured timings and consumer evidence are recorded in the implementation handoff.
