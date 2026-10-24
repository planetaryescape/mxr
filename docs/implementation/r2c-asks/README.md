# R2c has stored projections and still needs acceptance

Continue in the canonical worker checkout and `target-cli`. The candidate
is unfinished. Canonical contracts and focused storage, lifecycle and client
checks have passed, but the full classifier still fails its Send precision
floor. That failure and the remaining integration checks prevent acceptance.
The accepted contract remains [blueprint 37](../../blueprint/37-what-it-asks.md).
Do not merge this candidate as completed R2c.

## Inspect the stored answer and correct a task

```sh
mxr modes why MESSAGE_ID --format json
mxr todo why TODO_ID --format json
mxr todo correct TODO_ID not-mine --dry-run --format json
mxr todo correct TODO_ID not-mine --format json
mxr todo undo-correction TODO_ID CORRECTION_ID --dry-run --format json
mxr todo undo-correction TODO_ID CORRECTION_ID --format json
mxr todo correct TODO_ID wrong-details 'title=Sign the right contract' --dry-run
```

These commands share the daemon correction path. The parent has exercised
all four reasons, previews and undo through isolated fake-mail clients. `not-a-todo`, `not-mine`, `already-done` and `wrong-details`
are separate durable reasons. Wrong details takes field edits; opening its
editor saves nothing. Quotes remain source evidence. The returned
`correction_undo` token restores the prior row, including its fields,
state and provenance, unless newer work has changed it.

CLI, IPC, HTTP bridge, MCP, web and TUI use the same correction requests.
The assistant can preview `mxr_todo_correct` and `mxr_todo_undo_correction`
without approval. Committing either still asks for approval, including in a
trusted conversation. Step titles distinguish previews from calls that ran;
they do not repeat edited fields or claim that a correction succeeded.
The web and TUI `X` paths choose a reason; TUI choices use 1–4 and Enter.
Web rows keep the requester and exact quote on one truncated secondary line.
The full quote remains available in its title, details and source reader.
Eligible ask uncertainty is a separate `might_need_you` collection on the
runway. The web disclosure starts closed; TUI `m` opens its source list.
No uncertain row becomes a Now task.

## Home and asks commit together

`asks-r2c-8` covers received and sent messages in the existing assessment
storage envelope. `r2-1` remains the inbound home policy. Home-version
reports exclude the reserved asks namespace, and outbound asks cannot
create a home. Both facets share one generation CAS, transaction and dirty
acknowledgement. A shared read takes both facets from one SQLite snapshot.

The context token hashes the same immutable TZif bytes or POSIX definition
that Jiff resolves, including future offset rules. Named IANA zones, system
local time, absolute TZif paths, and POSIX `TZ` forms share this path.
Chrono date arithmetic retains the snapshot through its offset adapter. Reads reject a mismatched context before
reconciliation. The bounded reconciler also detects context changes.
Unknown or unreadable timezone files need recovery; they are not a reason
to show an answer from another timezone.

The materializer groups threads explicitly, orders messages by SQLite
storage sequence and prefers a preceding `In-Reply-To` match. It supplies
one predecessor and full cached body parts, including HTML text fallback.
Desk snippets are not the evidence boundary. Owner matching uses
`OwnedAddresses`, including send-as aliases.

Policy `asks-r2c-8` also distinguishes an explicit salutation to another
person from an owner request. Name-only salutations attach locally; an
addressed action cannot veto an independent unit. An adjacent cancellation
of the same actor's reply question is re-read as one verbatim span, including
“No need to reply”, without cancelling separate action tasks. Policy4, Policy5, Policy6 and Policy7 answers
remain retained history and read pending until the current policy is stored.
Coordinated future actions retain their speaker: a correspondent saying they
will rework a plan and send a revision does not ask the owner to send it.
Security-notice wording only suppresses the verification task; independent
actions and ordinary email activation remain eligible.

Singleton asks reads start with the distinct requested message IDs before
joining assessments. This keeps per-message projection from walking the
account's assessment index. Account, policy, effective generation and timezone
checks still apply. The native full-schema lookup regression measures SQLite
VM work at 1,000 and 10,000 rows; the full-set test reports sync and
classification stage times separately.
Timezone resolution reads the current source on every call. A single cached
snapshot reuses parsed rules only when the raw override and complete bytes
match; changed, missing or corrupt files cannot reuse old TZif rules.

