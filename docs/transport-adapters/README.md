# Transport Adapters — Implementation Plan

Goal: make the daemon's client transport pluggable — Unix socket today, TCP/stdio/community transports tomorrow — without destabilizing the protocol, the daemon, or the clients.

Research and rationale: [00-discovery.md](00-discovery.md). Design in one line: **freeze the protocol, abstract the byte stream, keep HTTP as a gateway, community transports out-of-process.**

## Phasing principle

Phases are ordered by **standalone value**: each of the first three phases is a worthwhile change even if the adapter system is never built. The adapter commitment happens at phase 4. You can stop after any phase and the codebase is strictly better than before.

| Phase | Deliverable | Value if adapters never ship | Adapter-specific? |
|---|---|---|---|
| [1](01-shared-client.md) | Shared IPC client crate (`mxr-client`) | Deletes 4× duplicated connect/frame/correlate logic; one place for timeouts, errors, `ClientKind` tagging | No |
| [2](02-ipc-conformance.md) | IPC conformance corpus (characterization tests over today's UDS) | Executable spec of protocol v4; regression net for any future server work | No |
| [3](03-generic-serve-core.md) | Serve core generic over `AsyncRead + AsyncWrite` | Hermetic, fast connection-layer tests via in-memory duplex; no socket files in tests | Leaning |
| [4](04-transport-traits.md) | `ServerTransport` / `Connector` traits; UDS becomes adapter #1 | — (this is the commitment point) | Yes |
| [5](05-tcp-stdio-adapters.md) | TCP-loopback+token adapter, stdio adapter, `mxr daemon dial-stdio` | — | Yes |
| [6](06-community-kit.md) | Conformance packaging, transport skeleton example, blueprint doc, backlog | — | Yes |

Dependency chain: 1 → 2 → 3 → 4 → 5 → 6. Phases 1 and 2 could technically run in parallel, but the corpus is cleaner written against the shared client, so sequence them.

## Ground rules (apply to every phase)

- The wire protocol (`IpcMessage`, `Request`, `ResponseData`, `DaemonEvent`, `IpcCodec` framing, `IPC_PROTOCOL_VERSION`) does not change, with one narrow exception gated to phase 5 (an `Authenticate` request for token-bearing transports, additive only).
- TUI and CLI keep speaking the same daemon requests; no client-only capabilities (repo invariant).
- Crate boundaries stay real Cargo dependencies; update `docs/blueprint/crate-boundary-audit.md` whenever a phase adds a crate or edge.
- Every phase ends green: `scripts/cargo-test -p <touched crates> --tests`, then the full suite once per PR, then live verification through the CLI against a running daemon where behavior could differ.
- Minimal blast radius per phase; refactors land behind unchanged public behavior.

## Decision gates

Answer before (or during) the phase that needs them:

| # | Question | Needed by | Recommendation |
|---|---|---|---|
| Q1 | HTTP stays a gateway only, or also a WS-binary byte-stream adapter for browser-native clients? | Phase 4 | Gateway only; revisit WS-binary if a non-REST browser client appears |
| Q2 | Remote (off-machine) access ever in scope? | Phase 5 | No in-daemon remote; SSH via `dial-stdio` covers it (containerd/Docker model) |
| Q3 | Windows port on the roadmap? | Phase 6 | If yes, named pipes via `interprocess` becomes a phase-6 adapter; otherwise backlog |
| Q4 | Trait home: new `mxr-transport` leaf crate vs inside `mxr-protocol`? | Phase 4 | New `crates/transport` crate — keeps protocol purely a wire contract |
| Q5 | Daemon-hosted web bridge switches to in-process transport (skip the socket round-trip to itself)? | Phase 5/6 | Yes, once the in-memory transport exists; measurable latency win, no behavior change |
| Q6 | ~~Fix `ClientKind` mislabel + orphan `web/src/ipc.rs`~~ | — | Done 2026-07-18 (see discovery §8) |

## Status log

- 2026-07-18 — discovery complete; plan drafted; pre-existing web-bridge issues fixed (`ClientKind::Web`, orphan `ipc.rs` deleted).
- 2026-07-18 — Phase 2 corpus implemented (14/14 scenarios, double-run clean); two behaviors pinned: event source tag = Cli, silent close on malformed frames.
