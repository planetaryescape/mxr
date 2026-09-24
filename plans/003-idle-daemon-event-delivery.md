# Plan 003: Deliver daemon events to the TUI while it is idle

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/client.rs crates/tui/src/ipc.rs`
> On any drift, compare the "Current state" excerpts against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED (core IPC path — mitigated by the tests in this plan)
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

The daemon pushes events (`SyncCompleted`, `LabelCountsUpdated`,
`OperationProgress`, `MutationReconciliationFailed`, …) to every connected
client over its Unix-socket connection (each connection subscribes to a
broadcast channel — `crates/daemon/src/server.rs:244`). But the TUI's `Client`
only reads its socket **inside `request()`** — there is no background reader.
So while the TUI sits idle (user reading their inbox, no keys pressed), events
pile up unread in the kernel socket buffer. New mail does not appear, sync
progress does not update, and mutation-reconciliation failures surface late —
until the next user interaction happens to trigger an IPC request, which
drains the backlog. For an email client whose core promise is a live,
daemon-backed inbox, this is the highest-impact TUI bug found in the audit.
During active use the bug is masked (frequent requests keep the socket
drained), which is why it has survived.

## Current state

- `crates/tui/src/client.rs:12-16` — `Client` owns
  `framed: Framed<UnixStream, IpcCodec>`; no spawned tasks.
- `crates/tui/src/client.rs:37-68` — `request()` is the only place the stream
  is read; events seen while awaiting a response are forwarded:

  ```rust
  async fn request(&mut self, req: Request) -> Result<Response, MxrError> {
      let id = self.next_id.fetch_add(1, Ordering::Relaxed);
      // ... send ...
      loop {
          match self.framed.next().await {
              Some(Ok(resp_msg)) => match resp_msg.payload {
                  IpcPayload::Response(resp) if resp_msg.id == id => return Ok(resp),
                  IpcPayload::Event(event) => {
                      if let Some(ref tx) = self.event_tx {
                          let _ = tx.send(event);
                      }
                      continue;
                  }
                  _ => continue,
              },
              // ... error/None arms ...
          }
      }
  }
  ```

- `crates/tui/src/ipc.rs:72-149` — the IPC worker's loop. Note the
  `event_rx.recv()` arm only receives what `request()` forwarded; when no
  request is in flight, nothing reads the socket:

  ```rust
  loop {
      tokio::select! {
          req = rx.recv() => { /* timeout-bounded client.raw_request(...) */ }
          event = event_rx.recv() => {
              if let Some(event) = event {
                  let _ = result_tx.send(AsyncResult::DaemonEvent(event));
              }
          }
      }
  }
  ```

- Downstream handling already exists and works:
  `AsyncResult::DaemonEvent` → `handle_daemon_event` in
  `crates/tui/src/daemon_events.rs` (sets `pending_*_refresh` flags, status
  bar, etc.). Nothing downstream needs to change.
- Existing test exemplar: `crates/tui/src/ipc.rs:312-348`
  (`unresponsive_daemon_times_out_instead_of_hanging`) — spawns a fake Unix
  socket listener, uses `#[tokio::test(start_paused = true)]`. Model the new
  test on it.
- Protocol shape: `mxr_protocol::IpcMessage { id, source, payload }` with
  `IpcPayload::{Request, Response, Event}` framed by `IpcCodec` (see
  `crates/protocol/`). The daemon never sends `Request` to clients.

Cancellation-safety fact the design relies on: `Framed::next()` (i.e.
`StreamExt::next` on `tokio_util::codec::Framed`) is cancel-safe — partially
received frames remain buffered inside the `Framed` when the future is
dropped by `select!`.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | all pass, exit 0 |
| This plan's tests only | `scripts/cargo-test -p mxr-tui --lib ipc` | pass |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/client.rs` (add an idle-read method)
- `crates/tui/src/ipc.rs` (add the idle-read select arm + test)

**Out of scope**:
- `crates/daemon/**` — the daemon side already broadcasts correctly.
- `crates/tui/src/daemon_events.rs`, `runner.rs` — downstream handling is
  correct; do not modify.
- `ipc_call_dedicated` (`ipc.rs:302-310`) — dedicated short-lived connections
  intentionally drop events (the main connection receives the same broadcast);
  leave as-is.
- Any change to request/response correlation or the retry logic in the worker.

## Git workflow

- Branch: `advisor/003-idle-event-delivery`
- Conventional commit, e.g. `fix: read daemon events while the tui ipc worker is idle`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1 (red test first): prove the bug

Add to `crates/tui/src/ipc.rs` tests: a fake daemon (Unix listener, as in the
existing test) that, immediately after accepting the connection, writes one
encoded `IpcMessage { id: 0, source: ClientKind::Daemon (or Tui if Daemon
doesn't exist — check `mxr_protocol::ClientKind`), payload: IpcPayload::Event(DaemonEvent::…) }`
frame and then holds the connection open. Use any easily-constructed
`DaemonEvent` variant (check `crates/protocol/` for one with simple fields —
e.g. the sync-completed or label-counts variant used in
`crates/tui/src/daemon_events.rs`). Spawn `spawn_ipc_worker`, make **no
requests**, and assert that `result_rx` yields
`AsyncResult::DaemonEvent(..)` within a bounded wait (e.g.
`tokio::time::timeout(Duration::from_secs(5), result_rx.recv())`).
To write the frame, reuse the codec: wrap the accepted stream in
`Framed::new(stream, IpcCodec::new())` and `send(msg).await`.

