# Phase 4 — Transport Traits; UDS Becomes Adapter #1

Adapter-specific: **yes**. This is the commitment point (decision gates Q1/Q4 in the README).

## Goal

Define the transport seam as traits, move the Unix socket behind them as the first server adapter, and make the shared client generic over a connector. After this phase, adding a transport means implementing a small trait — not editing the daemon.

## Design

### Crate

New leaf crate `crates/transport` (`mxr-transport`) — recommendation for Q4. Depends on `mxr-protocol` (for `ClientKind` interplay and codec re-export convenience) + `tokio`, `async-trait`, `futures`. Mirrors provider discipline: object-safe `#[async_trait]` traits over concrete shared types, no associated types, consumed as `Box<dyn _>` / `Arc<dyn _>`.

### Server side

```rust
pub type BoxedIo = Box<dyn AsyncReadWrite + Send + Unpin>; // blanket-impl'd combo trait

#[async_trait]
pub trait ServerTransport: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> TransportCapabilities;
    /// Bind and return a listener. Called once at daemon startup.
    async fn bind(&self) -> Result<Box<dyn TransportListener>>;
}

#[async_trait]
pub trait TransportListener: Send {
    async fn accept(&mut self) -> Result<(BoxedIo, PeerInfo)>;
    /// Release transport-owned resources (socket file, etc.). Idempotent.
    async fn cleanup(&mut self) -> Result<()>;
    /// Human-readable endpoint for logs/status ("unix:/path", "tcp:127.0.0.1:port").
    fn endpoint(&self) -> String;
}
```

- `PeerInfo` — auth evidence, the Tailscale lesson:

  ```rust
  pub enum PeerAuth {
      UnixPeer { uid: u32, gid: u32, pid: Option<i32> }, // SO_PEERCRED / getpeereid
      TokenRequired,       // transport provides no identity; dispatch must demand a token (phase 5)
      // future: TlsClientCert { .. }, PipeIdentity { .. }
  }
  pub struct PeerInfo { pub auth: PeerAuth, /* room to grow additively */ }
  ```

- `TransportCapabilities` — namespaced, additive, all-false defaults (the provider-capabilities lesson, `types.rs:2001-2073` precedent):

  ```rust
  #[derive(Default)]
  pub struct TransportCapabilities {
      pub locality: LocalityCaps,   // same_machine: bool  — guards $EDITOR-compose, file paths, build-id logic
      pub auth: AuthCaps,           // implicit_peer_identity: bool, token: bool
      pub lifecycle: LifecycleCaps, // client_autostart: bool — can a client start the daemon over this transport?
  }
  ```

  The daemon trusts these (provider rule); the skeleton example (phase 6) ships all-false.

### Client side

```rust
#[async_trait]
pub trait Connector: Send + Sync {
    async fn connect(&self) -> Result<BoxedIo>;
    fn describe(&self) -> String;
}
```

- `mxr-client`'s `IpcConnection` gains `connect_with(connector: &dyn Connector, kind: ClientKind)`; the existing path-based constructor becomes `UnixConnector` internally. The MCP `DaemonRequester` stays as-is — it's the layer above, now trivially generic.
- Address scheme (Docker precedent): `unix://<path>` parsed by a small `TransportAddr` type in `mxr-transport`. Only `unix://` exists this phase; `tcp://` and `cmd://` arrive in phase 5. Resolution order: explicit flag/env (`MXR_DAEMON_ADDR`) → config → default per-instance socket path (current behavior unchanged when unset).

### Daemon integration

- `run_daemon_with_overrides` (`server.rs:64`) builds transports via a **factory match over config** (provider pattern, `create_providers_from_config` precedent): today one arm, `UdsServerTransport`.
- `UdsServerTransport` absorbs the UDS lifecycle currently inline in `server.rs`: bind + chmod 0600 (`:105-107, :276-281`), stale-socket inspection/cleanup (`:617-643, :684`), `socket_file_identity` successor detection (`:660-679`). Pid file and singleton index-lock stay daemon-level (they are daemon lifecycle, not transport lifecycle).
- Accept loop iterates listeners generically; multiple simultaneous transports are structurally supported from day one (Vec of listeners), even though only one is configured this phase.
- `PeerInfo` flows into the dispatch context alongside `ClientKind` — no policy change this phase (UDS peers keep today's implicit trust), but the plumbing point exists for phase 5's token gate and future per-transport policy in `dispatch` (`handler/mod.rs:458`), where safety-policy/client-profile enforcement already lives.

### Conformance

The phase-2/3 corpus gains a third parameterization: run scenarios against any `ServerTransport` impl. In-tree carriers now: `UdsServerTransport`, in-memory duplex (formalized as `MemoryTransport` in `mxr-transport` behind a `test-util` feature — the "fake provider" analog).

## Non-goals

- No second real transport (phase 5 proves the trait; shipping both in one phase blurs the review).
- No auth/token enforcement changes.
- No wire protocol changes.
- HTTP bridge untouched (it consumes `mxr-client`, which is the point — Q1 resolved as "gateway").

## Docs to update in this phase

- `docs/blueprint/crate-boundary-audit.md` — new crate + allowed edges (`transport` depends on `protocol`; `client` and `daemon` depend on `transport`; `tui`/`web`/`mcp` still must not depend on `daemon`).
- `docs/blueprint/01-architecture.md` — transport seam diagram.
- `docs/blueprint/15-decision-log.md` — entries for Q1 (HTTP = gateway) and Q4 (crate home), with the Podman/tarpc rationale from discovery.

## Verification

- Corpus green over `UdsServerTransport` + `MemoryTransport`.
- `scripts/cargo-test -p mxr-transport -p mxr-client -p mxr --tests` (one at a time); full suite before PR.
- Live: daemon restart cycle ×2 (stale-socket path now inside the adapter — verify recovery from a killed daemon leaving a socket file), `mxr doctor`, CLI/TUI/web/MCP smoke, `mxr status` showing the transport endpoint.

## Risks

- **Stale-socket/lifecycle regressions** — the riskiest code moved this phase (successor detection, cleanup ordering vs the index-lock singleton guard, `server.rs:81-100` comment). Mitigation: move verbatim behind the trait; the existing daemon lifecycle tests (`server.rs:1704+`) plus a dedicated kill-and-restart live check.
- Object-safety friction (`BoxedIo` double-boxing with `Framed`). Accepted: one boxed indirection per connection is noise next to JSON serialization; provider crates made the same trade.
- Trait too wide too early. Guard: anything not needed by both UDS and Memory this phase stays off the trait.

## Exit criteria

Daemon serves exclusively through `ServerTransport`; no `UnixListener`/`UnixStream` in daemon code outside `UdsServerTransport` (probe/doctor callers go through `Connector`); clients connect via `Connector`; corpus is transport-parameterized; boundary docs updated; live daemon behavior unchanged.
