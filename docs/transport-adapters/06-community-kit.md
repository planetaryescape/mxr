# Phase 6 — Community Adapter Kit & Docs

Adapter-specific: **yes**. Packaging, documentation, and the long tail — the phase that turns "we have adapters" into "the community can add adapters."

## Goal

Mirror the provider system's community affordances one-to-one: reusable conformance functions, a compilable skeleton, and a blueprint chapter with an adapter-kit checklist.

## 6a. Conformance packaging

- Extract the corpus scenarios (phases 2–5) into reusable generic functions, provider-precedent placement: they live with the reference implementation. Options, in preference order:
  1. `mxr-transport` under a `conformance` feature (reference `MemoryTransport` already lives there behind `test-util`);
  2. a dedicated `crates/transport-conformance` if dev-dep weight in `mxr-transport` gets awkward.
- Shape mirrors `run_sync_conformance<P>` (`crates/provider-fake/src/conformance.rs`): 
  `run_transport_conformance<T: ServerTransport + ?Sized>(transport: &T)` plus `run_token_auth_conformance` for `TokenRequired` transports — capability-conditional sections exactly like the provider suite gates label tests on `capabilities().mutate.labels`.
- Out-of-tree adapters opt in via dev-dependency + one `#[tokio::test]`, identical to how provider-gmail/imap/smtp consume the provider suite. The suite needs a daemon serve core to test against — expose a minimal test harness (fake-provider-backed `AppState` + serve loop) from `crates/test-support` so out-of-tree adapters don't need daemon internals.

## 6b. Transport skeleton example

`examples/transport-skeleton/` mirroring `examples/adapter-skeleton/`:

- Standalone `[workspace]`, `publish = false`, depends only on `mxr-transport` (+ conformance dev-dep).
- `ExampleServerTransport` + `ExampleConnector` with every method returning a "not implemented" error and `TransportCapabilities::default()` (all-false), with the same inline warning the provider skeleton carries: *flip a capability only when the behavior is real — the daemon trusts this.*
- A commented `#[tokio::test]` invoking the conformance suite.

## 6c. Blueprint & docs

- New `docs/blueprint/20-transports.md`: architecture (frozen protocol / byte-stream adapters / HTTP gateway), the trait reference, `PeerInfo`/capabilities semantics, the per-transport security policy table (discovery §7), the adapter-kit checklist (implement traits → run conformance → all-false capabilities first → study `MemoryTransport`/`UdsServerTransport` as references → package as a crate depending on `mxr-transport` only), and the out-of-process story (`dial-stdio`, `cmd://`) as the *recommended* community path — in-tree trait impls are for transports that genuinely can't be byte-piped.
- `docs/blueprint/15-decision-log.md` entries: byte-stream-level abstraction (Podman/tarpc rationale), HTTP-as-gateway, remote-is-out-of-process, auth-evidence-in-PeerInfo.
- Update `docs/blueprint/01-architecture.md`, `crate-boundary-audit.md`, `09-cli.md` (`MXR_DAEMON_ADDR`, `dial-stdio`), and the docs site / README surface where connection options are user-facing.
- Retire/annotate superseded bits of `docs/blueprint/ipc-audit.md` if the transport chapter absorbs its transport-adjacent conclusions; log the journey in `docs/implementation-journey.md`.

## 6d. Backlog (documented, deliberately unscheduled)

| Adapter | Trigger | Notes |
|---|---|---|
| Windows named pipes | Q3: a Windows port becomes real | `interprocess` crate (`local_socket` = UDS/pipes portably); `PeerAuth::PipeIdentity` variant |
| systemd socket activation | Linux packaging demand | near-free via `tokio-listener` or direct `sd_listen_fds`; capability `client_autostart = false` |
| vsock | mxr inside microVMs | host side usually bridges to UDS anyway; likely community territory |
| WS-binary adapter | Q1 revisited: a browser-native non-REST client appears | same frames over WebSocket; REST gateway unaffected |
| TLS/mTLS remote | explicit product decision to reverse Q2 | full security review first; discovery §7/§9 caveats apply |

Evaluate `tokio-listener` adoption at the first backlog item that it covers (TCP/UDS/stdio/sd_fds/vsock in one dependency) versus continuing hand-rolled adapters.

## Verification

- Skeleton compiles standalone (`cargo check` in `examples/transport-skeleton/`) in CI, like the adapter skeleton.
- Conformance suite consumed from outside the workspace at least once (scratch out-of-tree crate) to prove the out-of-tree story actually works — the provider kit's claim, verified for transports.
- Docs pass: every code path referenced in `20-transports.md` exists at the cited location (verify-before-teaching rule).

## Exit criteria

A community member can write, conformance-test, and ship an out-of-tree transport crate against published `mxr-transport` docs without reading daemon source; the blueprint chapter is the single reference; backlog triggers recorded. The transport system now has everything the provider system has: trait, capabilities, conformance, skeleton, blueprint, and a decision log trail.
