# Phase 1 — Shared IPC Client Crate (`mxr-client`)

Adapter-specific: **no**. This phase pays for itself as pure deduplication.

## Goal

One implementation of "connect to the daemon, send a framed request, correlate the response, read events" — replacing four near-identical copies.

## Standalone value

- Four independent implementations of connect + `Framed<UnixStream, IpcCodec>` + `AtomicU64` id correlation collapse to one. Bug fixes (timeouts, error mapping, reconnect edge cases) land once.
- `ClientKind` tagging becomes a constructor parameter instead of a per-copy detail — the class of bug fixed on 2026-07-18 (web bridge tagging itself `Cli`) becomes structurally impossible.
- Consistent kinded error handling (`IpcErrorKind`, retryable flag) across all clients.

## Current duplication (as of `3950d80c`)

| Client | Where | Quirks to preserve |
|---|---|---|
| CLI / daemon-internal | `crates/daemon/src/ipc_client.rs` (`IpcClient`) | 120 s default timeout; `request_with_events` (inline `OperationProgress` while awaiting the correlated response); `notify`; `next_event`. ~58 command-file call sites. |
| TUI | `crates/tui/src/client.rs` + worker `crates/tui/src/ipc.rs` | Persistent connection; infinite reconnect w/ `ConnectionState` transitions; 60 s per-request bound; retry-safe request classification (`request_supports_retry`); dedicated throwaway connections for slow LLM calls (`ipc_call_dedicated`); idle event-frame reads. |
| Web bridge | `crates/web/src/lib.rs:1670-1746` | Connection per HTTP request; event-only connection relayed to WebSocket (`bridge_events`). |
| MCP | `crates/mcp/src/lib.rs` (`UnixDaemonRequester`) | Connection per call; already behind the `DaemonRequester` trait — the shape to generalize. |

## Design

New crate `crates/client` (`mxr-client`).

- **Dependencies:** `mxr-protocol` (+ transitively `mxr-core`), `tokio`, `tokio-util`, `futures`. Nothing else internal — same leaf discipline as provider crates.
- **Core type** `IpcConnection`:
  - `connect(path: &Path, kind: ClientKind) -> Result<Self>` (still `UnixStream` inside — the point of this phase is *concentration*, not abstraction; the single `UnixStream::connect` call site becomes the seam phase 4 opens).
  - `request(Request) -> Result<ResponseData, ClientError>` with per-call or per-connection timeout config (defaults preserved per consumer: 120 s CLI, 60 s TUI).
  - `request_with_events(Request, impl FnMut(DaemonEvent))` — the CLI progress pattern.
  - `notify(Request)` (fire-and-forget), `next_event() -> Result<IpcMessage>` (event streams).
  - Internal `AtomicU64` id correlation; responses matched by id, interleaved `Event` frames surfaced or skipped per call mode.
- **Error type** `ClientError { Connect, Io, Daemon { message, kind: IpcErrorKind, retryable }, Timeout, Closed }` — gives the TUI's retry classifier and the bridge's HTTP mapping one vocabulary.
- **Policy stays with consumers.** Reconnect loops, autostart, retry decisions are client policy, not connection mechanism:
  - TUI keeps its worker (`tui/src/ipc.rs`) — rebuilt on `IpcConnection`, keeping `ConnectionState`, autostart, retry-safe classification.
  - CLI keeps `ensure_daemon_running` / spawn logic in the daemon crate (it re-execs the binary; that is not client-crate business).
  - Bridge keeps connection-per-request (its isolation model) but constructs via `mxr-client`.
  - MCP's `UnixDaemonRequester` implements `DaemonRequester` on top of `IpcConnection`.

## Steps

1. Create `crates/client`; add to workspace; wire `IpcConnection` + `ClientError` with tests against a fake socket server (pattern exists in `crates/web/src/tests.rs::spawn_fake_ipc_server`; consider promoting that helper into `crates/test-support`).
2. Migrate `crates/daemon/src/ipc_client.rs` to a thin wrapper over `IpcConnection` **preserving the existing `IpcClient` API** so the ~58 command call sites don't churn in this phase.
3. Migrate TUI `Client` internals; keep `tui/src/ipc.rs` worker behavior byte-identical (its tests pin reconnect/retry semantics).
4. Migrate web bridge `ipc_request` / `ipc_request_with_id` / `bridge_events` internals.
5. Migrate MCP `UnixDaemonRequester`.
6. Update `docs/blueprint/crate-boundary-audit.md` and `docs/blueprint/01-architecture.md`: new rule — `client` depends only on `protocol`; `daemon`, `tui`, `web`, `mcp` may depend on `client`.

## Non-goals

- No transport trait, no `Connector` abstraction, no config changes, no protocol changes.
- No behavior changes visible to any client (timeouts, retry, event ordering all preserved).

## Verification

- Per-crate: `scripts/cargo-test -p mxr-client --tests`, then each migrated crate, one at a time (link-time discipline).
- Full suite once before the PR.
- Live: rebuild, restart daemon, exercise `mxr status --format json`, `mxr events` (event stream), one TUI session (reconnect path: kill daemon mid-session, watch it recover), one web bridge request, and `mxr activity` to confirm source attribution per client kind.

## Risks

- TUI reconnect semantics are subtle (retry-safe classification, dedicated connections). Mitigation: migrate mechanism only; leave every policy branch where it is; rely on existing TUI ipc tests.
- 58 CLI call sites — avoided by keeping the `IpcClient` facade this phase.

## Exit criteria

Four crates construct IPC connections exclusively through `mxr-client`; no `UnixStream::connect` outside `mxr-client`, `server.rs` (probe), and `doctor.rs` (reachability check); all suites green; live smoke passes.
