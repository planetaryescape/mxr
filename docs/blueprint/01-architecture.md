# mxr — System Architecture

## High-level architecture

mxr is a daemon-backed system. The daemon is the brain. Everything else is a client.

```
┌──────────────┐     unix socket      ┌──────────────────────────────┐
│   TUI        │◄────────────────────►│         Daemon               │
│  (ratatui)   │                      │                              │
└──────────────┘                      │  ┌─────────┐ ┌───────────┐  │
                                      │  │  Sync   │ │  Rules    │  │
┌──────────────┐     unix socket      │  │ Engine  │ │  Engine   │  │
│   CLI        │◄────────────────────►│  └────┬────┘ └───────────┘  │
│  (mxr search │                      │       │                     │
│   mxr export │                      │  ┌────┴────┐ ┌───────────┐  │
│   etc.)      │                      │  │  Store  │ │  Search   │  │
└──────────────┘                      │  │ (SQLite)│ │ (Tantivy) │  │
                                      │  └─────────┘ └───────────┘  │
┌──────────────┐     unix socket      │                              │
│  Scripts /   │◄────────────────────►│  ┌──────────────────────┐   │
│  Shell hooks │                      │  │     Providers        │   │
└──────────────┘                      │  │  ┌───────┐ ┌──────┐  │   │
                                      │  │  │ Gmail │ │ SMTP │  │   │
                                      │  │  └───────┘ └──────┘  │   │
                                      │  └──────────────────────┘   │
                                      └──────────────────────────────┘
```

### Why daemon-backed?

We considered having the TUI be the entire application (like mutt or aerc). We rejected this because:

1. **Background sync**: Sync should happen whether or not the TUI is open. A daemon handles this naturally.
2. **Multiple clients**: TUI, CLI, scripts, and future frontends all talk to the same engine through the same protocol. No data races, no duplicated logic.
3. **Headless operation**: You can run mxr on a server (no display) for sync + rules + automation, then connect from any terminal.
4. **Clean separation of concerns**: The TUI only handles rendering and input. Business logic lives in the daemon. This makes both easier to test and maintain.
5. **Platform potential**: This is what makes mxr a platform rather than a single binary with everything tangled together.

### Daemon lifecycle

- `mxr` (no subcommand) starts the TUI. If the daemon isn't running, the TUI starts it automatically as a background process.
- `mxr daemon` starts the daemon explicitly (for systemd/launchd integration, headless servers, etc.).
- `mxr sync`, `mxr search`, `mxr export`, etc. are CLI commands that connect to the running daemon.
- `mxr doctor` runs diagnostics (config validation, connection tests, daemon health).
- If the daemon isn't running when a CLI command executes, it either starts it temporarily or errors with a helpful message.

### IPC protocol

Communication between clients and the daemon is **JSON over a Unix domain socket**. We chose this over:

- **gRPC**: Too heavy for a local tool. Adds protobuf compilation, code generation, and a runtime dependency.
- **HTTP/REST**: Unnecessary overhead for local IPC. No need for HTTP semantics.
- **Raw binary protocol**: Harder to debug, harder for community tooling to interact with.
- **Named pipes**: One-directional, not suitable for request-response.

JSON over Unix socket is simple, debuggable (`socat` can talk to it), and fast enough for local communication. The implemented protocol is `IpcMessage { id, payload }`, where `payload` is `Request`, `Response`, or `DaemonEvent`.

### IPC contract buckets

The protocol should be read in four buckets:

1. `core-mail`: stable mail/runtime capabilities
2. `mxr-platform`: accounts, rules, saved searches, subscriptions, semantic runtime
3. `admin-maintenance`: status, events, logs, doctor, bug reports, operational controls
4. `client-specific`: not part of daemon IPC; belongs in TUI/web/CLI shaping layers

The daemon serves reusable truth and workflows, not screen payloads.

Socket location: `$XDG_RUNTIME_DIR/mxr/mxr.sock` (Linux) or `~/Library/Application Support/mxr/mxr.sock` (macOS).

### Transport seam

The wire protocol is frozen; the byte stream underneath it is pluggable. The `mxr-transport` crate holds the seam — object-safe traits over a boxed byte stream, in the same spirit as the mail-provider adapter system:

```
clients (CLI, TUI, MCP, web gateway, scripts)
        │  Connector::connect() -> BoxedIo          (mxr-transport)
        ▼
FROZEN protocol: IpcMessage / Request / ResponseData / DaemonEvent + IpcCodec
        ▲
        │  ServerTransport::bind() -> TransportListener::accept() -> (BoxedIo, PeerInfo)
        ▼
adapters: UDS (default) · in-memory duplex (tests) · TCP-loopback+token · stdio
          [community transports out-of-process via `mxr daemon dial-stdio` / `cmd://`]
