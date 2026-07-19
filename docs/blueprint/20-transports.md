# 20 — Transports

How the daemon's client transport is made pluggable, and how to write, conformance-test, and ship a new transport adapter. This chapter is the single reference for the transport seam; the design research and phase history live in `docs/transport-adapters/`.

**Design in one line:** freeze the protocol, abstract the byte stream, keep HTTP a gateway, put community transports out-of-process.

---

## 1. Architecture

The daemon speaks one frozen wire protocol over *some* byte stream. Three layers, only the middle one frozen:

```
clients (CLI, TUI, MCP, web gateway, scripts)
        │  Connector::connect() -> byte stream
        ▼
FROZEN protocol: IpcMessage / Request / ResponseData / DaemonEvent + IpcCodec framing
        ▲
        │  ServerTransport → TransportListener::accept() -> (byte stream, PeerInfo)
        ▼
adapters: UDS (default) · in-memory duplex (tests) · TCP-loopback+token · stdio
          [community, out-of-process: ssh / containers / tunnels via `mxr daemon dial-stdio`]

HTTP bridge (mxr-web): REST + WS + SPA gateway — consumes a Connector; NOT a ServerTransport.
```

Three principles, each with a decision-log entry:

- **Frozen protocol.** `IpcMessage` / `Request` / `ResponseData` / `DaemonEvent` and `IpcCodec` framing (4-byte length prefix, JSON body, 16 MiB cap — `crates/protocol/src/codec.rs`) are identical on every transport. `IPC_PROTOCOL_VERSION` is `4` (`crates/protocol/src/lib.rs:35`) and does not move per transport. The ecosystem premium is on the protocol, not transport pluggability (the Podman/varlink regret — see D052, D055).
- **Byte-stream-level abstraction.** An adapter produces a connected `AsyncRead + AsyncWrite` stream plus peer/auth evidence — nothing more. Not a typed-RPC `Transport` (tarpc), which would exclude curl/jq/scripts/non-Rust agents against mxr's CLI-first JSON shape (D055).
- **HTTP is a gateway.** `mxr-web` translates REST/WS into the same `Request` enum and *uses* a client transport; it is not a `ServerTransport`. Discovery measured it: ~100 lines transport plumbing, ~5,500 lines presentation (D052).

Where transports are named in code: the daemon's `build_transports` factory (`crates/daemon/src/server.rs:409`) is the only place concrete `ServerTransport` types are constructed; the accept loop iterates a `Vec<Box<dyn ServerTransport>>` and threads each connection's `PeerInfo` into dispatch. The client side is generic over a `Connector` through the shared `mxr_client::IpcConnection` (`IpcConnection::connect_with`).

---

## 2. Trait reference (as built)

All in `crates/transport` (`mxr-transport`), a pure byte-stream leaf crate depending on **no** internal `mxr-*` crate (only `tokio` / `async-trait` / `thiserror` / `tracing`). Object-safe `#[async_trait]` traits, consumed as `Box<dyn _>` — the provider-adapter shape (D053).

### `ServerTransport` (`src/lib.rs`)

| Method | Contract |
|---|---|
| `name(&self) -> &str` | Stable adapter name for logs/status (`"uds"`, `"tcp"`, `"memory"`). |
| `capabilities(&self) -> TransportCapabilities` | Namespaced, additive, all-false by default. The daemon **trusts** these. |
| `async bind(&self) -> Result<Box<dyn TransportListener>>` | Bind once at startup; return a listener. |

### `TransportListener` (`src/lib.rs`)

| Method | Contract |
|---|---|
| `async accept(&mut self) -> Result<(BoxedIo, PeerInfo)>` | Next connection: a byte stream + auth evidence. **Must be cancel-safe** — the accept loop `select!`s several listeners and drops the losers each round; a dropped `accept` future must not lose or leak a connection. |
| `async stop_accepting(&mut self)` | Close the listening endpoint so new clients are refused promptly (must not hang), **without** releasing the resource. Idempotent. |
| `async cleanup(&mut self) -> Result<()>` | Release the transport resource (socket file, …). Runs LAST, after the connection drain. Idempotent. |
| `endpoint(&self) -> String` | Human-readable endpoint for logs. |