**Verify**: `scripts/cargo-test -p mxr-tui --lib ipc` → the new test FAILS
(times out / recv pending) against current code. This confirms the bug.

### Step 2: add an idle-read method to `Client`

In `crates/tui/src/client.rs`:

```rust
/// Read one frame while no request is in flight, forwarding events.
/// Returns Err when the connection is closed/broken so the caller can
/// trigger its reconnect path. Cancel-safe: dropping the future leaves
/// any partial frame buffered inside `framed`.
pub(crate) async fn read_idle_frame(&mut self) -> Result<(), MxrError> {
    match self.framed.next().await {
        Some(Ok(msg)) => {
            match msg.payload {
                IpcPayload::Event(event) => {
                    if let Some(ref tx) = self.event_tx {
                        let _ = tx.send(event);
                    }
                }
                other => {
                    tracing::warn!(?other, "unexpected idle IPC frame; dropping");
                }
            }
            Ok(())
        }
        Some(Err(e)) => Err(MxrError::Ipc(describe_ipc_failure(&e.to_string()))),
        None => Err(MxrError::Ipc("connection closed".into())),
    }
}
```

(`Client` is `pub` but lives in the same crate; `pub(crate)` keeps this off
the public surface. If `tracing` isn't imported in client.rs, add `use` or
fully qualify — the crate already depends on tracing.)

**Verify**: `cargo build -p mxr` → exit 0.

### Step 3: add the idle-read arm to the worker loop

In `crates/tui/src/ipc.rs`, extend the `tokio::select!` in `spawn_ipc_worker`:

```rust
tokio::select! {
    req = rx.recv() => { /* unchanged */ }
    event = event_rx.recv() => { /* unchanged */ }
    idle = client.read_idle_frame() => {
        if let Err(error) = idle {
            let _ = result_tx.send(AsyncResult::ConnectionState(
                ConnectionState::Reconnecting {
                    since: std::time::Instant::now(),
                    reason: error.to_string(),
                },
            ));
            // Reuse the existing reconnect helper; on failure sleep and
            // let the next loop iteration retry, mirroring the initial
            // connect loop's behavior (lines 43-70).
            match connect_ipc_client(&socket_path, event_tx.clone()).await {
                Ok(fresh) => {
                    client = fresh;
                    let _ = result_tx.send(AsyncResult::ConnectionState(ConnectionState::Connected));
                }
                Err(_) => {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }
}
```

Borrow note: the `idle` arm holds `&mut client` only within its own future;
`select!` drops the unpolled futures, so `client.raw_request` in the `req` arm
does not conflict. If the compiler disagrees, restructure per STOP conditions
rather than fighting the borrow checker with unsafe/RefCell.

Important interaction to preserve: the `req` arm's timeout/reconnect logic
(lines 86-137) must stay exactly as-is.

**Verify**: `scripts/cargo-test -p mxr-tui --lib ipc` → Step 1's test now
PASSES; the pre-existing `unresponsive_daemon_times_out_instead_of_hanging`
still passes.

### Step 4: guard against reconnect loops in the new arm

Add a second test: fake daemon accepts, immediately closes the connection.
Assert the worker emits `ConnectionState::Reconnecting` (and does not busy-spin
— with `start_paused = true`, assert time advanced by the sleep between
retries, or simply that the test completes without hanging).

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → full crate suite passes.

## Test plan

- New test 1 (Step 1): unsolicited event reaches `result_rx` with no request
  in flight — the regression test for this bug.
- New test 2 (Step 4): connection drop while idle triggers reconnect state,
  no hang/busy-loop.
- Pattern: `crates/tui/src/ipc.rs` existing test module (paused-clock tokio
  tests with a fake Unix listener).
- Full suite: `scripts/cargo-test -p mxr-tui --tests`.

## Done criteria

- [ ] Both new tests exist in `crates/tui/src/ipc.rs` and pass
- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0
- [ ] `cargo build -p mxr` exits 0
- [ ] Manual smoke (if an operator is available): run TUI, trigger a sync from
      a second terminal (`mxr sync`), observe the status bar update without
      touching the TUI
- [ ] No files outside scope modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- The `select!` borrow structure in Step 3 does not compile after one honest
  restructuring attempt — report the compiler error; the fallback design
  (splitting `Framed` into read/write halves with a spawned reader task and a
  oneshot-per-request correlation map) is a bigger change that needs advisor
  sign-off.
- `Framed`'s stream turns out not to be cancel-safe in this tokio_util version
  (frames corrupted in the timeout test) — report immediately.
- Any pre-existing ipc test starts failing and the fix isn't obvious within
  two attempts.
- `mxr_protocol` has no `DaemonEvent` variant constructible in tests without
  daemon-side helpers — report which variants exist.

## Maintenance notes

- The idle arm makes the worker the *only* long-term reader of the connection;
  if anyone later adds a second reader (e.g. a streaming-response feature),
  correlation must move to a proper reader-task + response-map design.
- Reviewers should scrutinize: reconnect behavior when the daemon restarts
  (old connection EOF → idle arm fires → reconnect) and that
  request-in-flight timeout behavior is unchanged.
- Deferred: `EventsLagged` already covers broadcast overflow; no client-side
  backlog cap is needed since events drain continuously after this change.
