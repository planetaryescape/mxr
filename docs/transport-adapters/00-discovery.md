# Transport Adapters — Discovery & Research

Status: research complete, plan drafted
Date: 2026-07-18
Codebase state: commit `3950d80c` (line references below are as of this commit and will drift)
Method: three parallel codebase surveys (socket transport, web bridge, provider adapter pattern) plus prior-art research across Docker, LSP/MCP, Tailscale, Caddy, containerd, Podman, systemd, and the Rust RPC ecosystem.

## 1. The question

The daemon's client transport is a Unix domain socket, hard-coupled at the edges. The idea under evaluation: a **daemon transport adapter system** in the same spirit as the mail provider adapter system — a trait, multiple adapters (UDS, HTTP, future community transports), and a conformance suite.

**Verdict: sound idea, wrong default layer.** The provider *process* (leaf-crate trait, capability flags, conformance suite, skeleton example, blueprint doc) transfers well. The provider *abstraction level* does not: providers abstract wide behavioral surfaces; transports must abstract only "where bytes come from." Concretely:

- **Freeze the protocol.** `IpcMessage` / `Request` / `ResponseData` / `DaemonEvent` + `IpcCodec` framing stay identical on every transport.
- **Abstract the byte stream.** Adapters produce connected `AsyncRead + AsyncWrite` streams plus peer/auth evidence — nothing more.
- **HTTP is a gateway, not an adapter.** The web bridge is a presentation layer that *uses* a transport; forcing it to implement the same trait as UDS would misshape the trait (evidence in §3).
- **Community extensibility is out-of-process.** A `dial-stdio` proxy command makes SSH/container/tunnel transports possible as shell scripts with zero new daemon trust surface (Docker's `connhelper` model).

## 2. Current state: the Unix-socket transport

### Crate topology

`mxr-protocol` is the leaf wire contract (depends only on `mxr-core`); `daemon`, `tui`, `web`, `mcp` all depend on it. Socket path resolution lives in `mxr-config` (`crates/config/src/resolve.rs:242` — `MXR_SOCKET_PATH` override, per-instance path, `mxr` vs `mxr-dev`).

### Already transport-agnostic (the good news)

| Layer | Where | Note |
|---|---|---|
| Framing/serialization | `crates/protocol/src/codec.rs` | `IpcCodec` wraps `LengthDelimitedCodec`: 4-byte length prefix, JSON body, 16 MiB max frame. Generic over any `AsyncRead + AsyncWrite`. |
| Dispatch | `crates/daemon/src/handler/mod.rs:416` | `handle_request(&state, &IpcMessage) -> IpcMessage` — no socket types anywhere. |
| Eventing | `crates/daemon/src/server.rs:386-415` | `broadcast::Sender<IpcMessage>` fan-out; events multiplexed on the same connection as `IpcPayload::Event` frames (`id: 0`); `EventsLagged` resync signal on backpressure. No separate event socket. |
| Concurrency | `server.rs:36-41, 288-424` | Task per connection; per-request tasks gated by Hot (64) / Bulk (8) semaphores; out-of-order responses correlated by `IpcMessage.id`; `guard_ipc_response` catches handler panics. |

Protocol version: `IPC_PROTOCOL_VERSION = 4` (`crates/protocol/src/lib.rs:35`). `Request` is one flat `#[serde(tag = "cmd")]` enum (~150 variants); `Response::Error` carries a kinded envelope (`IpcErrorKind`, retryable flag, code).

### Unix-bound surface (the coupling inventory)

| What | Where |
|---|---|
| Server bind | `UnixListener::bind` — `server.rs:106` (sole production bind) |
| Accept loop + connection fn typed to `UnixStream` | `server.rs:222-260`, `serve_client_connection` at `server.rs:288` |
| Socket permissions | chmod `0o600`, `server.rs:276-281` |
| Stale-socket lifecycle | liveness probe `server.rs:629-643`; `socket_file_identity` (dev/ino/mtime successor detection) `server.rs:660-679`; `remove_file` cleanup at `:105, :271, :475, :884, :1051` |
| Pid file | `data_dir()/daemon.pid`, `server.rs:655, 708` |
| Version/build handshake | `daemon_requires_restart` compares `protocol_version` + `daemon_version` + `current_build_id` (path+size+mtime), `server.rs:547-564, 898` |
| Autostart | CLI re-execs `current_exe() daemon` detached, polls `Ping` (`server.rs:458, 1267, 1343`); TUI equivalent in `crates/tui/src/ipc.rs:206-233` |

### Four duplicated clients

Each reimplements `UnixStream::connect` + `Framed<_, IpcCodec>` + an `AtomicU64` id counter:

1. **CLI / daemon-internal** — `crates/daemon/src/ipc_client.rs` (`IpcClient`): 120 s default timeout, `request_with_events`, `notify`, `next_event`. Used by ~58 command files. Tags `ClientKind::Cli`.
2. **TUI** — `crates/tui/src/client.rs` + worker in `crates/tui/src/ipc.rs`: persistent connection, infinite reconnect with `ConnectionState` transitions, 60 s per-request bound, retry-safe request classification, dedicated throwaway connections for slow LLM calls. Tags `ClientKind::Tui`.
3. **Web bridge** — `crates/web/src/lib.rs:1670-1746`: fresh connection per HTTP request; `bridge_events` opens an event-only connection and relays frames to WebSocket. Tags `ClientKind::Web` (fixed 2026-07-18; previously `ClientKind::default()` = `Cli` — see §8).
4. **MCP** — `crates/mcp/src/lib.rs`: connection per call, **already behind a `DaemonRequester` trait** (`lib.rs:23-25`) with `UnixDaemonRequester` the sole impl. This is the client-side transport adapter in miniature — the existing proof of concept.

## 3. Current state: the web bridge (why HTTP is a gateway)

`crates/web` (`mxr-web`, axum) depends only on `mxr-protocol` + support crates — **not** on `mxr-daemon`. It translates HTTP routes into the same `Request` enum and forwards frames over the unix socket (`ipc_request_with_id`, `lib.rs:1674-1707`). Even the daemon-hosted bridge (`crates/daemon/src/bridge.rs:40`, `spawn_bridge_loop`) round-trips through its own socket rather than calling `handle_request` in-process. Events go out as a WebSocket at `/api/v1/events`: an event-only socket connection relayed frame-by-frame (`bridge_events`, `lib.rs:1709-1746`).

The decisive measurement: **~100 lines of the bridge are transport plumbing** (connect, frame, forward, relay events). The remaining **~5,500 lines are presentation**: per-route handlers and multi-IPC view-model assembly (`lib.rs`, `routes_v6.rs`, `chrome.rs`, `envelope_list.rs`), SPA serving (`spa.rs`, `web-ui` feature), OpenAPI/Swagger (`openapi.rs`), legacy redirects, and web security posture (bearer token at `bridge_token_path()` mode 0600, `Sec-WebSocket-Protocol` token passing, loopback-only bind enforcement `bridge.rs:119-129`, CORS + Host allowlist vs DNS rebinding, security headers).

Conclusion: the bridge is a **REST+WS presentation gateway sitting on a thin transport core**. In an adapter system it *consumes* the client transport; it is not an implementation of the server transport trait. Its security posture (token infra, loopback enforcement) is, however, directly reusable by a future TCP adapter.

## 4. The template: how the provider adapter system is shaped

Full anatomy in `docs/blueprint/03-providers.md`; the shape worth mirroring:

- **Leaf-crate, object-safe traits** — `MailSyncProvider` / `MailSendProvider` in `crates/core/src/provider.rs`, `#[async_trait]`, no associated types, used only as `Arc<dyn _>` / `Box<dyn _>`.
- **Opaque per-adapter state** — `SyncCursor(Vec<u8>)`: the daemon persists/replays without inspecting; adapters own serialization. (Transport analog: adapter-private lifecycle state such as socket identity.)
- **Namespaced additive capabilities** — `SyncCapabilities` with four sub-structs, all-false defaults, `#[serde(default)]` per field: "a missing field never means unsupported, only that the older shape doesn't carry this signal yet" (`crates/core/src/types.rs:2001-2073`). Capabilities drive real routing and are trusted by the daemon.
- **Factory match, not a registry** — `create_providers_from_config` (`crates/daemon/src/state.rs:~590-871`) is the only place concrete types are named; everything downstream is `dyn`.
- **Conformance suite as generic functions in the reference-impl crate** — `run_sync_conformance<P: MailSyncProvider + ?Sized>` / `run_send_conformance` in `crates/provider-fake/src/conformance.rs`; adapters add `mxr-provider-fake` as a dev-dependency and call them from a normal `#[tokio::test]` (call sites: provider-gmail, provider-imap, provider-smtp, provider-fake self-test).
- **Compilable all-stub skeleton** — `examples/adapter-skeleton/` (standalone `[workspace]`, `publish = false`, depends only on `mxr-core`), capabilities all-false with the comment "the daemon trusts this."
- **Blueprint "adapter kit" checklist** — `03-providers.md:375-405`.

## 5. Prior art

The consistent finding: **successful projects freeze the message protocol and abstract only the listener/dialer.** Projects that abstracted the RPC layer itself reversed course.

| Project | Abstraction chosen | Lesson |
|---|---|---|
| Docker | Protocol frozen (HTTP REST). Transport = `Dialer func(...) (net.Conn, error)` ([connhelper](https://github.com/docker/cli/blob/master/cli/connhelper/connhelper.go)). `ssh://` = exec `ssh <host> docker system dial-stdio`, subprocess stdio wrapped as a conn. `docker context` for named endpoints. | Community transports = out-of-process byte pipes; zero daemon changes. |
| LSP | JSON-RPC + Content-Length framing frozen; transports are stdio/socket/pipe/node-ipc. No official conformance suite — and server quality varies accordingly. | Spec the message stream; ship the conformance suite LSP never had. |
| MCP | stdio + Streamable HTTP normative; "MAY implement additional custom transports" provided the JSON-RPC format and lifecycle are preserved ([spec](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)). | Pluggability can be spec-sanctioned while the message layer stays frozen. |
| Tailscale | [`safesocket`](https://pkg.go.dev/tailscale.com/safesocket): UDS / Windows named pipe / (sandboxed macOS) localhost port + shared-secret token. | Auth evidence is per-transport; the abstraction must surface it, not just bytes. |
| Caddy | HTTP everywhere; UDS is a listener with a different security policy (origin checks off for UDS, mTLS required for remote) ([admin.go](https://github.com/caddyserver/caddy/blob/master/admin.go)). | Per-listener security policy table. |
| containerd | gRPC over UDS only; refused TCP — "run a proxy" ([#1324](https://github.com/containerd/containerd/issues/1324)). | Saying no to remote is a valid, safe default. |
| **Podman (regret)** | v1 API = varlink (novel RPC layer). Ecosystem wouldn't rewrite Docker-API tooling; v2 deleted it for Docker-compatible REST over UDS ([deprecation](https://podman.io/blogs/2020/08/01/deprecate-and-remove-varlink-notice.html)). | The ecosystem premium is on the protocol, not transport pluggability. |
| systemd | `sd_listen_fds` (transport = deployment config); own IPC migrating D-Bus → varlink = newline JSON over UDS + `SO_PEERCRED` ([LWN](https://lwn.net/Articles/1002398/)). | Industry converging on what mxr already has: plain JSON over UDS with peer creds. |
| tarpc | Genuine `Transport` trait — over *Rust-typed* messages. | Typed transports exclude curl/jq/scripts/non-Rust agents; against mxr's CLI-first JSON shape. |
| [tokio-listener](https://docs.rs/tokio-listener) | Listener enum: TCP, UDS (+ Linux abstract), stdio/inetd, `sd_listen_fds`, vsock; each connection is `AsyncRead + AsyncWrite`. | ~80% of the server-side adapter surface already exists as a crate; its type list matches observed demand exactly. |
| [connectrpc/conformance](https://github.com/connectrpc/conformance) | Data-driven scenario corpus executed by reference client/server across protocols. | The conformance model to transplant: one corpus, every adapter. |

Transports people demonstrably want (frequency-ordered from the survey): TCP+TLS (remote), SSH (Docker/Podman default remote), stdio (spawn-as-child, agent embedding), WebSocket (browsers), Windows named pipes, vsock (microVMs), systemd socket activation.

## 6. Recommended architecture

```
clients (CLI, TUI, MCP, web gateway, scripts)
        │ ClientTransport / Connector trait  ←  connect() -> byte stream
        ▼
FROZEN protocol: IpcMessage / Request / ResponseData / DaemonEvent + IpcCodec
        ▲
        │ ServerTransport trait  ←  accept() -> (byte stream, PeerInfo)
        ▼
adapters: UDS (default) · in-memory duplex (tests) · TCP-loopback+token · stdio
          [community, out-of-process: ssh/containers/tunnels via `mxr daemon dial-stdio`]

HTTP bridge (mxr-web): REST+WS+SPA gateway — consumes ClientTransport, is not an adapter.
```

Key trait design points (details in phase docs):

- `ServerTransport::accept() -> (stream, PeerInfo)` where `PeerInfo` carries auth evidence: `UnixPeer { uid, .. }` | `TokenRequired` | (future) `TlsClientCert`. The Tailscale lesson: identity evidence is part of the transport contract.
- Namespaced additive `TransportCapabilities` with all-false defaults (locality/same-machine, implicit-auth, autostart-supported) — the provider-capabilities lesson.
- The serve core (lanes, task-per-connection, event fan-out, panic guard, `EventsLagged`) is **shared**, generic over the stream; adapters only produce connections. Otherwise every adapter reimplements backpressure and the conformance suite fragments.
- Client side: promote the `DaemonRequester` shape; one shared client generic over a `Connector`.
- Conformance: one scenario corpus (id correlation, out-of-order responses, concurrent lanes, event push, `EventsLagged`, 16 MiB frame edge, malformed frames, disconnects, auth paths) run over every adapter — in-memory duplex is the reference transport (the "fake provider" analog).

## 7. Security consequences (per-transport policy)

Leaving UDS-only removes implicit protections one at a time. Policy table (Caddy/Tailscale model):

| Transport | Implicit auth | Required policy |
|---|---|---|
| UDS | fs perms (0600) + peer creds | none extra (today's posture) |
| In-memory | in-process | none |
| TCP loopback | **none** — any local process, and browsers via DNS rebinding / `0.0.0.0` tricks | bearer token even on loopback; refuse non-loopback bind without TLS (posture already implemented in the bridge — reuse it) |
| stdio | inherits spawning process's trust | none extra; the spawner is the authenticator |
| TCP remote | none | mTLS or refuse (Docker 2376 lesson; unauthenticated 2375 is a canonical RCE vector) — recommend deferring remote entirely |

## 8. Issues found during discovery (both fixed 2026-07-18)

1. `crates/web/src/ipc.rs` was an orphan — no `mod ipc;` declaration anywhere; dead duplicate of the live client code in `lib.rs`. **Deleted.**
2. The live bridge sent `source: ClientKind::default()` (= `Cli`) on IPC requests (`lib.rs:1685`), mis-attributing web traffic in activity surfaces. **Now sends `ClientKind::Web`.** (Ironically the orphan file had it right.)

Verified with the full `mxr-web` suite: 53 unit + 6 integration tests, 0 failures.

## 9. Product caveats

**Remote transports change the product, not just the plumbing.** Same-machine assumptions run deep: compose sessions are markdown files on disk keyed by path (`$EDITOR` flow), attachment materialization uses local paths, the build-id handshake compares the local binary, autostart re-execs `current_exe()`. A `locality` capability keeps these honest; genuine remote support should be its own project — or stay permanently out-of-process (`dial-stdio` over SSH), the containerd answer.

## 10. Open decision gates

See `README.md` in this directory — six questions (HTTP gateway vs WS-binary adapter, remote scope, Windows roadmap, trait home, in-process bridge transport, and phasing of optional adapters), each with a recommendation.
