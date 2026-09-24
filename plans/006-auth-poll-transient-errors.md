# Plan 006: Make the Outlook auth-session poller survive transient IPC errors

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/runner.rs`
> On any drift, compare the "Current state" excerpt against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

During device-code account auth (Outlook flow), the TUI polls the daemon for
session state every few seconds. One transient IPC failure — daemon briefly
busy, a reconnect blip — permanently kills the polling loop. The auth screen
then silently stops updating: the user completes auth in the browser but the
TUI never learns, and there is no retry. Auth is a first-run experience;
failing it on a blip is a bad first impression.

## Current state

`crates/tui/src/runner.rs:96-134` (`spawn_outlook_auth_session` polling tail):

```rust
loop {
    tokio::time::sleep(poll_interval).await;

    match ipc_get_auth_session(bg, session_id.clone()).await {
        Ok(updated) => {
            let done = is_terminal(&updated.state);
            let _ = result_tx.send(AsyncResult::AuthSession(updated));
            if done {
                break;
            }
        }
        Err(e) => {
            let _ = result_tx.send(AsyncResult::AccountOperation(Err(e)));
            break;                       // ← first error aborts polling forever
        }
    }
}
```

Context: `GetAuthSession` is not in the shared worker's
`request_supports_retry` allowlist (`crates/tui/src/ipc.rs:240-280`), so the
worker does not retry it on reconnect — the poller sees the raw error.
`is_terminal` (lines 100-107) defines when polling legitimately stops.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/runner.rs` (the polling function only)
- `crates/tui/src/runner/tests/accounts_and_delivery.rs` (tests)

**Out of scope**:
- `request_supports_retry` in `ipc.rs` — adding `GetAuthSession` there changes
  retry semantics for all callers; not needed for this fix.
- The daemon's auth session implementation.

## Git workflow

- Branch: `advisor/006-auth-poll-resilience`
- Conventional commit, e.g. `fix: retry transient ipc errors in auth session polling`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Tolerate consecutive transient failures

Replace the `Err` arm with a bounded-consecutive-failures policy:

```rust
const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 5;
let mut consecutive_failures = 0u32;

loop {
    tokio::time::sleep(poll_interval).await;

    match ipc_get_auth_session(bg, session_id.clone()).await {
        Ok(updated) => {
            consecutive_failures = 0;
            let done = is_terminal(&updated.state);
            let _ = result_tx.send(AsyncResult::AuthSession(updated));
            if done { break; }
        }
        Err(e) => {
            consecutive_failures += 1;
            if consecutive_failures >= MAX_CONSECUTIVE_POLL_FAILURES {
                let _ = result_tx.send(AsyncResult::AccountOperation(Err(e)));
                break;
            }
            tracing::debug!(error = %e, consecutive_failures, "auth session poll failed; retrying");
        }
    }
}
```

(Keep the existing 5s default `poll_interval` as the retry spacing — with the
worker's own 60s request timeout this gives minutes of tolerance without a
separate backoff mechanism.)

**Verify**: `cargo build -p mxr` → exit 0.

### Step 2: Tests

In `crates/tui/src/runner/tests/accounts_and_delivery.rs` (find the existing
auth-session tests there — grep `AuthSession` — and copy their fake-IPC
fixture style):

1. Poller receives Err, then Ok(terminal): asserts an `AuthSession` result is
   still delivered (no abort on first error).
2. Poller receives `MAX_CONSECUTIVE_POLL_FAILURES` consecutive Errs: asserts
   an `AccountOperation(Err(..))` is delivered and the task ends.

Use `#[tokio::test(start_paused = true)]` so the 5s sleeps are virtual.

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → all pass with 2 new tests.

## Test plan

As Step 2; the first test is the regression test (fails against current code).

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0 with the new tests
- [ ] `cargo build -p mxr` exits 0
- [ ] No files outside scope modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- `spawn_outlook_auth_session` has no test coverage hooks (can't inject an
  Err) without refactoring its signature — report the needed seam instead of
  refactoring runner.rs.
- Excerpt drift or verification fails twice.

## Maintenance notes

- If the daemon later adds push notifications for auth-state changes
  (see Plan 003's event delivery), this poller can shrink to a fallback.
- Reviewers: check the failure toast still appears when the daemon is truly
  down (test 2 covers it).