```

- `ServerTransport::bind` owns the socket lifecycle (bind, `chmod 0600`, stale-socket cleanup, successor detection); the daemon keeps the pid file and the search-index singleton lock, which are daemon — not transport — lifecycle. Graceful shutdown is ordered: `stop_accepting` (refuse new clients so they get connection-refused, not a hang) → drain in-flight connections → `cleanup` (ownership-guarded unlink, last).
- `TransportListener::accept` returns the byte stream plus `PeerInfo`. `PeerAuth::UnixPeer` always carries *real* OS peer credentials (an accept that can't read them fails that connection closed rather than fabricating an identity); `LocalProcess` marks the in-memory/in-process/stdio case; the token-bearing transport (TCP) carries `TokenRequired`. Auth evidence is per-transport (the Tailscale lesson). `accept` is contractually **cancel-safe** — the accept loop polls listeners with `select_all` and drops the losers each round; an adapter must not lose a connection when its `accept` future is dropped. `PeerInfo` is threaded into the dispatch context alongside `ClientKind`; the serve core's connection-scoped `ConnectionAuth` gate (`crates/daemon/src/serve.rs`) reads it — a `TokenRequired` peer gets an `Auth` error (and no events) on every request until a successful `Authenticate`, while UDS/memory/stdio start trusted.
- The serve core (lanes, task-per-connection, event fan-out, panic guard) is shared and generic over the stream; adapters only produce connections.
- `mxr-client` is generic over a `Connector`; the daemon builds transports through `build_transports` (`crates/daemon/src/server.rs`) — always UDS, plus TCP-loopback+token when `[transports.tcp]` is configured — and its accept loop iterates the `Vec` of listeners with per-round rotation for fairness. `mxr daemon --stdio` serves exactly one connection over stdin/stdout instead.
- The HTTP bridge (`mxr-web`) is a REST/WS presentation gateway that *consumes* the client transport — it is not a `ServerTransport` implementation (decision D052).
- **Address resolution has one source.** The daemon's bind, autostart, the liveness/stale probe, doctor's reachability, and the request path all resolve through `TransportAddr::resolve` (`MXR_DAEMON_ADDR` = `unix://<path>` / `tcp://<host:port>` / `cmd://<command>` for the CLI, precedence over `MXR_SOCKET_PATH` / the per-instance default), so start / probe / request can never disagree. The standalone `mxr-tui` / `mxr-web` / `mxr-mcp` clients resolve through the shared `TransportAddr::resolve_unix_socket` — `unix://` only, with `tcp://`/`cmd://` rejected with a clear message rather than silently ignored. See `docs/blueprint/20-transports.md` for the full transport reference.

### Subcommand structure

```
mxr                     # Opens TUI (starts daemon if needed)
mxr daemon              # Start daemon explicitly
mxr daemon --foreground # Start in foreground (for debugging / systemd)
mxr sync                # Trigger one-shot sync for all accounts
mxr sync --account NAME # Sync specific account
mxr search "query"      # CLI search, outputs results to stdout
mxr compose             # Open $EDITOR for new message
mxr export THREAD_ID    # Export thread (default: markdown)
mxr export THREAD_ID --format llm  # Export for LLM context
mxr doctor              # Run diagnostics
mxr accounts            # List configured accounts
mxr config              # Show resolved config
```

This means scripts and cron jobs can use `mxr search` and `mxr export` without the TUI. That's what makes mxr a platform, not just a GUI.

## Crate map (Cargo workspace)

