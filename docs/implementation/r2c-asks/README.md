# R2c's stored facet is under development

Continue from the independent asks facet in `mxr_core::asks`, the pure
`mxr_todo::asks::assess` judge and the daemon's
`handler/ask_assessments.rs`. The product contract remains
[blueprint 37](../../blueprint/37-what-it-asks.md). This candidate does not
complete R2c: To do, Your turn, Waiting on and relationship commitments
still use their existing consumers. Their model and detector documentation
remains current until those paths are retired.

## Inspect the stored answer through mode membership

```sh
mxr modes why MESSAGE_ID --format json
```

Use an isolated synthetic mailbox. The membership response adds `asks` and
`asks_pending`; old responses deserialize with both empty. Each asks record
contains its policy and fingerprint, body-pending flag, zones, candidate
spans, unit votes and resolved tags. An absent or invalidated record appears
in `asks_pending`. A record with an uncached body keeps `pending_body: true`.

The read surface is additive under protocol 25. The same mode-membership
request serves CLI JSON/JSONL, IPC and the bridge. Client presentation and
the switch of product projections remain unfinished.

## The asks record cannot create an outbound home

`asks-r2c-1` uses the existing `mail_assessments` storage envelope, with an
independent JSON payload. It covers both received and sent messages. The
`r2-1` home materializer still skips outbound messages and keeps its existing
resolver, claims and fingerprint. Home-version reports exclude the reserved
`asks-` policy namespace.

The asks write compares the observed thread generations in a transaction.
Each row stores its own thread token. The home batch acknowledges dirty
threads after the asks batch succeeds. A mutation between the two commits
makes the old asks row unreadable and rejects the home commit; the next
bounded reconciliation retries. Empty asks batches and settled reads use
the reader pool. They never acquire the writer to repair state.

No database schema migration is added. Backup/restore keeps the existing
assessment-table treatment. Required changes to a persisted asks payload
need an asks-policy bump; changing the home policy to refresh asks would
unnecessarily invalidate R2 placement.

## Authored units retain source evidence

Owner matching comes from `OwnedAddresses` through the daemon's existing
matcher. Facts retain the primary address and aliases, observed owner names,
recipient role, one predecessor and whether the owner wrote afterwards.
The predecessor is selected by storage order, independently of the sender's
Date header. The reader's `authored_text` excludes contextual material.
Explicit quote, forward, signature and footer boundaries are retained as
zones; removed text without a supported attribution stays `unattributed`.

Amounts share `money::find_candidates` with the existing positive selector.
Signed amounts, zero and R-prefixed ZAR remain candidates with verbatim byte
spans. Due candidates use `dates::find_due` and `natural_time`, anchored to
the message date in the supplied timezone. Account, invoice, policy and
order references retain their spans. HTML schema.org markup is not used by
this candidate; it supplies no unverified structured-data authority.

Rules emit unit votes. Agreement produces tags; conflicting or unsupported
readings abstain. Document readings require a positive amount and a date or
explicit immediate payment, with minimum-payment fields for statements.
Offers and automatic payments stay separate from other units. A reply after
a request settles a reply-only ask; it does not complete a named task.

## Verify persistence before switching product consumers

```sh
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr-todo --tests
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr-store --tests asks::
agent-work run -- env CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 RAYON_NUM_THREADS=4 scripts/cargo-test -p mxr --tests ask_assessments
```

Keep the inner `env`: the shared wrapper sets worker defaults before
launching its command. Reuse this worktree's `target-cli` and shared
sccache. Record actual check results in the worker report; these commands
are the verification entry points, not evidence that they passed.

Before integration, finish the item identity/lifecycle projection and safe
legacy-row reconciliation, retire the old active detector and commitment
model paths, connect all four product consumers, implement the correction
picker with durable reasons and concurrent-safe undo, add the collapsed
uncertainty section, and extend read-only R1c grading of stored results.
Regenerate OpenAPI and web types through their canonical generators, run
the complete touched-crate gate and synthetic public journey, and pass the
actual UI design matrix. The week of counts-only real-mail dogfood remains
pending and is outside development/smoke authorization.
