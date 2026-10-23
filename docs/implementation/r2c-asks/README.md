# R2c has stored projections and still needs acceptance

Continue in the canonical worker checkout and `target-cli`. The candidate
is unfinished: Rust compilation, canonical generators, evaluator floors,
public fake-mail journeys and the actual UI design matrix have not passed.
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

These are source-defined commands for the candidate, pending runtime
verification. `not-a-todo`, `not-mine`, `already-done` and `wrong-details`
are separate durable reasons. Wrong details takes field edits; opening its
editor saves nothing. Quotes remain source evidence. The returned
`correction_undo` token restores the prior row, including its fields,
state and provenance, unless newer work has changed it.

CLI, IPC, HTTP bridge, MCP, web and TUI use the same correction requests.
The web and TUI `X` paths choose a reason; TUI choices use 1–4 and Enter.
Eligible ask uncertainty is a separate `might_need_you` collection on the
runway. The web disclosure starts closed; TUI `m` opens its source list.
No uncertain row becomes a Now task.

## Home and asks commit together

`asks-r2c-2` covers received and sent messages in the existing assessment
storage envelope. `r2-1` remains the inbound home policy. Home-version
reports exclude the reserved asks namespace, and outbound asks cannot
create a home. Both facets share one generation CAS, transaction and dirty
acknowledgement. A shared read takes both facets from one SQLite snapshot.

The context token hashes the process timezone override and actual tzfile,
including future offset rules. Reads reject a mismatched context before
reconciliation. The bounded reconciler also detects context changes.
Unknown or unreadable timezone files need recovery; they are not a reason
to show an answer from another timezone.

The materializer groups threads explicitly, orders messages by SQLite
storage sequence and prefers a preceding `In-Reply-To` match. It supplies
one predecessor and full cached body parts, including HTML text fallback.
Desk snippets are not the evidence boundary. Owner matching uses
`OwnedAddresses`, including send-as aliases.

Mode responses carry bounded summaries: policy, fingerprint, unit tags,
candidate values, exact quotes when within the response limit, and
message/zone/span provenance. Full zones remain in local storage. A summary
reports omitted units; membership reports additional omitted messages.

## Tasks and commitments project the persisted answer

The To do scan no longer calls the legacy first-hit detector or mirrors
model commitment rows. It projects owner requests and owner promises from
stored tags. Reply-only asks gate Your turn; owner requests and their
commitments gate automatic Waiting on. Explicit timers retain their
existing authority. Relationship commitment projection commits with asks;
background model commitment extraction is no longer scheduled. Optional
compose safety and draft promise proposals remain separate existing paths.

Task identity uses the full counterparty address, action, explicit reference
or action object and due month. Payment reminders and failed-payment
escalations share the payment identity. A reminder without a date only
reuses an unambiguous existing period. New automatic rows wait for bounded
legacy-row reconciliation; manual edits, schedules and completed/dismissed
history are protected. Source replacement hides stale automatic rows until
the matching stored answer is projected. Withdrawn automatic asks expire;
confirmations still offer completion rather than guessing that work is done.

Migration 98 adds correction snapshots with a reason constraint. Backup
inventory and the old-schema comparison include its inverse. Wire protocol
and the web expectation are 26. OpenAPI, generated TypeScript and schema
snapshots must be regenerated before integration; they remain stale in
this unfinished candidate.

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
