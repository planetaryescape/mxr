# Phase 5 — Prove the Seam: TCP-Loopback+Token, stdio, `dial-stdio`

Adapter-specific: **yes**. Two real adapters with opposite auth models, plus the community escape hatch. Decision gates Q2 (remote) and Q5 (in-process bridge) land here.

> **Implemented 2026-07-19.** 5a (TCP+token), 5b (`mxr daemon --stdio`), and 5c (`cmd://` connector) shipped; 5d (in-process bridge) is **deferred** (see §5d). What follows is the design; deltas from the shipped code are called out inline as **Implemented:** notes.
>
> **Protocol-version ruling (the one additive change):** `Request::Authenticate { token }` + `ResponseData::Authenticated` were added **without** bumping `IPC_PROTOCOL_VERSION` (stays `4`). Rationale: the change is additive-only on `#[serde(tag)]` enums; an old client never emits the new request, and the only transport that requires it (TCP) is itself new, so no existing UDS exchange changes shape. `daemon_requires_restart` already forces a restart on any binary upgrade via the build-id handshake (`current_build_id` compares path+size+mtime), so a version bump would add nothing but spurious restart churn for same-build clients. The compatibility rule ("bump only if additive variants require it") is therefore satisfied by leaving it at 4.
>
> **Token precedence (documented once):** the daemon bearer token — shared by the HTTP bridge and the TCP transport, one file — resolves as `MXR_DAEMON_TOKEN` (env, non-empty) **>** the token file (`bridge_token_path()`, mode 0600). `mxr_config::resolve_daemon_token(create)` is the single resolver; the daemon creates on first run, clients read-only.
>
> **`cmd://` arg parsing (documented limit):** the `cmd://` body is split on ASCII whitespace into argv — **no shell quoting, escapes, globbing, or variable expansion**. An argument that must contain whitespace can't be expressed; wrap it in a script and point `cmd://` at the script.

## Goal

Demonstrate the trait carries transports with different security shapes without daemon changes, and ship the out-of-process extensibility story.

## 5a. TCP loopback + token adapter

The first transport with **no implicit peer identity** — it forces the `PeerAuth::TokenRequired` path and the per-transport policy table from discovery §7.

- **Bind policy:** loopback only; refuse non-loopback outright. Reuse the posture (and ideally the code) of `enforce_non_loopback_safety` (`crates/daemon/src/bridge.rs:119-129`) — Q2 resolved as "no in-daemon remote."
- **Token:** reuse the bridge's token infrastructure (`load_or_create_token`, `bridge_token_path()`, mode 0600, `bridge.rs:134`) — likely promoted to a shared location so bridge and TCP adapter use one token store.
- **Auth handshake (the one protocol addition, additive only):** raw framed IPC has no headers, so token-bearing transports need an in-band handshake:
  - Add `Request::Authenticate { token: String }` → `ResponseData::Authenticated`.
  - Dispatch gate: when `PeerInfo.auth == TokenRequired`, every request before a successful `Authenticate` on that connection gets `IpcErrorKind::Auth`. Connection-scoped flag in the serve core's per-connection state.
  - UDS/duplex/stdio are unaffected (`UnixPeer` / inherited trust). Old clients on UDS see zero change; `IPC_PROTOCOL_VERSION` bumps only if the compatibility rules in `docs/blueprint/` say additive variants require it — check `daemon_requires_restart` implications (`server.rs:547-564`) before deciding.
- **Client:** `tcp://127.0.0.1:PORT` in `TransportAddr`; `TcpConnector` sends `Authenticate` automatically when constructed with a token (env `MXR_DAEMON_TOKEN` or token-file path).
- **Config:** opt-in `[transports.tcp]` (disabled by default); UDS remains always-on default.
- **Capabilities:** `locality.same_machine = true` (loopback), `auth.token = true`, `lifecycle.client_autostart = false`.
- **Who wants it:** containers/WSL setups where UDS mounts are awkward; also the forcing function that proves the auth half of the trait.

## 5b. stdio adapter (server side)

`mxr daemon --stdio`: serve exactly one connection over the daemon process's stdin/stdout (LSP/inetd model).

- Implementation is nearly free after phase 3: `tokio::io::join(stdin, stdout)` handed to the serve core; process exits when the stream closes (connection lifetime = process lifetime).
- `PeerAuth`: inherited trust — the spawner is the authenticator (discovery §7). Capabilities: `same_machine = true`, `client_autostart = false`.
- Caveat to handle: logging must not write to stdout in this mode (frames own it) — audit tracing/println paths, route logs to stderr/file.
- Who wants it: agent embedding (spawn daemon as child), inetd-style supervision, and it's the transport MCP/LSP tooling understands natively.

## 5c. `mxr daemon dial-stdio` (client-side proxy — the community unlock)

> **Implemented early (2026-07-18):** shipped as an independent side-track ahead of phase 4. The subcommand, autostart, stdout discipline, docs, and integration tests are in place; the `cmd://` connector below stays deferred to the rest of phase 5.

The Docker `connhelper` move: a subcommand that connects to the local daemon socket and pipes bytes stdin↔socket (`tokio::io::copy_bidirectional`). ~50 lines, no new trust surface — the remote user still needs local UDS access on the daemon machine.

