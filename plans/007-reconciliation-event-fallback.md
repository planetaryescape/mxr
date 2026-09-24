# Plan 007: Never silently drop MutationReconciliationFailed events

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/daemon_events.rs`
> On any drift, compare the "Current state" excerpt against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (Plan 003 makes these events arrive promptly; this plan handles a malformed one)
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

When the daemon reports that a mutation failed to reconcile, the TUI parses
the event's `client_correlation_id` string back into its numeric mutation id.
If that parse fails (protocol drift, daemon bug, id echoed from another
client), the **entire** failure handling is skipped: no rollback of the
optimistic UI change, no toast, nothing. The user sees the action as
succeeded; it silently reverts on the next refresh. Low likelihood, but the
failure mode is exactly the kind of silent divergence the optimistic-mutation
machinery exists to prevent — the fallback should reconverge the UI loudly.

## Current state

`crates/tui/src/daemon_events.rs:123-134`:

```rust
DaemonEvent::MutationReconciliationFailed {
    client_correlation_id,
    error_summary,
} => {
    if let Ok(raw) = client_correlation_id.parse::<u64>() {
        let mid = MutationId::from_raw(raw);
        app.handle_mutation_reconciliation_failed(mid);
        app.pending_optimistic.clear(mid);
        app.refresh_mailbox_after_mutation_failure();
        app.push_toast(Toast::error(format!("Mutation failed: {error_summary}")));
    }                                     // ← parse failure: event vanishes
}
```

The full-resync idiom to reuse on the fallback path already exists in the same
file — `DaemonEvent::EventsLagged` handling (`daemon_events.rs:135-147`):

```rust
app.mailbox.pending_labels_refresh = true;
app.mailbox.pending_all_envelopes_refresh = true;
app.mailbox.pending_subscriptions_refresh = true;
app.diagnostics.pending_status_refresh = true;
if let Some(label_id) = app.mailbox.active_label.clone() {
    app.mailbox.pending_label_fetch = Some(label_id);
}
```

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/daemon_events.rs`

**Out of scope**:
- Daemon-side event emission (`crates/daemon/**`) — the string-typed
  correlation id in the protocol is settled; don't change the protocol.
- `handle_mutation_reconciliation_failed` internals.

## Git workflow

- Branch: `advisor/007-reconciliation-fallback`
- Conventional commit, e.g. `fix: reconverge ui when a reconciliation event has an unparseable id`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Add the fallback arm

```rust
match client_correlation_id.parse::<u64>() {
    Ok(raw) => { /* existing 4 lines unchanged */ }
    Err(_) => {
        tracing::warn!(
            client_correlation_id,
            "unparseable correlation id on reconciliation failure; forcing refresh"
        );
        app.push_toast(Toast::error(format!("Mutation failed: {error_summary}")));
        // Same full-resync idiom as the EventsLagged arm below: we can't
        // roll back a specific optimistic change, so refetch everything.
        app.mailbox.pending_labels_refresh = true;
        app.mailbox.pending_all_envelopes_refresh = true;
        app.mailbox.pending_subscriptions_refresh = true;
        if let Some(label_id) = app.mailbox.active_label.clone() {
            app.mailbox.pending_label_fetch = Some(label_id);
        }
    }
}
```

**Verify**: `cargo build -p mxr` → exit 0.

### Step 2: Test

`handle_daemon_event` takes `&mut App` and an event — unit-testable without a
runner. Add to the existing test module in `daemon_events.rs` (or create one
matching the crate's in-file test style): feed a
`MutationReconciliationFailed` with `client_correlation_id: "not-a-number"`,
assert a toast was pushed and `pending_all_envelopes_refresh` is true.

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → all pass with the new test.

## Test plan

As Step 2; the malformed-id case is the regression test (fails against
current code: no toast, no refresh flags).

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0 with the new test
- [ ] `cargo build -p mxr` exits 0
- [ ] No `if let Ok(raw) = client_correlation_id.parse` remains (replaced by exhaustive match)
- [ ] `plans/README.md` status row updated

## STOP conditions

- Excerpt drift, or the surrounding match arms changed shape.
- A verification fails twice.

## Maintenance notes

- If the protocol ever types `client_correlation_id` as `u64`, this fallback
  becomes dead — remove it then.
- Reviewers: confirm the toast copy matches the parsed-path wording.
