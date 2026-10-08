# Trust-reclassify follow-ups found in review

**Status:** open · **Found:** 2026-10-08 in Codex review of PR #314 at
`238cb19c` · **Severity:** low to medium; none blocks the PR

The rest of that review (findings 1-8, 11-13) is fixed in the PR. These are
left alone on purpose.

## 1. The Not-sure cap counts surviving conflicts, not questions shown

`not_sure_today` (`crates/daemon/src/handler/arrivals.rs`) takes the first
three conflict rows first seen today on every request. The cap is meant to
be three questions asked a day. If three questions are shown and one of the
emails is then deleted, its arrival row cascades away and the fourth
conflict appears the same day.

Fix: persist the ids shown per local day (a small table keyed by account,
day and message id), take the cap from that, and only add new ids while
fewer than three have been shown.

## 2. "Written to" trusts From == the account on inbound mail

Sync marks a message outbound when its `From` matches the receiving
account (`crates/sync/src/engine.rs`), and the written-to query
(`crates/store/src/desk.rs`) trusts that row's recipients. A delivered or
imported message with `From` set to the user and `To` set to a stranger
makes the stranger count as someone the user wrote to, which the never-bury
rule then trusts. The direction heuristic predates this PR; the never-bury
rule is the first thing that depends on it for sorting.

Fix: derive direction from the provider's sent label or folder, not the
header, where the provider has one; otherwise require a matching
Authentication-Results or the sent folder before counting recipients.

## 3. UndoMove has no preview

`Request::UndoMove` takes only a correction id, and the CLI and HTTP bridge
run it at once. Every other new mutation here has a dry run. Undo reverses
the user's own action and refuses when a newer move stands, so the risk is
small, but the preview rule says new mutations get one. Policy question for
BK: does an undo of your own move need a preview, or is it exempt like the
existing mail-mutation undo?

## 4. The 8 MiB runtime stack is still needed

`crates/daemon/src/main.rs` keeps the 8 MiB tokio worker stack from #313.
`dispatch` is one large `match` and its poll frame is still big; Reading,
Updates' `get_digest`/`let_go` and the arrivals requests are boxed, but
other arms are not. Follow-up: measure the frame, box the remaining large
arms, then remove the custom stack size and confirm with the demo-daemon
`archive` repro.

## 5. An email move and a sender move in the same second can order wrongly on undo

An email move records `moved_at` in whole seconds, and a sender decision
that overrides it compares `decided_at > moved_at` strictly. Within one
second: move an email to Messages, move its sender to Updates, move the same
email to Reading, then undo the Reading move. Restoring the snapshot clears
`superseded_by` and the strict comparison no longer sees the sender move as
newer, so the email returns to Messages instead of Updates. Fix: snapshot
`superseded_by` with the move and restore it, or order by correction id
instead of timestamps.

## 6. Plain crowd mail from someone you've written to still goes to Updates

Mail addressed to you and more than ten others, with no list headers, from a
person you've written to is classified as a person (`Person`, not
`WrittenTo`). `kept_in_messages` protects only the `WrittenTo` rule, so the
crowd rule in `conversation_shape` sends it to Updates without a Not-sure
question. Product call: should a known correspondent's crowd mail ask, or
stay in Messages?

## 7. Two overlapping moves to To do can create two tasks

`MoveMessage(todo)` checks for an open to-do and then inserts. Two requests
that both pass the check before either inserts each create a task; manual
to-dos get a fresh id and a `manual|{id}` dedup key, so nothing in the
database stops it. Fix: a uniqueness key per source email for open manual
to-dos, or serialise the check and insert per message.

## 8. Clients that stay connected across an upgrade cannot decode `ModesChanged`

The new `DaemonEvent::ModesChanged` is broadcast to every connection. A
v0.6.58 client has no variant for it and no fallback, so decoding fails and
its pending request returns an error, although both versions report
protocol version 4. Fix: a fallback `#[serde(other)]` variant on the event
enum in the released protocol going forward, and a version gate (bump the
protocol version or send the event only to clients that declared support).
