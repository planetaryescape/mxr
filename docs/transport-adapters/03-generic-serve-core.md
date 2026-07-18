# Phase 3 — Generic Serve Core

Adapter-specific: **leaning** — still a net win without adapters (hermetic tests), but its shape is chosen with phase 4 in mind.

## Goal

Make the daemon's per-connection machinery generic over the byte stream, so a connection is "anything `AsyncRead + AsyncWrite`" rather than a `UnixStream`. The accept loop stays Unix-only; the serve core stops caring.

## Standalone value

- Connection-layer tests (the phase-2 corpus) gain an in-memory carrier via `tokio::io::duplex` — no socket files, no temp dirs, no daemon process: faster and hermetic.
- The corpus running over **two carriers** (UDS + duplex) proves the scenarios are carrier-independent — which is the whole premise of the adapter idea, demonstrated cheaply before committing to traits.
- Unlocks Q5 later (daemon-hosted web bridge calling in-process instead of round-tripping through its own socket).

## Current state (as of `3950d80c`)

- `serve_client_connection(stream: UnixStream, ...)` — `crates/daemon/src/server.rs:288-424`. Everything inside already operates on `Framed`/`IpcMessage`; only the parameter type and the accept loop bind it to Unix.
- Accept loop `server.rs:222-260`: `tokio::select!` over `listener.accept()`, shutdown watch, `JoinSet`; subscribes each connection to `state.event_tx`.
- Dispatch `handle_request` (`handler/mod.rs:416`) — already transport-neutral.

## Design

1. **Generify the connection fn:**

   ```rust
   async fn serve_client_connection<S>(stream: S, state: Arc<AppState>, ...)
   where S: AsyncRead + AsyncWrite + Unpin + Send + 'static
   ```

   Body unchanged — `Framed::new(stream, IpcCodec::new())` already compiles generically. Monomorphizes for `UnixStream`; no runtime cost.

2. **Extract a serve-core module** (`crates/daemon/src/serve.rs` or similar): the generic connection fn plus the pieces it owns — Hot/Bulk semaphores, per-request `JoinSet`, event subscription/fan-out, `EventsLagged` handling, `guard_ipc_response` panic guard, shutdown arm. `server.rs` retains what is genuinely UDS lifecycle: bind, permissions, stale-socket handling, pid file, the accept loop.

3. **Do not abstract the accept-loop concurrency.** Adapters (phase 4) will only produce connections; lanes/backpressure/event fan-out live here, once. This is the "don't fragment backpressure" rule from discovery §6.

4. **Connection metadata placeholder:** thread a minimal `ConnInfo` struct (today: empty or just a debug label) through `serve_client_connection`. Phase 4 widens it to `PeerInfo`. Doing the plumbing now keeps phase 4's diff small; keeping it empty now keeps this phase honest.

5. **Duplex carrier for the corpus:** add a factory to the phase-2 corpus that spins the serve core on one end of `tokio::io::duplex(N)` and hands the other end to `IpcConnection` (phase 1 gains a `from_stream` constructor usable in tests). Every scenario now runs twice: `uds` and `duplex`.

## Non-goals

- No trait objects, no new crates, no config changes.
- No change to socket lifecycle, autostart, build-id handshake, or permissions.
- No behavior change observable by any client.

## Verification

- Phase-2 corpus green on both carriers — this is the primary gate; the corpus was written first precisely to guard this refactor.
- `scripts/cargo-test -p mxr --tests`; full suite once before PR.
- Live smoke: restart daemon, `mxr status`, `mxr events`, TUI session — confirming the monomorphized UDS path is byte-identical in practice.

## Risks

- Refactor-shuffle regressions in the `select!` arms (bias ordering matters at `server.rs:306`). Mitigation: move code verbatim; the corpus pins drain/shutdown/read/event-push interleaving behavior.
- Generic bounds creep (`Send + 'static` infects helpers). Mitigation: keep the generic surface to the one entry function; helpers keep taking `Framed` halves.

## Exit criteria

Serve core compiles generically; accept loop is the only place that knows about `UnixListener`/`UnixStream` server-side; conformance corpus green on UDS **and** duplex; zero client-visible changes.