The `stop_accepting` / `cleanup` split is deliberate: during an upgrade restart a successor daemon can re-bind the endpoint while the old one drains, so the resource release is deferred and ownership-guarded (`UdsListener` matches socket identity before unlinking — `src/uds.rs`).

### `Connector` (`src/lib.rs`)

| Method | Contract |
|---|---|
| `async connect(&self) -> Result<BoxedIo>` | Open one connection. |
| `describe(&self) -> String` | What this connector dials. |
| `auth_token(&self) -> Option<&str>` | Bearer token, if any. The byte-stream seam stays protocol-free: the connector advertises a token here, and `mxr_client::IpcConnection::connect_with` performs the framed `Authenticate` handshake. Default `None` (no handshake) — every implicit-trust transport. |

`BoxedIo = Box<dyn AsyncReadWrite + Send + Unpin>`; `AsyncReadWrite` is blanket-implemented for any `AsyncRead + AsyncWrite`, so the boxed connection drops straight into `Framed<_, IpcCodec>`.

### `PeerInfo` / `PeerAuth` (`src/peer.rs`)

`accept` surfaces auth evidence — the Tailscale lesson: identity is per-transport, so the contract must carry it, not just bytes (D056).

```rust
enum PeerAuth {
    UnixPeer { uid: u32, gid: u32, pid: Option<i32> }, // OS peer creds — ALWAYS real
    LocalProcess,                                        // in-process, implicitly trusted
    TokenRequired,                                       // must authenticate before trust
}
struct PeerInfo { auth: PeerAuth }   // a struct so it can grow additively
```

`UnixPeer` always means the OS reported these credentials for this connection — an accept that cannot read them fails closed rather than fabricating the variant (`UdsListener::accept`). So the serve core can match `UnixPeer` and *know* the creds are real. Additive: future transports add variants (`TlsClientCert`, `PipeIdentity`) without disturbing today's.

### `TransportCapabilities` (`src/caps.rs`)

Namespaced, every field a `bool` defaulting to `false` — the provider-capabilities lesson: a missing/false signal is the safe default, and the daemon trusts what an adapter advertises.

```rust
struct TransportCapabilities {
    locality:  LocalityCaps  { same_machine: bool },              // guards $EDITOR-compose, local paths, build-id handshake
    auth:      AuthCaps      { implicit_peer_identity: bool, token: bool },
    lifecycle: LifecycleCaps { client_autostart: bool },          // client can re-exec the daemon
}
```

### `TransportAddr` (`src/addr.rs`)

Docker's precedent — a small scheme names an endpoint. `MXR_DAEMON_ADDR` (`DAEMON_ADDR_ENV`) overrides the default socket path; precedence `MXR_DAEMON_ADDR` > `MXR_SOCKET_PATH` > per-instance default.

- `unix://<path>` — the default; path taken verbatim (no percent-decoding, spaces preserved).
- `tcp://<host:port>` — loopback + token (§4).
- `cmd://<command line>` — spawn-and-pipe; argv whitespace-split, no shell quoting.

`resolve_unix_socket` lets the `unix://`-only clients (TUI/web/MCP) share the resolver but reject `tcp://`/`cmd://` with a clear message rather than silently ignore them.

---

## 3. Per-transport security policy

Leaving UDS-only removes implicit protections one at a time (Caddy / Tailscale model). As implemented:

| Transport | Type | `PeerAuth` on accept | Implicit auth | Required policy |
|---|---|---|---|---|
| **UDS** (default) | `UdsServerTransport` | `UnixPeer { uid, gid, pid }` | fs perms (0600) + peer creds | none extra — today's posture |
| **In-memory** | `MemoryTransport` (`test-util`) | `LocalProcess` | in-process | none |
| **stdio** | `mxr daemon --stdio` | `LocalProcess` | inherits the spawner's trust | none extra — the spawner authenticates |
| **cmd (out-of-process)** | `CmdConnector` (client side) | — | inherits the spawned bridge's trust | the bridge (SSH/exec) is the authenticator |
| **TCP loopback** | `TcpServerTransport` | `TokenRequired` | **none** — any local process, and browsers via DNS-rebinding / `0.0.0.0` tricks | bearer token even on loopback; **refuse non-loopback bind** (no TLS in scope) |
| **TCP remote** | — (out of scope) | — | none | mTLS or refuse; recommend deferring remote entirely (D054/Q2) |

