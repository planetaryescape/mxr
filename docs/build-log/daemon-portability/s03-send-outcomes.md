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

## The mail-merge companion checkpoints recovery before dispatch

Candidate `8e1289708bd835b3a410683c0c90f9e299414565` updates the shipped
`mxr-mailmerge` companion to preview an initial send or schedule, or a permitted
retry after confirmed rejection, and save the account, reviewed revision,
operation ID and delivery time in its campaign manifest before invoking the
real `mxr` binary. A retained command reuses its saved identity while checking
the outcome. Immediately after an immediate send, it queries the exact reviewed
account and operation ID. On a later campaign run for an immediate send,
reconciliation starts with the account-scoped `draft:DRAFT_ID` alias and checks
the retained operation ID when needed. An accepted result can reconcile the
manifest after the provider response was lost and the draft was deleted. `unknown` and outcome-query errors remain
visible as uncertain; `--retry-failed` does not authorize resending them. A
confirmed `not_accepted` result becomes failed, and an explicit
`--retry-failed` obtains a fresh preview and operation ID before another
attempt. This is durable recovery evidence, not an end-to-end exactly-once
delivery guarantee.

A lost schedule acknowledgement remains `schedule_uncertain` with the reviewed
delivery time retained. Query `mxr send-outcome 'draft:DRAFT_ID' --account ACCOUNT_ID`;
a missing result is not proof that scheduling was rejected, and
the companion does not automatically reschedule it. Retry that retained
schedule only after confirmed `not_accepted` and an explicit retry; initial
scheduling does not require a previous rejection. Existing manifests with no
review object remain readable, but a legacy attempt without a retained daemon
outcome has no recoverable operation identity and requires reconciliation before
retry.

The TUI will not reopen a compose file while send, provider save, scheduling or
asynchronous cleanup owns it. Other retained files remain available. A late
identity update for file A preserves file B's newer recovery selection.

With v0.6.68 or later installed on `PATH`, run the reusable synthetic consumer
check with both installed binaries:

```sh
python3 scripts/verify-mailmerge-outcomes.py \
  --mxr "$(command -v mxr)" \
  --mailmerge "$(command -v mxr-mailmerge)" \
  --runtime /tmp/mxr-mailmerge-outcomes \
  --evidence /tmp/mxr-mailmerge-outcomes/evidence.json
```

The intermediate consumer-02 run recorded four provider acceptances in
11.185 seconds. It exercised immediate acceptance/recovery, confirmed
rejection and explicit retry, uncertainty across restart, and scheduled versus
schedule-uncertain manifest states. This synthetic run is not final candidate
CI or release acceptance. Its recorded binary SHA-256 values are
`d8e54b04e9edc73f7ebc4c62eef12ec67c23ae0c9b5510cf83853523303da09f` (`mxr`)
and `141688284db63920577b28948512afca45249ca70b5794e4b0576c2bda9f5e43`
(`mxr-mailmerge`). Root owns the final focused gates, whole-candidate review,
release and installed verification.

## The reviewed candidate passed CI and merged