- Enables today, with zero further daemon work:
  - SSH remoting: `ssh -T host mxr daemon dial-stdio` (`-T`: no PTY — a PTY corrupts the byte stream) — bytes flow, full protocol, events included.
  - Container access: `docker exec -i <c> mxr daemon dial-stdio`.
  - Any community bridge that can exec a process and pipe stdio.
- **`cmd://` connector (stretch, recommended):** `CmdConnector` spawns a command and wraps its stdio as the byte stream — making `MXR_DAEMON_ADDR="cmd://ssh -T host mxr daemon dial-stdio"` work for every client (CLI, TUI, bridge) uniformly. This is the entire "community transport plugin" system: an executable that speaks bytes.
- Remote caveats from discovery §9 apply and should be documented where `dial-stdio` is documented: compose `$EDITOR` flow, attachment paths, and autostart assume same-machine; over SSH those degrade — acceptable for scripting/agent use, documented as such. Build-id mismatch handling must not try to restart a remote daemon (`daemon_requires_restart` callers gate on locality capability).

## 5d. Q5: daemon-hosted bridge goes in-process (optional, recommended)

With `MemoryTransport` real (phase 4), `spawn_bridge_loop` (`bridge.rs:40`) hands the bridge an in-process connector instead of the socket path — deleting a socket round-trip per web request. Pure win, no behavior change; do it here while the plumbing is warm.

> **Deferred (2026-07-19).** `mxr-web` threads `state.config.socket_path` (`&Path`) into ~50 call sites that all funnel into just **two** connection-open points (`ipc_request_with_id` and `bridge_events` in `crates/web/src/lib.rs`). Switching the daemon-hosted bridge to an in-process `MemoryConnector` cleanly means replacing that `PathBuf` with an `Arc<dyn Connector>` and rethreading those call sites — a sizable, self-contained `mxr-web` refactor whose only payoff is latency (Q5 is explicitly "optional, recommended, no behavior change"). It was carved out of phase 5 to keep the web crate stable within this change's blast radius. The landing recipe: give `WebServerConfig` an `Arc<dyn Connector>`, point the two connect sites at `IpcConnection::connect_with`, and have `spawn_bridge_loop` build a `MemoryTransport` whose accept loop serves `serve_client_connection` with `PeerInfo::local()`. Standalone `mxr web` keeps a `UnixConnector`.

## Conformance & the auth matrix

- Corpus (scenarios 1–13) runs over: UDS, memory, TCP+token, stdio. Scenario 14 becomes real:
  - TCP: pre-auth request → `Auth` error; bad token → `Auth` error; good token → full corpus passes post-auth.
  - UDS/stdio/memory: no auth demanded (pinned explicitly, so a future accidental token-gate on UDS fails loudly).

> **Implemented.** Scenarios 1–13 now run over a **fifth** harness — the real `TcpServerTransport`/`TcpConnector` (`TcpTokenHarness`), which the framed-client wrappers `Authenticate` up front — so the whole carrier-independent corpus passes post-auth (65 matrixed tests). Scenario 14 is pulled out of the generic macro (its assertions differ by transport) and split into: `mod no_auth_pins` (the Ping-without-auth pin, one test each for socketpair/duplex/real-UDS/real-memory — the `LocalProcess`/`UnixPeer` set that also stands in for the stdio server's `LocalProcess` peer) and `mod auth_matrix` (three bespoke TCP tests: pre-auth request → `Auth`, bad token → `Auth` + still-gated, good token → `Authenticated` then a request dispatches). A dedicated stdio *harness* was judged not feasible/needed: `mxr daemon --stdio` feeds `serve_client_connection` a `LocalProcess` peer over joined stdin/stdout, which is byte-for-byte what the duplex/socketpair harnesses already exercise; the real stdio server is covered by a live smoke instead.

## Non-goals

- No TLS, no non-loopback binds, no in-daemon SSH (Q2: permanently out-of-process).
- No Windows named pipes / vsock / sd_listen_fds (phase 6 backlog).
- No changes to the HTTP gateway's REST surface.

## Verification

- Corpus × 4 transports green; `scripts/cargo-test` per touched crate; full suite before PR.
- Live TCP: enable `[transports.tcp]`, `MXR_DAEMON_ADDR=tcp://127.0.0.1:<port> MXR_DAEMON_TOKEN=… mxr status --format json`; then without token → clean auth error; `mxr events` over TCP (event stream over the authenticated connection).
- Live stdio: `mxr daemon --stdio` driven by a scripted client (or `IpcConnection::from_stream` harness) — request/response + event.
- Live dial-stdio: `ssh -T localhost mxr daemon dial-stdio` round-trip via `cmd://` connector; `mxr activity` attribution intact.

## Risks

- **Auth gate placed wrong** could lock UDS clients out or leave TCP open. Mitigation: the pinned no-auth-on-UDS scenario and the TCP pre-auth-rejection scenario are both in the corpus before the gate merges.
- Token handling sprawl (env, file, config). Mitigation: single resolution helper shared with the bridge; document one precedence order.
- stdio logging pollution. Mitigation: explicit test that a `--stdio` session's stdout contains only frames.

## Exit criteria

Three in-tree server transports + memory; `cmd://`/`dial-stdio` path demonstrated over SSH-to-localhost; auth matrix in the corpus; per-transport policy table from discovery §7 implemented and documented.