Mode responses carry bounded summaries: policy, fingerprint, unit tags,
candidate values, exact quotes when within the response limit, and
message/zone/span provenance. Full zones remain in local storage. A summary
reports omitted units; membership reports additional omitted messages.

## Tasks and commitments project the persisted answer

The To do scan no longer calls the legacy first-hit detector or mirrors
background model commitment rows. Explicitly recorded promises and draft
promises accepted by sending retain their existing user authority. Their
projection checks the accepted record and date provenance in the write CAS;
model-written or unknown dates stay visible as unverified and never drive Due
or Now. User-edited task dates survive later provenance changes. It projects owner requests and owner promises from
stored tags. Reply-only asks gate Your turn; owner requests and their
commitments gate automatic Waiting on. Explicit timers retain their
existing authority. Relationship commitment projection commits with asks;
background model commitment extraction is no longer scheduled. Optional
compose safety and draft promise proposals remain separate existing paths.

Commitment history reads retain legacy records, including unverified dates.
The explicit open view still requires current stored asks, a user-recorded
promise or a retained draft promise promoted after sending. A cached source
must belong to that account; missing cached Sent mail retains promotion recovery. History retention does not put retired model detections into Waiting on.

Task identity uses the full counterparty address, action, explicit reference
or action object and due month. Payment reminders and failed-payment
escalations share the payment identity. A reminder without a date only
reuses an unambiguous existing period. New automatic rows wait for bounded
legacy-row reconciliation; manual edits, schedules and completed/dismissed
history are protected. Source replacement hides stale automatic rows until
the matching stored answer is projected. Withdrawn automatic asks expire;
confirmations still offer completion rather than guessing that work is done.

Migration 98 adds correction snapshots with a reason constraint. Backup
inventory and the old-schema comparison include its inverse. Schema 99 adds
durable, generation-scoped projection retries. Wire protocol and the web
expectation are 26. The stored evaluator reads linked task fields from the
same SQL snapshot as their ask provenance. Incomplete bodies and unsettled
projection retries remain pending; a missing task after projection has settled
counts as a miss. User corrections are excluded from classifier field scores.
OpenAPI and generated TypeScript come from the canonical exporters. Schema
and CLI help snapshots require their corresponding test generators and
review before integration.

## Run the focused gates before accepting the candidate

```sh
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr-todo --tests asks::
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr-store --tests asks::
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr --tests ask_assessments
```

Run heavy work only when the shared memory gate permits it. Keep the inner
four-worker environment and reuse the existing target. After the foundation
passes, run correction, migration/backup, concurrency and touched-crate
tests; the canonical OpenAPI/types/reference generators; web component
checks; the evaluator with unchanged floors; and `cargo build -p mxr`.
Use only synthetic mail for the public CLI and client journeys.

The evaluator reads persisted asks and reports zone, thread-position and
abstention slices. It does not repair state or call a model. Its metrics,
quote/actor/state coverage and held-out corpus acceptance remain unverified.
The week of counts-only real-mail dogfood remains pending and is outside
local development/smoke authorization.

## Projection work survives a committed assessment

Migration 99 adds a bounded projection outbox and legacy obligation links.
Writing an asks facet enqueues its source, fingerprint, timezone context and
thread generation in the same transaction. A successful matching projection
acknowledges that ticket; an error leaves it queued across restart. A stale
acknowledgement cannot consume a later generation, even when its fingerprint
is unchanged. Settled reads select without taking the writer.

Reconciliation continues through projection-only pages after thread work has
finished. Each replay batch acknowledges at most 100 tickets and retains failed work.
The background loop yields between projection-only continuation pages. A settled call performs no writes. Synthetic grading
settles through this same bounded seam before taking its read-only packet;
reconciliation is not part of the production evaluator.

Legacy reconciliation blocks only the corresponding source and task kind.
When one old row could represent several actions, untouched automatic work
expires and the stored actions can project separately. Edited or closed
history retains its details and records all possible successor keys as
suppressed; no arbitrary successor inherits the user's decision. Missing
source bodies remain pending for that source. Completing reconciliation
requeues the source projection atomically.

