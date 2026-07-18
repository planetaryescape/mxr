# Phase 2 — IPC Conformance Corpus (Characterization)

Adapter-specific: **no**. This phase writes tests only — zero production code changes.

## Goal

Turn the daemon's observable IPC behavior into an executable specification: a scenario corpus that today runs against the real Unix socket, and later (phases 3–6) runs unchanged against every transport adapter. Written **before** the phase-3 refactor so that refactor lands guarded.

## Standalone value

- Protocol v4's connection-level semantics are currently enforced only by implementation. The corpus becomes the regression net for *any* future server work (lane tuning, event changes, framing), adapters or not.
- The connectrpc lesson applied: one scenario corpus, executed against every carrier. The provider system already proved this shape works here (`run_sync_conformance` in `crates/provider-fake/src/conformance.rs`).

## Scenario corpus

Written as generic async functions over a connection factory (`Fn() -> Future<IpcConnection>` from phase 1), so the same scenarios later accept duplex/TCP/stdio factories.

**Correlation & concurrency**

1. Request/response id correlation — client-chosen ids echoed; multiple in-flight requests on one connection each get their own response.
2. Out-of-order completion — a slow request (Bulk lane) does not block a fast one (Hot lane) on the same connection; responses arrive by completion order, matched by id. (Lane classification: `request_lane`, `crates/daemon/src/server.rs:348-375`.)
3. Lane saturation — more concurrent requests than `REQUEST_CONCURRENCY_LIMIT`/`BULK_CONCURRENCY_LIMIT` (64/8) queue rather than fail.

**Eventing**

4. Broadcast event reaches every connected client, `id: 0`, `source: ClientKind::default()` (== `Cli`). (Originally specified as `source: Daemon`; the corpus pinned the actual value — see [Pinned findings](#pinned-findings).)
5. Events interleave with an in-flight request on the same connection without corrupting correlation (`request_with_events` path).
6. Event-only connection (never sends a request) receives events — the `mxr events` / bridge `bridge_events` pattern.
7. `EventsLagged { skipped }` is delivered point-to-point to a slow consumer after broadcast overflow (channel capacity 256, `state.rs:522`), and the connection survives.

**Framing edges**

8. Frame at/near the 16 MiB limit round-trips; an oversized frame errors without killing the daemon (`codec.rs:18`).
9. Malformed JSON in a valid length-prefixed frame → `InvalidData` handling; daemon connection behavior on it is pinned (documented, whatever it is today).
10. Truncated frame / mid-frame disconnect → server cleans up the connection task without leaking.

**Lifecycle & failure**

11. Client disconnect with a request in flight — handler completes/aborts without wedging a lane permit.
12. Handler panic → kinded `Error` response, connection stays usable (`guard_ipc_response`, `server.rs:974`).
13. Daemon shutdown signal closes connections cleanly (shutdown watch arm, `server.rs:324`).
14. Unauthenticated/unauthorized paths: placeholder scenario asserting today's UDS behavior (any local connection accepted) — becomes the per-transport auth matrix in phase 5.

## Implementation shape

- Location: `crates/daemon/src/server/ipc_conformance.rs` — an in-crate `#[cfg(test)]` child module of `server`, in a module designed for later extraction (phase 6 packages them for out-of-tree adapters, mirroring provider-fake's placement). Not `crates/daemon/tests/`: the corpus drives the private `serve_client_connection` and the `#[cfg(test)]` state constructors (`AppState::in_memory`, `add_sync_provider_for_test`) directly, none of which are reachable from a black-box integration test (private fn typed to `UnixStream`; `#[cfg(test)]` items aren't compiled for downstream builds).
- Server under test: real daemon serve loop on a temp socket with fake providers — existing patterns: `#[cfg(test)] add_sync_provider_for_test` (`state.rs:1189`), `mxr-provider-fake`, temp-socket tests already in `server.rs:1704+`, and `crates/test-support`.
- Determinism: for the slow-request scenario prefer a naturally Bulk-lane request against fake-provider latency; if nothing is deterministic enough, add a `#[cfg(test)]`-only delay hook rather than sleeps-and-hope.
- Each scenario is one focused `#[tokio::test]`; corpus doc-commented as the normative description of connection-level behavior.

## Non-goals

- No production code changes (except a `#[cfg(test)]` hook if scenario 2 requires one).
- No new crates yet; extraction is phase 6.
- Not a request-semantics suite — this corpus covers connection/transport-level behavior only, not what each of the ~150 `Request` variants does.

## Verification

`scripts/cargo-test -p mxr --tests` (daemon crate) — corpus green against the real UDS path. Full suite once before PR.

## Risks

- Timing-sensitive scenarios (lag, saturation) can flake. Mitigation: drive with explicit synchronization (barriers/oneshots), never wall-clock sleeps; `EventsLagged` scenario controls the receiver, not the sender.
- Characterization may surface surprising current behavior (e.g., what happens on malformed frames). Rule: **pin what exists, file an issue if it looks wrong** — this phase documents, it does not redesign.

## Pinned findings

Characterization pins what exists — it does not redesign. Two current behaviors diverged from expectations and are pinned as-is; fixing either is a candidate follow-up **outside this phase**.

1. **Daemon events carry `source: ClientKind::default()` (== `Cli`), not `Daemon`.** Every production emitter sets this — `chimes::emit_daemon_event`, `diagnostics::emit_operation_event`, and the server's own `SyncCompleted` broadcast all construct the event message with `source: ClientKind::default()`. Scenario 4 asserts `== ClientKind::default()` and `!= ClientKind::Daemon` to lock this in. *Candidate follow-up (not this phase): give daemon-synthesized events a `source: Daemon` tag so activity surfaces attribute them correctly.*
2. **Malformed-JSON and truncated frames close the connection silently — no error frame is returned.** A valid length-prefixed frame carrying malformed JSON surfaces as `io::ErrorKind::InvalidData` in the decoder; a truncated / mid-frame disconnect surfaces as a partial-frame error or EOF. In both cases the serve loop stops accepting on that connection and drains it — the client just observes EOF, never a `Response::Error`. Pinned by scenarios 9 and 10. *Candidate follow-up (not this phase): decide whether protocol-level frame errors should return a kinded error frame before closing.*

## Exit criteria

All 14 scenarios green against the real UDS carrier in CI; corpus readable as a behavioral spec; any surprising pinned behaviors logged as findings (see [Pinned findings](#pinned-findings)).