Candidate `04a72f7bef8dcbebb3aa4838b15c855247566348` (tree
`c66ddc36ebbf5d7276bf7b0674ac07c0a8bb97cf`) merged as
`d6ba93ba7082a644e027b331e67da44d8e6dd4e8` with the identical tree. CI run
[38146965743](https://github.com/planetaryescape/mxr/actions/runs/38146965743)
passed in 924 seconds. It reported 4,025 fast tests passed with 9 skipped,
4,071 semantic tests passed with 10 skipped, 897 web tests across 158 files,
and workspace Clippy in 91 seconds. The browser job reported 380 passing tests;
the unchanged people-pane assertion observed 452 pixels against 453 and passed
on retry. Four speed checks passed in 23.8 seconds.

The fresh whole-candidate Opus 5.5/max review returned GREEN after root triage
in 29m51.603s, 9m51.603s beyond its 20-minute budget. Root accepted six
bounded findings for documentation or later diagnostic follow-up; none blocked
the merged candidate. The source-correction window from the consumer finding
at 13:59:11 UTC to root acceptance at 14:25:27 UTC was 26m16s wall, including
implementation, checks, packaging and review. These elapsed intervals overlap;
worker time and BK attention were not measured.

The final local companion consumer used immutable candidate
`2b9e731409e207a226823c0c2450bbfeb55dfaaa` (tree
`869d34b9b59cee4a01d565a291810bcc4ec9948d`). It completed in 11.480572s with
four provider acceptances and seven durable pre-effect witnesses. The binary
SHA-256 values were `911af44b54dfd5271ff594ebe41a788df6648c5ea4f0c60d169e605fa89c41ce`
for `mxr` and
`32fbb611269f87cab18b1ba15e67bcca3d449864ebb07f756415c7245ab3cce3` for
`mxr-mailmerge`. Root verified the evidence digest and all 53 worker-packet
hashes. This synthetic consumer check is not installed-release acceptance.

The final local TUI replay on the same candidate refused reopening the compose
file while send owned it, kept one editor open, and removed the file once after
an Accepted result. SQLite retained Accepted, and the owned ports were closed.
The earlier candidate's second-editor/FileNotFoundError observation is
superseded; the final replay did not establish edited-text loss.

The merged source was released and checked through installed CLI, companion,
TUI and browser journeys as recorded below.

## The first release attempt failed at a workflow path

Release source `0d00cb2d644b0b914c45ac9508b12fe4daa0b706` for v0.6.67 was not
published. [Run 38149908274, Release CLI Smoke job
114501057493](https://github.com/planetaryescape/mxr/actions/runs/38149908274/job/114501057493)
compiled with `.cargo/config.toml`'s `target-cli` directory, then called
`verify-mailmerge-outcomes.py` with `target/debug/mxr`; the script failed with
`FileNotFoundError` at line 30. The earlier v1 launch proof and provider smoke
passed; the companion proof did not run. This was a workflow path error, not a
send-recovery failure. The v0.6.67
tag remains failed and unpublished; it was not retagged.

Workflow fix [PR357](https://github.com/planetaryescape/mxr/pull/357) merged as
`86c95992`. Release metadata [PR358](https://github.com/planetaryescape/mxr/pull/358)
set v0.6.68 at source `621e9e49739eeb80abbccea0916578420941c3fc` (tree
`6d391d9d90db51339d9a8d100e50915732a895eb`). [Release run
38151840433](https://github.com/planetaryescape/mxr/actions/runs/38151840433)
completed successfully from 15:34:28 to 15:58:17 UTC and published v0.6.68 at
15:56:03 UTC. The release took 23m49s; native builds took 18m55s on macOS and
14m28s on Linux. The archive checksums and installer/Homebrew binary hashes
match. Installed CLI and companion checks passed with 50 CLI calls, eight fake
provider acceptances, and four companion acceptances in 9.366s. The [curated
release and installed evidence](evidence/s03-v0.6.68-release-installed.json)
records the source, asset hashes, installed binary hashes and check summary.
No real mail was sent.

The installed TUI acceptance passed using the same v0.6.68 binary hash. Four
journeys covered provider save and stale-revision preservation, explicit retry
after rejection, same-file ownership during a send, and retained-Unknown
recovery after restart. The run took 385.907 seconds; owned ports 18414 and
18415 were closed afterward. The [curated TUI packet](evidence/s03-installed-v0.6.68-tui/README.md)
keeps selected database fields, visible message excerpts and source
hashes. SQLite showed the canonical provider-draft link, but provider memory
was not inspected.

The installed browser journey also passed on the same v0.6.68 binary. The fake-
only run took 8m39s. After reload and an explicit query, the injected A outcome
remained Unknown and another send was refused. B's definite rejection retained
its draft; an explicit retry used a distinct operation and was accepted. Root's
read-only SQLite query confirmed Unknown, NotAccepted and Accepted identities;
the final spool contained two fake acceptance records, and the scoped storage
key and outcomes survived browser and daemon restart. The [curated browser
packet](evidence/s03-installed-v0.6.68-browser/README.md) contains the corrected
observer summary and selected restarted-Drafts screenshot. Its three distinct
send observer entries were 502, 502 and 200 for A, B's rejection and B's retry.
These are deduplicated observer event indexes, not server request IDs. The
storage key was captured after a fake acceptance record existed and before the
timeout response; it does not prove retention before provider effect. The
intermediate spool counts were observed during the run but were not saved
individually. Browser errors output was empty; three UDS warnings remain
unclassified. No real mail or credentials were used.

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