The serve core enforces the TCP row: `TokenRequired` peers get `IpcErrorKind::Auth` on every request — and no events — until a successful `Authenticate` (`ConnectionAuth`, `crates/daemon/src/serve.rs:56`). UDS/memory/stdio start trusted and are byte-for-byte unchanged (pinned by the corpus's no-auth tests, so an accidental token-gate on UDS fails loudly).

---

## 4. Auth: the token gate

One additive protocol request (`Request::Authenticate { token }` → `ResponseData::Authenticated`), **no `IPC_PROTOCOL_VERSION` bump** — additive-only, an old client never emits it, and the only transport that requires it (TCP) is new (D054). The gate is connection-scoped state in the serve core, not the transport (which stays protocol-free) and not the stateless dispatcher (which has no connection notion). The `Authenticated` ack is sent inline so it always precedes any buffered event. The token is a dedicated IPC secret — `MXR_DAEMON_TOKEN` env > `<config_dir>/daemon-token` (0600, atomic create), via `mxr_config::resolve_daemon_token` (`crates/config/src/resolve.rs:172`) — distinct from the HTTP bridge token (reusing it would leak: the bridge hands its token to any loopback caller). Comparison is constant-time; the `TcpConnector` refuses non-loopback targets so the token is never sent in plaintext off-machine.

---

## 5. Adapter kit — how to build a transport

Mirrors the provider adapter kit (`03-providers.md`). The whole point: a community member writes, conformance-tests, and ships an out-of-tree transport crate against published `mxr-transport` docs without reading daemon source.

**First question — do you need an in-tree trait impl at all?** The *recommended* community path is out-of-process: a `cmd://` connector over `mxr daemon dial-stdio` (`crates/daemon/src/cli/mod.rs`). SSH, containers, and tunnels all become one-line byte pipes with zero new daemon trust surface (Docker's `connhelper` model):

```
MXR_DAEMON_ADDR="cmd://ssh -T host mxr daemon dial-stdio"      # SSH
MXR_DAEMON_ADDR="cmd://docker exec -i box mxr daemon dial-stdio" # container
```

Implement `ServerTransport` / `Connector` directly **only** for a transport that genuinely cannot be byte-piped (a listener the daemon itself must own — a named pipe, an sd_listen_fds handoff, a vsock).

If you do need a trait impl, the checklist:

1. **Implement the traits.** `ServerTransport` + `TransportListener` + `Connector`, over your byte carrier. Surface real auth evidence in `PeerInfo` — never fabricate a `UnixPeer`; use `LocalProcess` for in-process or `TokenRequired` for a transport that must authenticate.
2. **Start with all-false capabilities.** `TransportCapabilities::default()`, then flip one flag on only when the property is genuinely provided — the daemon trusts it.
3. **Run conformance.** Add `mxr-transport` (feature `conformance`) as a dev-dependency and call the suite from a `#[tokio::test]`:

   ```rust
   #[tokio::test]
   async fn my_transport_conforms() {
       let transport = MyServerTransport::new(/* … */);
       let connector = MyConnector::new(/* same endpoint */);
       mxr_transport::conformance::run_transport_conformance(&transport, &connector).await;
       // For a token transport, also:
       // mxr_transport::conformance::run_token_auth_conformance(&transport, &connector).await;
   }
   ```

   `run_transport_conformance` pins the transport contract — bind, accept yields a connected bidirectional stream plus `PeerInfo`, `accept` is cancel-safe, `stop_accepting` refuses further connections without hanging, `cleanup` is idempotent, and the accepted peer's evidence is coherent with the advertised capabilities. It is **protocol-free**: it never frames through `IpcCodec`, never sends a `Request`, never touches an `AppState`.

4. **Study the references.** `UdsServerTransport` (`src/uds.rs`, the production reference) and `MemoryTransport` (`src/memory.rs`, behind `test-util`, the in-memory reference). `examples/transport-skeleton/` is a compilable all-stub starting point (standalone `[workspace]`, `publish = false`, depends only on `mxr-transport`).
5. **Package as a crate** depending on `mxr-transport` only.

### Transport conformance vs protocol conformance (the split)

`mxr-transport`'s `conformance` module tests the **transport contract** — what an adapter author must prove. The daemon's serve-core corpus (`crates/daemon/src/serve/ipc_conformance.rs`) tests **protocol behavior** — id correlation, out-of-order completion, lane back-pressure, event fan-out, framing edges, the `Authenticate` gate. That corpus already runs every scenario over the real UDS / in-memory / TCP transports through `bind`/`accept`/`connect`, so protocol behavior is proven transport-independent in-tree. An out-of-tree author does **not** re-run it (it needs `AppState`, the private serve loop, the lane constants — all daemon-internal); they only prove their byte stream and its lifecycle, which is exactly the transport suite. This split is what lets the transport suite live in a daemon-free leaf crate (D057).

---

## 6. Backlog (documented, deliberately unscheduled)

Each with the trigger that would schedule it (full table in `docs/transport-adapters/06-community-kit.md`):

| Adapter | Trigger | Notes |
|---|---|---|
| Windows named pipes | a Windows port becomes real (Q3) | `interprocess` crate (`local_socket`); a `PeerAuth::PipeIdentity` variant |
| systemd socket activation | Linux packaging demand | near-free via `sd_listen_fds`; `client_autostart = false` |
| vsock | mxr inside microVMs | host usually bridges to UDS anyway; likely community territory |
| WS-binary adapter | a browser-native non-REST client appears (Q1 revisited) | same frames over WebSocket; REST gateway unaffected |
| TLS/mTLS remote | an explicit decision to reverse Q2 | full security review first; discovery §7/§9 caveats |

Evaluate `tokio-listener` (TCP/UDS/stdio/sd_fds/vsock in one dependency) at the first backlog item it covers, versus continuing hand-rolled adapters.

### Known follow-ups

- **5d — in-process web bridge (deferred).** Q5's daemon-hosted bridge over an in-memory `Connector` (skip the socket round-trip to itself) is a latency-only win with no behavior change. Deferred because it requires rethreading `mxr-web`'s ~50 `socket_path` call sites onto a `Connector`; the landing seam is the two connect points in `crates/web/src/lib.rs` (`ipc_request_with_id`, `bridge_events`). See D054.
- **Phase-4 UDS `stop_accepting` test flake.** `uds::tests::stop_accepting_refuses_new_connections_but_defers_unlink` passes in isolation but can fail under heavy parallel test load: after `stop_accepting` drops the `UnixListener`, a racing `connect` can momentarily complete into the kernel's not-yet-drained backlog before the socket fully closes, so the expected `ConnectionRefused` does not always arrive on the first attempt. A timing artifact of the test's single-attempt assertion, not a transport-contract bug (the socket *is* closed; the refusal is merely not instantaneous). Follow-up: make the test's post-`stop_accepting` connect assertion tolerant of a brief window (retry-until-refused with a bound, or accept an immediate-EOF connection), mirroring how `run_transport_conformance` already tolerates a post-stop connection that immediately EOFs.

---

## 7. References

- Research & rationale: `docs/transport-adapters/00-discovery.md` (§5 prior art, §7 security table, §9 product caveats).
- Phase specs: `docs/transport-adapters/0{1..6}-*.md`.
- Decisions: D052 (HTTP gateway), D053 (crate home), D054 (phase-5 transports + token gate), D055 (byte-stream abstraction), D056 (auth evidence in PeerInfo), D057 (conformance split) in `15-decision-log.md`.
- Crate boundaries: `crate-boundary-audit.md`.