```
mxr/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── core/                     # Types, internal model, traits, errors
│   │                             # No dependencies on other mxr crates.
│   │                             # This is the foundation everything builds on.
│   │
│   ├── store/                    # SQLite persistence via sqlx
│   │                             # Migrations, queries, connection management.
│   │                             # Depends on: core
│   │
│   ├── search/                   # Tantivy indexing and query engine
│   │                             # BM25, field boosts, saved searches, query parser.
│   │                             # Depends on: core
│   │
│   ├── provider-gmail/           # Gmail API adapter (MailSyncProvider + MailSendProvider)
│   │                             # OAuth2 flow, history.list delta sync, batch API.
│   │                             # Depends on: core, mail-parse, outbound
│   │
│   ├── provider-imap/            # IMAP adapter (MailSyncProvider only, first-party)
│   │                             # CONDSTORE/QRESYNC + UID fallback + IDLE.
│   │                             # Depends on: core, mail-parse
│   │
│   ├── provider-outlook/         # Outlook adapter (MailSyncProvider + MailSendProvider)
│   │                             # Microsoft Graph sync plus outbound rendering.
│   │                             # Depends on: core, outbound
│   │
│   ├── provider-smtp/            # SMTP send adapter (MailSendProvider only)
│   │                             # Via lettre. Depends on: core, outbound
│   │
│   ├── provider-fake/            # In-memory test double (both traits)
│   │                             # Deterministic, no network. For tests and adapter authors.
│   │                             # Depends on: core
│   │
│   ├── mail-parse/               # Shared RFC 5322/mail parsing helpers
│   │                             # Depends on: core
│   │
│   ├── outbound/                 # Shared outbound message rendering/building
│   │                             # Markdown render, attachments, RFC 5322 assembly.
│   │                             # Depends on: core
│   │
│   ├── llm/                      # LLM provider clients and prompt/result DTOs
│   │                             # Mail-model-free; callers pass plain data.
│   │                             # Depends on: no mxr crates
│   │
│   ├── sync/                     # Sync engine: orchestrates providers ↔ store ↔ search
│   │                             # Delta tracking, conflict resolution, snooze wake loop.
│   │                             # Depends on: core, store, search
│   │
│   ├── compose/                  # $EDITOR workflow, frontmatter parsing, draft UX
│   │                             # Draft management, context block generation.
│   │                             # Depends on: core, outbound
│   │
│   ├── reader/                   # Reader mode: HTML→text, signature stripping,
│   │                             # quote collapsing, boilerplate removal.
│   │                             # Depends on: no mxr crates
│   │
│   ├── rules/                    # Deterministic rules engine
│   │                             # Condition evaluation, action dispatch, dry-run, replay.
│   │                             # Depends on: core
│   │
│   ├── export/                   # Thread export in multiple formats
│   │                             # Markdown, JSON, mbox, LLM context.
│   │                             # Depends on: core, reader
│   │
│   ├── relationship/             # Relationship analytics and recipient intelligence
│   │                             # Uses local mail state, reader text, and optional LLM DTOs.
│   │                             # Depends on: core, store, reader, llm
│   │
│   ├── safety/                   # Local reply/composition safety checks
│   │                             # Pure checks over caller-provided message/reply context.
│   │                             # Depends on: core, reader, relationship
│   │
│   ├── protocol/                 # IPC types: Request, Response, Command enums
│   │                             # Shared between daemon and all clients.
│   │                             # Depends on: core
│   │
│   ├── transport/                # Transport seam: ServerTransport / TransportListener
│   │                             # / Connector traits over a boxed byte stream,
│   │                             # PeerInfo auth evidence, TransportCapabilities,
│   │                             # unix:// addressing; UDS + in-memory adapters.
│   │                             # Depends on: no mxr crate (pure byte-stream)
│   │
│   ├── client/                   # Shared daemon IPC connection: connect + frame +
│   │                             # correlate + read events, kinded ClientError.
│   │                             # Generic over a transport Connector.
│   │                             # Depends on: protocol, transport
│   │
│   ├── daemon/                   # Background process: socket server, sync loop,
│   │                             # snooze waker, rules executor, search indexer.
│   │                             # Depends on: core, store, search, sync, compose,
│   │                             #             rules, export, protocol, client,
│   │                             #             transport, mail-parse, providers
│   │
│   ├── mcp/                      # First-party MCP server; tools call the daemon
│   │                             # over IPC (source=mcp) through client.
│   │                             # Depends on: core, protocol, client, config
│   │
│   └── tui/                      # Ratatui frontend: panes, vim navigation,
│   │                             # command palette, keybinding dispatch.
│   │                             # Depends on: core, protocol, client, config,
│   │                             #             compose, reader, mail-parse
│   │
│   └── web/                      # HTTP/WebSocket bridge client over daemon IPC.
│                                 # Depends on: core, protocol, client, config,
│                                 #             compose, mail-parse
│
├── migrations/                   # SQLite migrations (used by store crate)
├── config/
│   └── default.toml              # Default configuration
├── tests/                        # Integration tests
├── docs/
│   └── blueprint/                # This document set
├── .github/
│   └── workflows/                # CI: build, test, lint, fmt
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
└── CONTRIBUTING.md
```

### Crate dependency rules

These are strict. Violations should be caught in code review:

1. **`core` depends on nothing internal.** It is the leaf node. All other crates depend on it.
2. **`protocol` depends only on `core`.** It defines the IPC contract between daemon and clients.
3. **`transport` depends on no internal mxr crate.** It is a pure byte-stream seam (`ServerTransport` / `TransportListener` / `Connector` traits, `PeerInfo` auth evidence, `TransportCapabilities`, `unix://` addressing, UDS + in-memory (`test-util`) adapters), depending only on `tokio` / `async-trait` / `thiserror` / `tracing`. A `protocol` dependency may arrive in phase 5 with the additive `Authenticate` request. `client` and `daemon` may depend on `transport`; `transport` must not depend on daemon, store, search, sync, semantic, or provider crates. The wire protocol stays frozen — transports abstract only where bytes come from.
4. **`client` depends only on `protocol` and `transport` at runtime.** It is the one shared IPC connection (connect + frame + correlate + read events, kinded `ClientError`), generic over a transport `Connector`. Test fixtures may dev-depend on `core` (as provider crates dev-depend on `provider-fake`), but no other runtime dependency is allowed. The daemon (for its CLI/internal client), `tui`, `web`, and `mcp` may depend on `client`; `client` must not depend on daemon, store, search, sync, semantic, or provider crates.
4. **Provider crates depend on `core` plus shared mail utility crates only.** Today that means `mail-parse` and `outbound`. Gmail, IMAP, SMTP, Outlook, and fake adapters do NOT depend on store, search, sync, daemon, TUI, or web.
4. **`store` depends only on `core`, and `search` depends only on `core`.** They are storage backends, not business logic.
5. **`semantic` owns embeddings and dense retrieval.** It may depend on `core`, `config`, `reader`, and `store`. It must not depend on daemon, TUI, or provider crates.
6. **`llm` owns LLM provider clients and prompt/result DTOs.** It deliberately depends on no internal crates; higher layers pass plain data into it.
7. **`relationship` owns relationship analytics and recipient intelligence.** It may depend on `core`, `store`, `reader`, and `llm`. It must not depend on protocol, daemon, clients, sync, search, semantic, or provider crates.
8. **`safety` owns local reply/composition safety checks.** It may depend on `core`, `reader`, and `relationship`. It must not depend on store, protocol, daemon, clients, sync, search, semantic, or provider crates.
9. **`sync` depends on `core`, `store`, `search`.** It orchestrates data flow between providers and local state.
10. **`daemon` is the integration point.** It depends on most crates. This is expected and acceptable — it's the application entry point.
11. **`tui` and `web` are clients.** They may depend on `core`, `protocol`, and client-local utility crates such as `config`, `compose`, `reader`, and `mail-parse`, but they must not depend on daemon, store, search, sync, semantic, or provider crates.
12. **Architectural seams are Cargo seams.** Do not fake crate boundaries with `#[path]` source inclusion; use real workspace crates and normal path dependencies.

### Package surface

- The repo-root package `mxr` is the install/product surface.
- Internal crates under `crates/` are workspace implementation details and default to `publish = false`.
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.

### Key dependencies (external crates)

| Concern | Crate | Why this one |
|---|---|---|
| Async runtime | `tokio` | Standard for Rust async. Full-featured, well-maintained. |
| SQLite | `sqlx` | Async, compile-time checked queries, not an ORM. Fits "correctness beats cleverness." We considered sea-orm (too much magic) and rusqlite (not async). |
| Full-text search | `tantivy` | Rust-native search engine, Lucene-inspired. BM25 natively, faceted search, scales to millions of docs. We considered SQLite FTS5 but it's too basic for our search ambitions. |
| TUI framework | `ratatui` + `crossterm` | The standard for Rust TUI apps. More control than Bubbletea (Go) for complex multi-pane UIs. |
| Email parsing | `mail-parser` (Stalwart) | Best Rust email parser. Handles MIME, attachments, encoding edge cases. |
| SMTP | `lettre` | Mature, async-capable Rust mail sending library. |
| HTTP client | `reqwest` | For Gmail API calls. We chose raw reqwest over the generated `google-gmail1` crate because the generated crate is bloated and awkward. Raw HTTP + serde gives full control. |
| OAuth2 | `oauth2` crate | Standard OAuth2 flow implementation for Rust. |
| Markdown → HTML | `comrak` | Rust CommonMark/GFM parser and renderer. |
| Fuzzy matching | `nucleo` | Extracted from the Helix editor. Faster than skim/fzf matching. Powers the command palette. |
| Credential storage | `keyring` | Cross-platform system keyring access. Tokens and passwords never stored in config files. |
| Config | `toml` + `serde` | Standard for Rust CLI tools. |
| Unique IDs | `uuid` v7 | UUIDv7 is time-sortable, so primary keys are naturally ordered by creation time. |
| Bitflags | `bitflags` | For MessageFlags (read, starred, draft, etc.). Efficient storage as integer. |
| Error handling | `thiserror` + `anyhow` | `thiserror` for typed errors in libraries, `anyhow` for application-level error propagation. |
| Logging/tracing | `tracing` + `tracing-subscriber` | Structured logging with span support. Better than `log` for async daemon debugging. |
| HTML to text | `html2text` | Built-in fallback for HTML→plain text conversion. Users can override with external tool (w3m, lynx). |