Calendar RSVP uses the accepted stored invite selection (`REQUEST`, RSVP
requested, `NEEDS-ACTION`). Its stored unit is a request to reply, with a
calendar source zone and ICS provenance. The task remains RSVP; an invitation
is not evidence of a promise to attend. Existing PARTSTAT completion remains.

The evaluator reads envelopes, current home and asks facets, and record
membership in one SQL statement. Label schema 3 adds exact quote, actor and
state expectations when a scenario supplies them. Existing labels and floors
remain; the extended corpus and its measurements still need acceptance.


## Round4 repairs measured stored-ask regressions

Policy `asks-r2c-4` invalidates earlier draft answers after the quick evaluator
exposed missing common bill, signing and promise readings. Yearless deadlines
remain marked unchecked while contributing a resolved candidate to a complete
bill. The common money parser reads decimal-comma and grouped EUR/ZAR amounts
without accepting a shorter numeric prefix. Past-due candidates retain the
original tense when they use the shared date resolver. Word durations use the
existing natural-time grammar rather than a second date parser.

RSVP tasks keep their stored Reply action. The evaluator's historical `attend`
category is the retained RSVP reporting bucket, including structured calendar
responses, rather than a claim that the owner promised to attend. Ordinary
reply-only asks remain Your turn. Tasks born expired are excluded using stored
projection facts in the same read-only SQL packet. No label or floor is lowered.

The full To do crate tests passed during this round; final stored evaluator,
provenance, held-out, migration and release acceptance still require the exact
candidate's recorded results. To inspect the synthetic quick set's retained
facts, units, votes and candidates while diagnosing a failure, run its focused
test with `MXR_CLASSIFY_TRACE=1` and `--nocapture`.

## HTML facts require an explicitly trusted receiving service

The default receiver trust map is empty. Raw `Authentication-Results` headers
alone never authorize structured bill fields. After the receiving server’s
administrator has confirmed that its border removes forged reports claiming
its own authserv-id, configure that service for the exact account ID through
`mxr config edit`:

```toml
[todo.trusted_authentication_services]
"ACCOUNT_ID" = ["mx.example.net"]
```

Restart the daemon to apply an edited config file. Do not copy a service name
from an email or infer trust from a sender, `Received` header or a pass string.
The first matching configured receiver report is authoritative, including a
failure; mxr requires a parsed aligned DKIM or SPF pass. The stored fact says
that the receiver reported alignment, not that mxr performed cryptographic
verification. Account trust changes invalidate cached answers before reuse.
The raw HTML and exact report remain local provenance. A structured due date
can supplement one authored bill without changing its exact ask quote, and
cannot turn an unrelated offer into a task. Link actions still open the email;
receiver trust never enables one-click payment.

This follows [RFC8601’s trust boundary](https://www.rfc-editor.org/rfc/rfc8601.html#section-1.6)
and [Gmail’s aligned sender requirement](https://developers.google.com/workspace/gmail/markup/registering-with-google).
The parser dependency uses `msg-auth-status` 0.2.0 with `std` and `mail_parser`:
its upstream feature gate otherwise fails to compile its `HeaderValue` adapters.
Only that compatibility edge uses `mail-parser` 0.9.4; the workspace parser remains
0.11. No verifier, DNS, crypto or network feature is enabled.

Schema 100 persists receiver authority per account and global shape/timezone
context. Startup and the background reconciler compare the durable values;
unchanged authority uses reads only. A receiver change advances only that
account’s generations, while an actual global context change advances all
accounts. The first migration without a marker can invalidate once. Receiver
IDs use the same lowercase/sorted/deduplicated policy in matching, fingerprints
and persisted authority; whitespace is preserved and never silently trusted.

The native authority tests pause materialization at its existing commit barrier
and queue a trust-revocation writer at the config gate. Publication finishes
under the old authority, then revocation makes that account pending without
retiring another account’s answer or task. The persistent-startup test opens
`AppState` twice with the same configured receiver and checks the actual first
and later reconciler ticks for unchanged answers, outbox and writer counts.
Separate shape-only and timezone-context-only changes test both accounts’
generations without modifying process-wide timezone settings. Run these proofs
with:

```sh
cargo nextest run -p mxr --lib -E 'test(handler::tests::assessment_authority::) | test(receiver_revocation_waits_for_preparation)'
```
