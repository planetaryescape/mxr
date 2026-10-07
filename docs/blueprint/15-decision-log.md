# mxr — Decision Log

Every significant design decision, what alternatives were considered, and why we chose what we chose. This exists so that a coding agent (or future contributor) doesn't re-debate settled decisions.

---

## D001: Language — Rust over Go

**Chosen**: Rust

**Considered**: Rust, Go, TypeScript, Python

**Why Rust**:
- Tantivy (full-text search engine) is a Rust library with no equivalent quality in other languages. Go has Bleve (decent but slower, less featureful).
- Ratatui is more capable than Go's Bubbletea for complex multi-pane TUIs.
- Ecosystem fit: mail-parser, lettre, sqlx, comrak, nucleo are all excellent Rust crates for this specific project.
- No GC: predictable memory for a long-running daemon.
- Single small binary.

**Why not Go**: Go would give faster iteration (faster compiles, simpler concurrency, lower learning curve for contributors). If search wasn't a core differentiator, Go would have been the recommendation. The compile time and borrow checker friction are real costs for a side project.

**Why not TypeScript**: TUI options weaker. Ink exists but less polished. Runtime dependency (Node).

**Why not Python**: Performance for large mailboxes. Textual is decent but can't match ratatui.

---

## D002: Architecture — Daemon-backed, not monolithic

**Chosen**: Separate daemon process with TUI/CLI as clients over Unix socket.

**Considered**: Monolithic binary (TUI IS the app), daemon-backed.

**Why daemon**:
- Background sync works whether or not TUI is open
- Multiple clients (TUI, CLI, scripts, web UI) share one engine
- Headless operation on servers
- Clean separation of concerns (TUI handles rendering, daemon handles logic)
- Makes it a platform, not just a GUI

**Why not monolithic**: mutt and aerc are monolithic. It works but sync stops when you close the app, scripts can't access data, and you can't run headless.

**IPC choice**: JSON over Unix socket. Not gRPC (too heavy for local IPC, adds protobuf dependency), not HTTP (unnecessary overhead), not raw binary (harder to debug).

---

## D003: Database — sqlx over ORMs

**Chosen**: sqlx with compile-time checked queries

**Considered**: sqlx, rusqlite, sea-orm, diesel

**Why sqlx**:
- Async (works with tokio)
- Compile-time checked queries (catches schema drift at build time, fits "correctness beats cleverness")
- Not an ORM — you write SQL directly, no magic
- Good migration support

**Why not rusqlite**: Not async. Would need to spawn blocking tasks for every DB call.

**Why not sea-orm**: Too much ORM magic. We want explicit SQL we can read and understand.

**Why not diesel**: Heavy macro usage, code generation complexity. Better for larger teams, overkill for this project.

**SQLite specifically because**: Local-first. No external database server. Single file. Well-understood. Fastest possible local persistence.

---

## D004: Search — Tantivy over SQLite FTS5

**Chosen**: Tantivy as primary search engine. FTS5 kept as lightweight fallback.

**Considered**: Tantivy only, FTS5 only, Tantivy + FTS5 fallback

**Why Tantivy**:
- Rust-native, Lucene-inspired, purpose-built search engine
- BM25 ranking with field boosts
- Faceted search, boolean queries, phrase matching
- Scales to 100k+ documents without performance issues
- "Blazing fast search" is a first-class product feature, not an afterthought

**Why not FTS5 alone**: Basic BM25, no field boosts, limited query syntax, slow on large corpora. It's a database feature, not a search engine.

**Why keep FTS5**: Near-zero maintenance cost (triggers keep it synced). Useful as fallback for simple queries when Tantivy isn't available. Safety net for reindexing.

**Key design rule**: SQLite is the source of truth. Tantivy stores denormalized search documents. Search results resolve back to SQLite IDs. Tantivy index is always rebuildable from SQLite.

---

## D005: Vector search — NOT in v0.1

**Chosen**: Defer vector/hybrid search to Phase 2.

**Why defer**:
- Embeddings pipeline, vector persistence, incremental updates, model packaging, local inference performance, and ranking fusion tuning all create a second system before the first is proven
- BM25 via Tantivy is already better search than any terminal email client ships today
- Adds significant complexity and binary size (ML model)

**When we do build it**: candle for local embeddings (all-MiniLM-L6-v2), usearch or hnsw_rs for ANN index, RRF (Reciprocal Rank Fusion) for combining BM25 and vector results.

---

## D006: Gmail API — Direct API over gws CLI

**Chosen**: Direct Gmail REST API via reqwest + oauth2 crate

**Considered**: Google Workspace CLI (gws), direct API, generated google-gmail1 crate

**Why direct API**:
- Full control over OAuth2 flow
- history.list for efficient delta sync (Gmail's killer feature)
- Batch requests (up to 100 API calls in one HTTP request)
- Structured JSON responses deserialized into our types
- No external binary dependency
- Proper typed error handling

**Why not gws CLI**: It's a CLI wrapper — shelling out + parsing stdout is fragile. Pre-1.0 with expected breaking changes. External dependency breaks single-binary story. Can't do efficient delta sync. Error handling becomes string matching.

**Why not google-gmail1 crate**: Bloated, awkward API. Auto-generated code that's hard to debug. Raw reqwest is cleaner for the endpoints we need.

---

## D007: Provider traits — Split MailSyncProvider / MailSendProvider

**Chosen**: Two separate traits (sync and send)

**Considered**: Single EmailProvider trait, split traits

**Why split**:
- SMTP can only send. A single trait would force it to implement sync methods it can't support.
- Gmail can do both sync and send.
- IMAP does sync while SMTP handles send.
- The type system should reflect reality, not pretend every provider does everything.

**Account model consequence**: An account has separate sync_backend and send_backend fields. A user might sync via Gmail but send via their company's SMTP relay. This is a real-world configuration.

---

## D008: Label model — Unified organizer surface, explicit folder seam

**Chosen**: Labels as the unified organizer surface in the app model, with explicit honesty seams for folder-backed providers (`LabelKind::Folder`, provider IDs, `SyncCapabilities.mutate.labels == false`).

**Considered**: Labels only (flatten everything), labels + separate mailbox_membership + flags, labels + ProviderMeta blob

**Why this approach**:
- App logic sees labels: clean, unified, simple.
- We initially tried stuffing everything into Envelope, but that polluted the canonical model with provider-specific concerns.
- IMAP folder membership has different semantics than Gmail labeling (COPY+DELETE vs label add/remove). Flattening too aggressively causes subtle bugs.
- Provider truth stays visible through provider-scoped IDs, sync cursors, capability flags, and folder-vs-label distinction.

**ProviderMeta note**: The type/schema remain as a reserved escape hatch, but current sync/store flows do not materially depend on it at runtime.

---

## D009: Compose — $EDITOR with YAML frontmatter

**Chosen**: Open $EDITOR with a markdown file. YAML frontmatter for metadata (to, cc, subject). Markdown body converted to multipart on send.

**Why $EDITOR**: Users already know how to write in their editor. Don't compete with neovim/helix/vim. This is one of the strongest product bets.

**Why YAML frontmatter**: Hugo/Obsidian/Jekyll pattern — widely known. Human-readable. Easy to parse (serde_yaml). Separates routing metadata from body cleanly.

**Context block for replies**: The original thread is included as a commented-out block below the compose area. This solves "I need to reference the original while writing" without building a split-pane multiplexer.

**Why not split pane / tmux-style**: Target users already run tmux or a tiling WM. Building a terminal multiplexer inside an email client is massive scope for marginal benefit. Violates "mxr is an email client, not a terminal multiplexer." The context block solves 80% of the reference need with minimal code.

---

## D010: HTML rendering — Distraction-free, plain text first

**Chosen**: Strip HTML to plain text. Reader mode strips further. No images. Browser escape hatch.

**Considered**: Terminal HTML rendering (sixel/kitty images), embedded terminal browser, plain text only, configurable external renderer

**Why plain text first**: "Distraction-free email is a feature, not a limitation." Newsletters hijack attention with flashy banners, tracking pixels, animated GIFs. Stripping to plain text shows just the words.

**Why no terminal images**: Inconsistent terminal support (sixel, kitty protocol). Gimmicky. Contradicts distraction-free philosophy. Terminal email clients have survived 30 years without inline images.

**Browser escape hatch**: `o` keybinding opens original HTML in system browser via xdg-open. Covers the 5% of emails that need rich rendering.

**Configurable external renderer**: Power users can set `html_command = "w3m -T text/html -dump"` for better table handling. Built-in default uses html2text crate.

---

## D011: Reader mode — Strip to human content

**Chosen**: Active stripping of signatures, quoted replies, legal boilerplate, tracking junk.

**Why**: The rendering pipeline already converts HTML to text. Reader mode is one more pass on top. Same pipeline serves search indexing (cleaner text), thread export (tighter LLM context), and future rules matching.

**Implementation**: Regex patterns and heuristics, NOT ML. `-- \n` for signatures (RFC 3676), `>` prefixes and "On ... wrote:" for quotes, keyword matching for boilerplate.

**Display**: Stats shown in status bar ("reader mode: 342 → 41 lines") to reinforce value on every message.

---

## D012: Unsubscribe — One-key via RFC 2369

**Chosen**: Parse List-Unsubscribe header at sync time. `U` keybinding with confirmation.

**Why this works**: RFC 2369 List-Unsubscribe is a standard header that most legitimate newsletters include. RFC 8058 adds one-click HTTP POST. Most Substack/Mailchimp/ConvertKit newsletters support this. The user never leaves the terminal.

**Fallback**: If header absent, scan HTML body for unsubscribe links (lower confidence).

**Storage**: UnsubscribeMethod enum stored as JSON on the messages table. Parsed once at sync time, instant when user hits `U`.

---

## D013: Snooze — Local-first, Gmail archive integration

**Chosen**: Local snooze with Gmail archive on snooze, inbox restore on wake.

**Why local**: Gmail API has no snooze endpoint. Snooze in Gmail web is internal only. Local implementation is actually better — full control, works offline, extensible (conditional snooze later via rules).

**Gmail integration**: On snooze, message is archived on Gmail (INBOX label removed). On wake, INBOX label re-applied. State is consistent across mxr and Gmail web.

**Implementation**: `snoozed` SQLite table. Daemon runs a wake loop checking every 60 seconds.

---

## D014: Rules engine — Deterministic data first, scripts later

**Chosen**: Rules as serializable data (Conditions + Actions), not scripts.

**Why data first**: Rules must be inspectable, replayable, idempotent, and dry-runnable. "Show me what this rule would do" must work before "run this rule." Users need trust before they rely on automation. Scripts are escape hatches (shell hooks), not the foundation.

**Phasing**: v0.2 declarative rules, v0.3 shell hooks, future scripting runtime (Lua/Rhai).

---

## D015: Adapter strategy — historical note, later overridden

**Chosen at the time**: First-party Gmail sync + SMTP send. Community adapters for everything else.

**Why not build IMAP**: IMAP is not the maintainer's use case. Building for checkbox coverage instead of actual usage leads to poor quality. The architecture is clean enough that IMAP is a great community adapter candidate.

**Why not IMAP first**: We considered this because IMAP is the open standard. Rejected because Gmail is the actual use case, Gmail API is significantly better for sync (delta via history.list), and IMAP requires more complex state management (UIDVALIDITY, connection pooling, IDLE).

**Adapter kit**: The project provides traits, a fake provider, conformance tests, fixture data, and a "how to build an adapter" doc. Otherwise "extensible" is just words.

---

## D016: Name — mxr

**Chosen**: mxr (pronounced "mixer" or as letters)

**Why**: Short, distinctive, terminal-friendly, easy to type, available on crates.io, no CLI binary conflicts, no significant GitHub repo conflicts. Subtle connection to MX records. "Mixer" works as metaphor (multiple backends, mail + automation).

**Rejected names**:
- `mailx`: Already a standard Unix command. Would conflict in $PATH.
- `helo`: Taken on crates.io (v0.0.0 name squatter).
- `vox`: Name overloaded in GitHub (VoxelSpace, VoxCPM).
- `kite`: Taken on crates.io (search engine library).
- `letterbox`: SEO muddied by Letterboxd movie site.

---

## D017: IPC protocol — JSON over Unix socket

**Chosen**: JSON request/response over Unix domain socket.

**Considered**: gRPC, HTTP REST, raw binary, named pipes

**Why JSON over Unix socket**: Simple, debuggable (socat can talk to it), fast enough for local IPC, no external dependencies, easy for community tools to interact with.

**Implemented shape**: `IpcMessage { id, payload }`, where `payload` is `Request`, `Response`, or `DaemonEvent`.

**Boundary rule**: The daemon serves reusable truth/workflows. Client-specific shaping stays in clients. The protocol now tracks four conceptual buckets: `core-mail`, `mxr-platform`, `admin-maintenance`, `client-specific` (the last one should stay out of daemon IPC).

**Why not gRPC**: Too heavy for local tool. Adds protobuf compilation, code generation, runtime dependency.

**Why not HTTP**: Unnecessary overhead. No benefit of HTTP semantics for local process communication.

---

## D018: Encryption — Defer entirely, use standard protocols when ready

**Chosen**: No encryption in v1. When implemented, use PGP and/or age, not custom crypto.

**Considered**: SSH key-based custom encryption (mxr-to-mxr only), PGP integration, age integration, autocrypt.

**Why not custom SSH encryption**: Creates a proprietary protocol only mxr users can read. Zero utility at launch (network effect problem). Rolling your own crypto protocol is universally advised against.

**Future plan**: PGP integration with good UX (make existing standards not suck), potentially age support for users who hate PGP. mxr's angle on encryption is UX, not protocol invention.

---

## D019: Vim motions — Wire ourselves, no drop-in crate

**Chosen**: Implement vim navigation keybindings manually in the event loop.

**Why**: No "vim navigation for ratatui" drop-in exists. But the surface area is small (~20 keybindings for navigation). It's a match statement, not a framework. The real design work is the configurable keymap layer and action dispatch system.

**Multi-key sequences** (like `gg`): Small state machine with 500ms timeout. ~50 lines of code.

**Key architectural insight**: Keybindings and command palette both dispatch through the same Action enum. Build the action dispatch system once, both input methods use it.

---

## D020: Embedded terminal multiplexer — Rejected

**Chosen**: Don't build split panes. Users have tmux/zellij/tiling WMs.

**Why not**: Target users already have terminal multiplexing. Building one inside mxr is massive scope, violates "mxr is an email client, not a terminal multiplexer." The context block in compose files solves the reference problem with minimal code.

---

## D021: Drizzle ORM — Not applicable

**Noted**: Drizzle was suggested early in planning. It's a TypeScript ORM. We're building in Rust. Not applicable. The Rust equivalent discussion led to choosing sqlx (see D003).

---

## D022: Cloud backend — Rejected

**Chosen**: Local daemon, not cloud.

**Why**: The entire pitch is "local-first." A cloud backend contradicts the core value prop, adds infrastructure cost, and creates a dependency. The daemon runs locally. No cloud required.

---

## D023: Google Workspace CLI as adapter foundation — Rejected

**Noted**: gws CLI (https://github.com/googleworkspace/cli) was the original idea for Gmail integration. Rejected in favor of direct Gmail API (see D006). The gws CLI should be treated as if it doesn't exist.

---

## D024: Progressive body indexing — SUPERSEDED by D049

~~**Chosen**: Index headers/snippets at sync time. Index body text when the body is fetched (on first read).~~

~~**Why**: Keeps initial sync fast (headers only). Search works immediately against subjects and snippets. Gets richer as the user reads messages. Bodies are the expensive part — fetching all of them upfront would make initial sync very slow for large mailboxes.~~

Superseded by D049. Bodies and body text are now indexed at sync time.

---

## D049: Eager body fetch replaces lazy hydration

**Chosen**: Fetch envelope + body together during sync. No on-demand body fetching.

**Considered**: Keep lazy hydration with better prefetch, eager fetch for recent + lazy for old.

**Why eager**:
- Lazy hydration caused visible "Loading..." in TUI when opening messages — violates "blazing fast" UX
- Network calls at read time mean offline access only works for previously-read messages
- Progressive search indexing meant body text wasn't searchable until opened
- The complexity of maintaining two fetch paths (sync + on-demand) wasn't justified

**Trade-offs accepted**:
- Initial sync downloads more data (~2-5x per message for Gmail Full vs Metadata format)
- Storage grows proportionally to mailbox size, not reading habits
- Sync is slightly slower per batch (offset by eliminating background prefetch)

**What changed**:
- `SyncBatch.upserted` is now `Vec<SyncedMessage>` (envelope + body paired)
- `fetch_body` removed from `MailSyncProvider` trait
- Gmail uses `MessageFormat::Full` instead of `Metadata`
- IMAP uses `BODY.PEEK[]` instead of `BODY.PEEK[HEADER]`
- Body prefetch loop removed from daemon
- `GetBody` handler reads from SQLite only, no provider call
- Search indexes body text immediately during sync

---

## D050: Hybrid search — English default, multilingual opt-in, lazy model delivery

**Chosen**:

- Tantivy BM25 stays the default lexical path
- local semantic retrieval is added as an optional second retrieval path
- `bge-small-en-v1.5` is the default local profile
- `multilingual-e5-small` is opt-in
- `bge-m3` is optional advanced install only
- model weights are lazy-downloaded into the mxr data dir
- dense ANN state is rebuildable from SQLite, not treated as canonical storage
- BM25 + dense retrieval fuse with Reciprocal Rank Fusion

**Considered**:

- always-default multilingual profile
- shipping model weights inside the binary
- `sqlite-vec` as the primary dense retrieval engine
- cloud-only embeddings

**Why this choice**:

- English default keeps first-use download and CPU cost smaller for the common case
- multilingual still matters, but opt-in avoids penalizing every user
- lazy model delivery preserves the single-binary install story without making the binary huge
- SQLite remains the canonical store for chunks, embeddings, and profile state
- a rebuildable sidecar ANN index matches the existing Tantivy pattern
- RRF gives robust hybrid ranking without forcing incompatible score spaces into fake normalization

**Why not always-default multilingual**:

- larger default footprint
- slower first enable for users who do not need it
- weaker product default for the majority-English use case

**Why not bundled model weights**:

- binary size balloons immediately
- every user pays for every profile whether they use it or not

**Why not cloud-first embeddings**:

- breaks the local-first story
- adds network latency and privacy concerns to core search

---

## D051: Workspace boundaries — real crates, one product surface

**Chosen**:

- keep the repo-root package `mxr` as the install/product surface
- make the logical seams under `crates/` real workspace crates
- default internal crates to `publish = false`
- enforce seams with normal Cargo dependencies, not `#[path]` source inclusion

**Why**:

- architectural seams only matter if Cargo enforces them
- one product package is simpler for users than publishing a constellation of crates
- private workspace crates avoid accidental coupling without creating crates.io noise
- daemon remains the integration root, which is correct for the application architecture

**Trade-offs accepted**:

- more `Cargo.toml` files in the repo
- one new shared utility crate (`mxr-outbound`) exists because outbound message building is a real seam shared by compose and send adapters
- clients still use some local utility crates (`config`, `compose`, `reader`, `mail-parse`) even though they remain runtime clients of the daemon

**What changed**:

- `crates/daemon/src/lib.rs` no longer source-includes pseudo-crates via `#[path]`
- internal seams are normal workspace crates with path dependencies
- shared mail parsing moved into `mxr-mail-parse`
- shared outbound message building moved into `mxr-outbound`
- `mxr-search` no longer owns store-backed saved-search service glue

---

## D052: HTTP bridge is a gateway, not a transport adapter (transport-adapters Q1)

**Chosen**: The HTTP/WebSocket bridge (`mxr-web`) *consumes* the client transport (`Connector`); it is NOT a `ServerTransport` implementation. Browser-native access stays REST+WS over the bridge.

**Considered**: Make the bridge a WS-binary byte-stream `ServerTransport` so browsers speak the raw IPC frame protocol; keep it a gateway.

**Why gateway**:
- Discovery measured the bridge: ~100 lines are transport plumbing, ~5,500 are presentation (per-route handlers, view-model assembly, SPA serving, OpenAPI, security posture). Forcing it to implement the same trait as UDS would misshape the trait around a presentation layer.
- The ecosystem premium is on the protocol, not transport pluggability (the Podman/varlink regret: v1 shipped a novel RPC layer, the ecosystem wouldn't rewrite Docker-API tooling, v2 deleted it for Docker-compatible REST). mxr freezes the wire protocol and abstracts only the byte stream.
- Typed-transport RPC (tarpc's `Transport` over Rust-typed messages) would exclude curl/jq/scripts/non-Rust agents — against mxr's CLI-first JSON shape.

**Trade-offs accepted**:
- A future non-REST browser client that wants raw frames would need a WS-binary adapter added then (revisit only if it appears).
- The bridge's security posture (bearer token, loopback enforcement) is not shared with the trait, but it is directly reusable by a future TCP adapter.

---

## D053: Transport traits live in a new `mxr-transport` leaf crate (transport-adapters Q4)

**Chosen**: A new `crates/transport` (`mxr-transport`) crate owns the transport seam — `ServerTransport` / `TransportListener` / `Connector` traits, `PeerInfo`, `TransportCapabilities`, `unix://` addressing, and the UDS + in-memory adapters. It is a pure byte-stream crate depending on **no** internal `mxr-*` crate (only `tokio` / `async-trait` / `thiserror` / `tracing`); a `mxr-protocol` dependency may arrive in phase 5 with the additive `Authenticate` request.

**Considered**: Put the traits inside `mxr-protocol`; a new leaf crate.

**Why a new crate**:
- Keeps `mxr-protocol` a pure wire contract (types + codec). Transport is "where bytes come from," a different concern; co-locating them would blur the frozen-protocol boundary.
- Transport carries no protocol types today (traits deal only in byte streams and peer/auth evidence), so it stays even leaner than protocol — a genuine leaf.
- Mirrors the provider adapter system's crate shape (a leaf crate of object-safe traits, capability flags, a fake/in-memory reference impl behind a feature) — the discovery's explicit template.
- `mxr-client` and `mxr` (daemon) depend on it; `tui`/`web`/`mcp` reach it only transitively through `mxr-client` and still cannot depend on the daemon.

**Trade-offs accepted**:
- One more workspace crate. Justified: the seam is real and shared by both a client (`Connector`) and the daemon (`ServerTransport`), exactly the case for a leaf crate.

**What changed**:
- `UdsServerTransport` absorbed the UDS socket lifecycle (bind, `chmod 0600`, stale-socket cleanup, successor detection) that was inline in `server.rs`; the pid file and index-lock singleton stayed daemon-level.
- `IpcConnection` became generic over a `Connector` (`connect_with`); the path constructor builds a `UnixConnector` internally.
- `PeerInfo` (UDS peer credentials) is threaded into the dispatch context; no policy reads it yet (phase 5's token gate does). `PeerAuth::UnixPeer` always carries real creds — a `peer_cred` failure fails that connection closed rather than fabricating an identity — so phase-5 policy can trust a `UnixPeer` match.
- The conformance corpus runs every scenario over four harnesses: the socketpair/duplex carriers plus the real UDS and in-memory transports through `bind`/`accept`/`connect`.
- **`MXR_DAEMON_ADDR` single-source resolution**: the daemon bind, autostart, the socket probe, doctor's reachability, and the request path all resolve through the same `TransportAddr::resolve` (precedence `MXR_DAEMON_ADDR` > `MXR_SOCKET_PATH` > per-instance default), so start / probe / request never disagree. The standalone `mxr-tui` / `mxr-web` / `mxr-mcp` clients stay on `mxr_config::socket_path()` this phase; their `MXR_DAEMON_ADDR` adoption lands in phase 5.

---

## D054: Phase 5 transports — TCP+token, stdio, `cmd://`; token gate in the serve core; no protocol-version bump

**Chosen**: Ship three transports with opposite trust models and one additive protocol request.

- **5a — TCP loopback + token** (`TcpServerTransport` / `TcpConnector`): binds loopback only and **refuses non-loopback outright** (Q2: no in-daemon remote — off-machine reach is `dial-stdio` over SSH). Its accept surfaces `PeerAuth::TokenRequired`.
- **5b — stdio server** (`mxr daemon --stdio`): serves exactly one connection over stdin/stdout, `PeerAuth::LocalProcess` (the spawner authenticates), stdout carries only frames. Cannot run alongside a socket daemon (same exclusive state).
- **5c — `cmd://` connector** (`CmdConnector`): spawns a command and wraps its stdio as the byte stream (kill-on-drop, stderr passthrough), so `MXR_DAEMON_ADDR="cmd://ssh -T host mxr daemon dial-stdio"` works for the CLI. Argv is whitespace-split — no shell quoting.
- **5d — in-process bridge**: **deferred**. The win is latency-only (Q5 is "optional, recommended, no behavior change"); it requires rethreading `mxr-web`'s ~50 `socket_path` call sites onto a `Connector`, a self-contained web-crate refactor carved out to bound this change's blast radius.

**Auth gate**: `Request::Authenticate { token }` → `ResponseData::Authenticated`. The gate is **connection-scoped state in the serve core** (not the transport, which stays protocol-free; not the stateless dispatcher, which has no connection notion). A `TokenRequired` peer gets `IpcErrorKind::Auth` on every request — and no events — until a successful `Authenticate`; the `Authenticated` ack is sent inline so it always precedes any buffered event. UDS/memory/stdio start trusted and are byte-for-byte unchanged (pinned by corpus no-auth tests, so an accidental token-gate on UDS fails loudly).

**Token store**: the IPC token is a **dedicated** secret, distinct from the HTTP bridge token — `MXR_DAEMON_TOKEN` (env) **>** `<config_dir>/daemon-token` (0600, atomic `O_EXCL` create, 0600 re-asserted on read), via `mxr_config::resolve_daemon_token`. Reusing the bridge token would be a privilege leak: the bridge's `/api/v1/auth/local-token` endpoint hands its token to any loopback caller. The gate's comparison is constant-time (`constant_time_eq`). The `TcpConnector` also refuses non-loopback targets so the token is never sent in plaintext to a remote host (the server refuses non-loopback binds; the client closes the other half).

**No `IPC_PROTOCOL_VERSION` bump** (stays 4): the change is additive-only; an old client never emits `Authenticate`, and the only transport that requires it (TCP) is new, so no existing UDS exchange changes shape. The build-id handshake (`daemon_requires_restart`) already forces a restart on any binary upgrade, so a bump would only add spurious restart churn.

**Client adoption**: the CLI builds its connector from `MXR_DAEMON_ADDR` (`unix://`/`tcp://`/`cmd://`); autostart and the stale-socket probe are skipped for the non-unix schemes (they manage their own reachability). TUI/web/MCP route socket resolution through the shared `TransportAddr::resolve_unix_socket` (re-exported from `mxr-client`) — `unix://` only, `tcp://`/`cmd://` rejected with a clear message (support can follow demand).

**Conformance**: scenarios 1–13 gain a fifth harness (real TCP+token, post-`Authenticate`); scenario 14 is a bespoke auth matrix (pre-auth reject / bad token reject / good token unlock) plus no-auth pins for the four implicit-trust transports.

---

## D055: Abstract the byte stream, not the RPC layer (transport-adapters)

**Chosen**: A transport adapter produces a connected `AsyncRead + AsyncWrite` byte stream plus peer/auth evidence — nothing more. The wire protocol (`IpcMessage` / `Request` / `ResponseData` / `DaemonEvent` + `IpcCodec` framing) is frozen above every adapter.

**Considered**: A typed `Transport` trait over Rust-typed messages (tarpc's model); abstracting the RPC layer itself.

**Why byte-stream-level**:
- The ecosystem premium is on the protocol, not transport pluggability. Podman v1 shipped a novel RPC layer (varlink); the ecosystem wouldn't rewrite Docker-API tooling and v2 deleted it for Docker-compatible REST over UDS. Freezing the message protocol and abstracting only the listener/dialer is the pattern successful projects (Docker, LSP, MCP, systemd) converge on.
- A typed-RPC transport (tarpc) excludes curl/jq/scripts/non-Rust agents — directly against mxr's CLI-first JSON shape. A byte stream carries the same JSON frames everywhere, so every adapter is scriptable.
- The serve core (lanes, task-per-connection, event fan-out, panic guard, `EventsLagged`) stays shared and generic over the stream; adapters only produce connections, so backpressure and the conformance corpus never fragment per adapter.

**Trade-offs accepted**: adapters cannot negotiate protocol shape per transport — intentional; the protocol is the invariant.

---

## D056: Auth evidence is part of the transport contract (`PeerInfo`) (transport-adapters)

**Chosen**: `TransportListener::accept` returns `(BoxedIo, PeerInfo)`; `PeerInfo` carries `PeerAuth` — `UnixPeer { uid, gid, pid }` | `LocalProcess` | `TokenRequired` (additive). The transport surfaces identity evidence; the serve core decides policy.

**Considered**: accept returns bytes only, and the daemon re-derives peer identity out-of-band.

**Why in the contract**:
- The Tailscale lesson (`safesocket`): identity evidence is per-transport (UDS peer creds, a pipe SID, a token) and must be surfaced by the abstraction, not just bytes. A transport that hides it forces the daemon to special-case each carrier.
- `UnixPeer` always means the OS reported real credentials for this connection — a `peer_cred` failure fails that connection closed rather than fabricating the variant, so phase-5's token gate can match `UnixPeer` and *know* the creds are genuine.

**Trade-offs accepted**: a new transport with a novel identity kind adds a `PeerAuth` variant. Additive by construction — existing variants are undisturbed.

---

## D057: Transport-contract conformance in `mxr-transport`; protocol conformance stays in the daemon (transport-adapters, phase 6)

**Chosen**: Split the reusable conformance suite. `mxr-transport` (feature `conformance`) exports `run_transport_conformance` / `run_token_auth_conformance` — the **transport contract** (bind/accept/`stop_accepting`/`cleanup`, cancel-safety, a bidirectional stream, `PeerInfo`↔capability coherence), protocol-free. The daemon keeps the **protocol** corpus (`crates/daemon/src/serve/ipc_conformance.rs`) — id correlation, out-of-order completion, lane back-pressure, event fan-out, framing edges, the `Authenticate` gate.

**Considered**: One suite. Per the phase-6 spec's original 6a option, export a minimal fake-provider-backed `AppState` + serve loop from `mxr-test-support` so out-of-tree adapters run the *protocol* corpus against their own transport.

**Why the split**:
- An out-of-tree transport crate must prove conformance "without reading daemon source" and "depending only on `mxr-transport`" (phase-6 exit criteria). Requiring a daemon serve core + `AppState` + a fake provider would pull essentially the whole daemon into an adapter's dev-dependencies — the opposite of a leaf-crate kit.
- Protocol behavior is transport-independent by construction, and the in-tree corpus already proves it by running every scenario over the real UDS / in-memory / TCP transports. An adapter author re-running it would test the daemon, not their adapter. What they actually need to prove is that their byte stream and its lifecycle behave — exactly the transport suite.
- **The transport suite covers the protocol's byte-envelope requirement**, so the split leaves nothing untested out-of-tree. `run_transport_conformance` round-trips a payload at the codec's 16 MiB max-frame size each way (a documented `MAX_PROTOCOL_FRAME_BYTES` const mirroring `crates/protocol/src/codec.rs`, so the leaf crate needs no `mxr-protocol` dependency). A transport capped below the cap would pass a small round-trip yet fail the daemon's near-limit-frame scenario in production — the large-frame check catches that at the transport layer. It also asserts tri-coherence between `connector.auth_token()`, `capabilities.auth.token`, and the accepted `PeerAuth` (a mismatch would make the IPC client send an unwanted `Authenticate` handshake), and cancel-safety across a parked-then-dropped `accept`.
- Mirrors the provider kit's `run_sync_conformance<P>` shape: generic functions in the reference-impl leaf crate, consumed via one dev-dependency and a `#[tokio::test]`. Proven end-to-end by an out-of-tree scratch crate that implements its own transport and passes the suite (including the 16 MiB envelope) depending only on `mxr-transport`.

**Trade-offs accepted**: two conformance entry points instead of one. Justified — they test genuinely different contracts (byte-stream lifecycle vs. protocol semantics) and have different, correct homes.

---

## D058: `SyncNow { background }`, live sync progress, and one analytics repair per backfill (issue #179)

**Chosen**: Sync becomes ack-and-run, with progress readable from the status row and from events, and one shared finalizer for every sync path.

- **`Request::SyncNow { account_id, background }`** (`#[serde(default)]`, so an older client's wire bytes still mean the old blocking call). `background: true` marks the account as syncing, spawns the pass, and acks. `false` is unchanged: the daemon answers when the pass is over, still detaching at `MANUAL_SYNC_TIMEOUT`. **No `IPC_PROTOCOL_VERSION` bump** — additive only, same reasoning as D054, and the build-id handshake already forces a restart on any binary upgrade.
- **`AccountSyncStatus.progress: Option<SyncProgressData>`** (`current`, `total`, `message`) — live, in daemon memory, never persisted. It changes several times per page, which is not worth a write per step, and it means nothing once the daemon that reported it is gone. Non-daemon status builders (`mxr doctor` reading the store directly) report `None`.
- **Engine progress hook**: `SyncEngine::sync_account_reporting(provider, sink)` with `mxr_sync::SyncProgress` (`PageFetched` / `PageStored` / `PageIndexed`). The sink is a plain `&(dyn Fn + Send + Sync)`, so `mxr-sync` keeps no dependency on the protocol or the daemon. Only `PageStored` adds to the run's count — the same page is reported three times.
- **One finalizer**: `loops::SyncPass` + `begin_sync_pass` / `run_sync_pass` / `finalize_sync_pass`. The sync loop, the manual handler, and the detached-sync reaper all use it, so all three write the same status, complete the same log row, emit the same events, and run the same post-sync fan-out. Before this the reaper skipped the fan-out entirely and the manual handler ran a partial one.
- **Analytics repair once per backfill**: `incremental_analytics_backfill` is skipped while `has_more` or the initial backfill cursor is still live, and the skip is recorded as a **debt on the account** (`SyncRun::analytics_repair_owed`) that the first pass to find the backfill over settles. Measured at 82-106s per call on a 50k database *regardless of page size* — it is a whole-table `WHERE column IS NULL` repair, not an incremental one. Running it per page put roughly ten minutes of writer contention behind a 26s seed, and a real Gmail account paging 500 messages at a time paid it every page. Deferring the *work* is safe because the steps filter the whole table, so the settling run repairs every row the earlier pages left — but deferring the *decision* is not something the post-sync fan-out can own: the fan-out only runs for a page that carried messages, and a provider ends a backfill with an empty final page (Gmail when the page after a `nextPageToken` turns out empty, IMAP when the last UID chunk is all deleted). Nothing else in the daemon runs the repair — there is no startup call — so a missed settling run would leave analytics wrong until a later sync happened to land messages. The decision therefore lives in `finalize_sync_pass`, which runs for every pass including empty ones, and the repair is spawned from there rather than from `post_sync_fanout`. Rules and delivery scanning stay per page — they scale with the page, not the mailbox, and users expect labels promptly.
- **Detach reaper aborts on no progress, not on elapsed time**: the grace period now measures silence. A 50k backfill legitimately outlives any grace worth giving a wedged sync, so elapsed time cannot tell the two apart; the progress counter can.

**Considered**: having `background: true` just wake the account's sync loop (`idle_notify`) and ack. Rejected — the wake is indistinguishable from "sync started", so a client that acks and polls could read the account as idle before the loop picked the work up, and the reaper's missing fan-out would have stayed unfixed.

**Trade-offs accepted**: `mxr sync` without `--wait` now reports only that a sync started; failures after that point show on the status row rather than in the command's exit code. That is what `--wait` is for, and it is the honest shape for an operation that runs for minutes.

**Two states, not one, for "the account is busy"**: `sync_in_progress` is now `has_more || a background sync is queued`. A background `SyncNow` is acked before its pass has taken the provider lock, and in that window another pass can finish and would otherwise write "idle" over a client's live request. The claim that keeps that flag true is an RAII guard, released on drop, so a panic in the spawned pass or the runtime dropping it at shutdown cannot leave an account permanently "already syncing".

---

## D059: On a natural-key match the STORED `MessageId` wins, and the whole page follows it (issue #179 follow-up)

**Chosen**: `upsert_envelope_tx` reports what it did to the row — a `StoredEnvelope { id, vacated_thread_id }` — and the sync path rewrites its in-memory copy to match. `messages` has a `UNIQUE (account_id, provider_id)` natural key, so an envelope whose id derivation changed between releases (the 0.4.52 `from_provider_id` → `from_scoped_provider_id` switch, and any future change) resolves to a row that already exists under a different id. That row keeps its id; `Store::apply_sync_upserts` takes the page by `&mut` and points the envelope, the body, and the body's attachments at the stored id before writing them, and `SyncEngine` reads `upserted_message_ids` back off the page after storing it — so the lexical index entry, reply pairs, `NewMessages`, and semantic ingest all name the same id as the rows.

**Considered**: re-keying the stored row to the incoming id. Rejected — `messages.id` is referenced by bodies, attachments, labels, keywords, reply pairs, snoozes, semantic chunks, and the search index; rewriting it means re-keying every dependent row inside the sync transaction, and any table missed leaves an orphan. Keeping the stored id touches nothing already on disk.

**Why it mattered**: dependents were written against the *incoming* id, which no `messages` row had, so the page failed on a foreign key — pinned by a test that asserted the failure. Reachable in practice as soon as a demo or fake profile's cursor is reset, and by design for anyone upgrading across an id-derivation change.

**The same read closes a second gap**: the upsert overwrites `messages.thread_id`, so once it has run nothing else knows which thread a re-threaded message left. The prior value comes back with the id, and the sync engine adds it to `threads_changed` — tombstoned when the move emptied the thread — so a client drops the metadata it cached for a thread that no longer holds the message. Previously only the thread the message *joined* was reported, and the vacated one was caught only when the JWZ rethread pass happened to touch it (which providers with native threading never run).

**Trade-offs accepted**: `Store::upsert_envelope`/`upsert_envelope_with_direction` now return the stored `MessageId`. Callers that write dependent rows afterwards must use it; callers writing a genuinely new message can ignore it. Two production callers do write dependents — the sync engine and `ingest_sent_message` (the Sent copy filed after a send) — and both follow the returned id. Every other call site in the tree is test setup. Audit them by temporarily marking `typed_id!`'s struct `#[must_use]` and running `cargo check --workspace` (no `--all-targets`, so `#[cfg(test)]` code stays out): the workspace lints turn the dropped value into an error at exactly the production sites that ignore it.

---

## D060: A pid is not a daemon — identity is proven before a signal, and re-proven when it goes out (issue #179 follow-up)

**Chosen**: The daemon writes a `daemon.identity` TOML record beside `daemon.pid` (`pid`, `started_at`, canonical `exe`, `instance`, `config_dir`, `data_dir`), and the two discovery paths carry opposite defaults.

- **Pid-file path: trusted unless disproven.** Only this profile's daemon writes this profile's pid file, so the profile is already established and the one thing left to rule out is pid recycling. The identity record answers that by start time. A record that names a different pid, or another profile, *contradicts* the file rather than saying nothing — the two are written and cleared together, so disagreement means one is stale, and it is a rejection. With no record at all the fallback is the weak "is argv[1] `daemon`", deliberately indifferent to flags and to which build it is. Anything unreadable counts as still ours.
- **`ps`-scan path: distrusted unless proven.** Nothing on the file system vouches for the pid, so the executable is matched by canonical path (not file name) and the candidate's environment must resolve the same profile as ours. A pid adopted this way gets a record stamped for it, so the next run has something exact to check.
- **The evidence travels with the pid and is re-run with the signal in hand.** Verification and signalling are seconds apart — the status probe between them can take five, and asking a wedged daemon for status is itself a reason for it to exit — so a pid handed to another process in that window would otherwise be signalled on a stale observation. Start time is re-checked always; for a scan-found pid, so are the executable and profile that identified it. The SIGKILL escalation re-checks too.

**Considered**: matching on the command line alone, which is what shipped before. Rejected by the bug: every mxr checkout builds a binary called `mxr` and `mxr demo` pins one instance name across every profile, so two checkouts (or two demo profiles) restarted and killed each other's daemons.

**Why the record sits beside the pid file rather than inside it**: a binary older than this change still reads `daemon.pid` as the plain number it has always been. A missing record is normal and simply drops the caller back to the weaker check.

**Trade-offs accepted**:
- Reading another process's environment is platform-specific and lossy. Linux uses `/proc/<pid>/environ`; macOS has no procfs and the kernel interface needs `unsafe`, which this workspace denies, so it parses `ps -E` text — split on where the next `KEY=` begins, because every macOS profile lives under "Application Support" and whitespace splitting truncated exactly the demo profiles this check exists for. A value that cannot be recovered makes the caller decline rather than adopt.
- Every unprovable case fails toward *not* signalling and starting a fresh daemon. A stranded daemon is recoverable; a killed neighbour is not.

**Addendum, 2026-08-20 (retro fix wave)**: the no-record fallback is no longer indifferent to *every* flag. It now also rejects a command line that spells out an `--instance` name that is not ours, in either spelling clap accepts. Rationale: "is argv[1] `daemon`" says yes to every mxr daemon on the machine, so a pid recycled onto a neighbouring profile's daemon passed the only check standing between it and a `SIGTERM`. Autostart always passes the marker, so a disagreeing name is the one thing a command line can say on the subject.

It is weighed as evidence, not proof — `--instance` is a marker the daemon parses and discards, and the profile still comes from the inherited environment — so it is confined to *verification*, where a wrong "no" drops through to the `ps` scan and the daemon can still be adopted by canonical executable and environment. The pre-signal re-check deliberately does **not** consult it: that check runs only where the start times agree or are both missing, and its "no" goes straight to clearing the lifecycle files of a daemon it never signalled, with no scan behind it. A name that is absent, value-less, or contains whitespace (unreconstructable from `ps` output) is not a mismatch. Both `ps` command-line probes gained `-ww` in the same change, because a truncated command line reads as a name that disagrees.

**The honesty half of the same change**: a client cannot tell "no accounts" from "the daemon ran out of time reading its database" unless the daemon says so, so `ResponseData::Status` carries `degraded: bool` and every renderer prints `unknown` instead of a zero. `FeatureHealthReport` gained `search`, taken once per report so a finding and the health block cannot disagree. And `mxr doctor --check` exits on `healthy`, which agents and CI read — so `healthy` now means no Error-severity finding, tying the exit code to the thing the report already prints as an error.

---

## D061: Companion processes, not a plugin framework

**Chosen**: Keep canonical mail state and mutation authority in mxr. Put specialized interpretation and orchestration in ordinary out-of-process companions that consume CLI JSON/JSONL or daemon IPC. mxr does not load companion code, maintain a plugin registry, manage companion installation or lifecycle, or expose provider credentials.

**Boundary**:
- Core owns accounts, provider access, sync, messages, threads, labels, attachments, drafts, composition formats, MIME assembly, sending, dry-runs, undo, and mutation validation.
- Companion candidates include LLM assistance, analytics, semantic enrichment, mail merge, and other specialized workflows.
- HTML and plain-text body support are core email capabilities. Expanding a template over recipient data is companion orchestration.
- A companion may analyse data or propose drafts and mutations. mxr remains responsible for validating, persisting, previewing, and executing them.

**Considered**:
- Keep every first-party capability inside the daemon.
- Build a managed plugin system with discovery, installation, lifecycle hooks, and an in-process ABI.
- Leave companion tools to scrape human-readable output or access the local database directly.

**Why companions over a plugin framework**:
- Preserves the Unix model: structured data crosses a documented process boundary and tools may be written in any language.
- Keeps optional dependencies, external services, failures, and release cadence outside the core mail runtime.
- Prevents extensions from bypassing provider abstraction, mutation safety, or the canonical store.
- Uses seams mxr already needs for its own clients instead of creating a second extension architecture.
- Allows strategically important first-party tools without making them required for reliable core mail.

**Migration rule**: This decision is not a mandate to extract existing built-in capabilities. Start with new, clearly bounded companions and improve the structured interfaces they require. Move an existing capability only when there is a concrete product or engineering benefit, not for architectural symmetry.

**Trade-offs accepted**: Companion installation and version compatibility are less seamless than an in-process framework. Cross-process calls add modest overhead, and rich UI integration may require additional protocol work. These costs are preferable to a privileged plugin runtime and a second package ecosystem.

## D095: The desk is the web app's home; arrival order is one key away

**Chosen**: The web app opens on the desk: You owe, Due, Waiting on and New from people, each row saying why it's there. Badges count only work: desk (owed plus due), reply queue and screener. Unread counts are gone from the sidebar and labels. The arrival-order inbox stays one key away (`g i`), and a setting makes it home instead.

**Considered**: Keeping the inbox as home with a desk tab; an unread-count badge beside the desk badge.

**Why**: Deferral is daily for most people, and "unread" gets misused as a todo flag (Microsoft Research, CHIIR 2019). The daemon already knows who is owed what, and leading with arrival order hid that. Counts on newsletters and receipts add anxiety without adding work.

**Trade-offs accepted**: A new user sees an unfamiliar home. The inbox setting covers people who prefer arrival order. (v0.6.36; `21-web-experience.md`)

## D096: One natural-time parser; clients store the previewed instant

**Chosen**: `mxr_core::natural_time` is the only phrase-to-time parser. It resolves in local time, or in the browser's IANA zone for web requests. The daemon's `ResolveTime` previews a phrase. Every client sends the chosen resolved instant (RFC3339) to mutations, never the words.

**Considered**: Keeping per-surface parsers; re-parsing the phrase at commit time.

**Why**: Five parsers disagreed, and one resolved "tomorrow 9am" as 09:00 UTC. Re-parsing at commit lets the stored time drift from what the user saw. (v0.6.34)

## D097: Mail kinds are rule-based, explainable, and corrected per sender

**Chosen**: One classifier (`mail_kind::classify`) places mail as People, Reading or Paper trail, and returns the rule that fired as a human reason. A one-key correction is stored per sender as a screener disposition (Allow, Feed, PaperTrail, Deny). Reading and Paper trail are views over the inbox, and sync keeps INBOX for their mail.

**Considered**: LLM classification; per-domain corrections; a separate kinds table.

**Why**: Users turn off categorisation that is opaque, on by default, or can't be fixed (Apple Mail categories, Spark, Notion Mail). Screener dispositions were already durable, per-sender state.

**Trade-offs accepted**: Rules miss edge cases that a model might catch, but every miss is visible and fixable in one key. (v0.6.38)

## D098: Sweeps commit only what their preview listed

**Chosen**: A sweep's dry run returns a single-use, scope-bound preview token. The commit archives preview ids ∩ still in the place ∩ unpinned. Each chunk rechecks pins and place membership under a per-account gate, taken after the provider lock. A whole-place sweep's confirm opens on Cancel.

**Why**: Rerunning the selection at commit time swept mail that a concurrent sender move or a reused rowid had added. The dry-run rule in `AGENTS.md` means what was previewed is what changes. (v0.6.38, v0.6.39, v0.6.41)

## D099: Done means archived, read, and gone until someone writes again

**Chosen**: Desk Done (`e`, the row check, or a short swipe) depends on the lane:
- You owe and New from people: archive, mark read, and dismiss.
- Waiting on: mark read and dismiss.
- Due: resolve the promise and dismiss, unless another promise on the thread is open.

Reply-later is cleared. A dismissal lasts until any new message is stored in the thread (a rowid/count watermark, not a Date header). Undo restores the per-message state exactly.

**Considered**: Permanent dismissal; Date-header watermarks.

**Why**: BK asked to "resolve an item… no action for me… don't bring it up again and mark read". New mail is new information, so it returns. Kept over permanent dismissal in D105. (v0.6.40)

## D100: Summaries belong in the list; the reader keeps the ask and the facts

**Chosen**: People rows in the desk, inbox, labels, reply queue and owed page show a one-line gist and the ask. The reader shows the verified-ask highlight, facts and promises, and shows a gist only for long threads (4 or more messages, or over 400 words). `GetThreadGists` answers from the cache. Missing gists go to a bounded background writer (people only, newest first, de-duplicated, with backoff). A gist is announced only if its thread gained no message while it was written.

**Why**: BK: "what's the use of me seeing the summary when I've already opened the email?" A summary is a triage tool. AI must never block reading or slow the list. (v0.6.42)

## D101: Delight is rare, earned and optional; feel never costs speed

**Chosen**:
- Low tide appears once per clearing, and is a still frame under reduced motion.
- Sound is synthesised locally, off by default, and follows the daemon chime setting. It never plays on navigation, in a background tab, or for held keys.
- Key hints appear after three pointer uses, once per action ever.
- No points, streaks or scolding copy.
- Keyboard-repeated actions never animate, and held keys never repeat destructive actions.

**Why**: Delight that is frequent, forced or slow becomes noise (Kowalski, Freiberg; Asana's frequency setting; Superhuman on game design over gamification). Speed is what users pay for. (v0.6.39)

## D102: Experience scores are graded independently and measured at real scale

**Chosen**: The experience rubric (v2) requires:
- Scores of 2 or more cite evidence checked by a grader who didn't build the work.
- Section-A scores of 3 need a five-day dogfooding log.
- Real-scale budgets (desk, owed, places, list scroll, new SQL) are measured read-only on a real, large mailbox and recorded with date and size.

**Why**: The v1 pass was self-graded, missed whole dimensions (triage at a glance, shared keys), and was scored on demo data. The worst bugs, a 120-second `mxr owed` and an 88k-row owed list, showed only on a real 110k-message mailbox. (2026-09-30)

## D103: `mxr owed` is the desk's You owe lane; `--all` keeps the raw list

**Chosen**: `ListOwedReplies` returns the desk's You owe lane for one account by default, computed by the desk itself (`get_desk_at`), in the lane's order: mail from people, in the inbox, from someone you have written to, with dismissals, snoozes and reply-later times respected. `older_than_days` and `within_days` narrow it by the latest message from them. `expected_days` is your usual reply time to that person (one day without history), and `usual_seconds` says when that history exists. `all: true` (`mxr owed --all`, `?all=true` on the bridge) returns the previous raw list from the store: every thread whose latest inbound has no later outbound, ranked by contact cadence. `is:owed-reply` in search keeps matching the raw set. The CLI, TUI Owed lens and web Owed page all use the default.

**Considered**: Pushing the desk's filters into the owed SQL as a second implementation; leaving `mxr owed` raw and documenting the difference.

**Why**: On a real 110k-message mailbox the raw list held 89,030 threads, mostly automated and archived mail, while the desk showed 7. Two answers to "who do I owe" taught people to distrust both. Reusing the desk computation keeps one rule and costs 32 to 47 ms in process on that mailbox (the raw path took about 210 ms for 50 rows). Scripts that relied on the wide list keep it behind one flag.

**Trade-offs accepted**: The default only sees conversations active in the last 30 days, like the desk, apart from reply-later returns. Older unanswered threads need `--all`. (2026-10-02)

## D104: Allow makes a person, not a conversation; the Screener is for strangers

**Chosen**:
- Being "in conversation" requires an actual exchange: you sent mail to them (To, Cc or Bcc, per the window or the contacts table), or you replied in that conversation. A screener Allow alone no longer counts.
- An allowed sender is still a person (`mail_kind::classify`), so their mail from someone you have never written to lands in New from people, and moves to You owe once you write. New from people keeps an allowed sender's mail for the desk's whole 30-day window, not only the 7 days it gives strangers, so Allow never takes a conversation off the desk.
- The Screener is for strangers: anyone you have written to is never screened. The screener queue and the desk's screener count both leave out senders you have sent mail to (To, Cc or Bcc), where sent mail follows the desk's rule: stored as outbound, or of unknown direction from one of the account's own addresses.

**Considered**: Keeping Allow as an exchange; a separate "trusted" disposition.

**Why**: Allowing someone says "this is a person I want to hear from", not "I owe them". Counting it as a conversation put first-time senders BK had only screened into You owe. The queue listed everyone with no decision, including people BK writes to: 1,131 of 9,195 queued senders on the real mailbox, and the demo's You owe people as "first-time senders". (2026-10-02)

## D105: Done comes back when someone writes again, by choice

**Chosen**: Desk Done keeps D099's semantics. A dismissal lasts until any new message is stored in the conversation, then the conversation returns to its lane.

**Considered**: Permanent dismissal, so a thread marked Done never returns.

**Why**: A new message is new information, and a permanent Done would hide replies that need an answer. To stop hearing from a sender, the screener (Deny, Feed, Paper trail) is the tool, not Done. Settled with D103 and D104 when the open questions in `21-web-experience.md` were closed. (2026-10-02)

## D107: Email is five modes, each with its own verb

**Chosen**: mxr treats email as five apps sharing one inbox: Messages (reply or start a conversation), To do (do it, schedule it, tick it off), Updates (glance and let go), Reading (read now or later, or unsubscribe) and Archive (file it, find it later). Each mode has its own verbs and rhythm. The full model is [22-email-modes.md](22-email-modes.md).

**Considered**: Keeping the desk plus places (Reading, Paper trail) and adding more places; a single smart inbox ranked by importance.

**Why**: Each kind of mail already has its own action, and the current app makes the user apply the wrong one: a sign-in alert from a person-looking sender sits in You owe as if it needed a reply, and admin with a deadline has no home at all. Places were organised by who sent the mail; modes are organised by what the user does with it.

**Trade-offs accepted**: Five modes plus Inbox is more navigation than one list. The desk stays as the one place to start the day, so a user who never opens a mode still sees what needs them.

## D108: One email can be in several modes, and handoff carries it forward

**Chosen**: A message is in every mode whose aspect it has (the conversation, the task and its deadline, the fact, the article, the record). Each mode keeps its own done state, with the `desk_dismissals` watermark, so done in one mode never clears another. Handoff passes an item to the next mode ("move to To do" after replying, "this needs me" from Updates). The provider archive happens only when the last mode holding the message lets it go.

**Considered**: One mode per message, chosen by precedence (as desk lanes dedupe today); archive as the shared done for every mode.

**Why**: An accountant's "send the signed form by Friday" is a conversation and a deadline at once. Forcing one mode loses one of them, and a shared archive means replying would silently drop the to-do.

**Trade-offs accepted**: Membership is a computed set, not a label, so a client can't read it from the provider. The delayed provider archive can surprise someone who checks Gmail directly; Inbox (everything) shows what modes still hold.

## D109: The rail is the five modes, with Inbox as the everything view

**Chosen**: The web and TUI rail lists Now (the desk), Messages, To do, Updates, Reading and Archive, then Inbox in arrival order. Reply queue, Waiting on and Owed become filters in Messages. Paper trail splits into Updates and Archive. Snoozed becomes a state each mode shows, with the cross-mode list under More. Screener, Invites, Deliveries and Subscriptions move into their mode or under More.

**Considered**: Keeping the current rail (`Sidebar.tsx`: Desk, Inbox, Reply queue, Waiting on, Snoozed, Reading, Paper trail, Screener); adding To do as one more entry.

**Why**: The current rail is a Gmail-style folder list organised by mail mechanics (flags, labels, timers). Users navigate by what they want to do. Adding To do alone would make nine entries and leave the mechanics in charge.

**Trade-offs accepted**: Existing routes and keys (`g q`, `g w`, `g p`) move or redirect, and the keymap parity test must change in the same release for web and TUI.

## D110: The desk is a cross-mode now view, not a sixth place

**Chosen**: The desk shows people you owe a reply (from Messages), to-dos and promises due soon (from To do), and one Updates digest a day (amended by D112: two cuts a day, with the latest as one card). It holds no item of its own: acting on a row acts in the row's mode. New from people moves into Messages.

**Considered**: Retiring the desk in favour of opening on Messages; keeping New from people on the desk.

**Why**: Starting the day needs one answer to "what needs me now?" across modes (D095 still holds). A desk that owns items would be a sixth mode with its own rules, which is how owed and desk counts drifted apart.

**Trade-offs accepted**: The desk's lanes become a composition of mode queries, so its speed budget (D1, under 300 ms warm) now covers to-do and digest reads too.

## D111: Classification is rules first, then the user's own model, with a reason on every item

**Chosen**: Placement runs in layers. Sender rules (`mail_kind::classify`) and screener dispositions set the base mode: Allow is Messages, Feed is Reading, PaperTrail is Updates or Archive, Deny is no mode. Deterministic message rules add aspects (admin verbs, due phrases, schema.org, receipts, invites). The user's configured model then decides what rules can't: whether there is a task, its due words, whether it's a notification. Due words must appear verbatim and are resolved by `natural_time`, never by the model. Corrections are stored per message and per sender and beat every layer. Every item says why it's there and what decided it. Background classification uses a loopback endpoint unless the user opts in for this feature explicitly.

**Considered**: Rules only (D097 as it stood); a model for everything.

**Why**: D097 kept kinds rule-based because users abandon opaque categories they can't fix, and that stays true for the sender's base mode. But sender rules can't see that one message from a person is a sign-in alert, or that a billing email has a deadline; those are message-level facts. Layering keeps every placement explainable and correctable, keeps working with AI off (the default), and keeps mail on the user's machine unless they configured otherwise (`GistPolicy::pin`, `llm_endpoint_is_local`, `relationship_data_allowed`).

**Trade-offs accepted**: This amends D097 and the "no automatic LLM classification" line in `21-web-experience.md` for message aspects only. A local model is slower and less accurate than a hosted one, so accuracy is measured on BK's real mail (counts only) before model placement counts toward the rubric.

## D112: Each mode shows an email its own way, not as a list of subject lines

**Chosen**: Each mode has its own unit, extracted data, up-front actions, rhythm and end state, set by the research in `docs/research/email-modes/`: Messages shows a person with topics inside (group threads keyed by thread, CC-only threads in Updates); To do shows an instruction (verb plus object) on a runway to its act-by date with one labelled button; Updates shows a briefing by source in two fixed cuts a day (08:00, 16:30), with Now showing the latest cut as one card; Reading shows readable items in an edition with a Later shelf; Archive shows records under an answer box. One key map covers Now and every mode, with `e` as done here everywhere. This amends D110's "one Updates digest a day". The full design is [22-email-modes.md](22-email-modes.md).

**Considered**: Five modes that each list emails with the same row (sender, subject, snippet) and only differ by membership; one digest a day.

**Why**: BK: "otherwise we'll end up with 5 good categorizations that just show lists again like traditional emails." Taskmaster found a marker that says "something to do" without saying what did not help planning; HEY, Shortwave and SaneBox fold notifications to one row per source; refinding studies favour records found by search over messages filed by hand. Fitz, Kushlev et al. found a few fixed batches a day helped where hourly did nothing, and a parcel update is stale by the next morning.

**Trade-offs accepted**: Five views and their extractors are more code than five filters, and each view's quality depends on extraction accuracy, which is measured on BK's real mail (rubric v3) before the shape counts as done.

## D113: Each mode indexes the part of the email it cares about

**Chosen**: Content units are extracted once per message (new text without quotes and signatures, article or link sections, record fields, attachment text, the gist). A typed, versioned recipe per mode in `crates/semantic` picks units, windowing and a context prefix: Messages indexes each message's new text prefixed with person and topic, plus the gist; To do one instruction chunk plus the body; Updates one fact chunk per message, deduplicated by template; Reading section-aware chunks and link items, embedded lazily; Archive a field chunk plus PDF text, leaving identifiers to BM25. Chunks are tagged with every mode they serve, embeddings are keyed by a hash of the chunk text, a baseline (header plus new text) is indexed at sync and enriched after classification, and messages carry recipe and classification versions so only stale ones reindex through the resumable index job. One embedding model and one ANN index per profile stay; search gains a mode filter beside `allowed_source_kinds`.

**Considered**: Keeping one recipe for all mail (`build_chunks`: a header chunk plus 120-word windows with 30-word overlap); a separate index per mode; recipes as user config from the start.

**Why**: One recipe re-embeds quoted history in every reply, splits a record's fields across windows, and spends embedding work on newsletters that are never read. Keying embeddings by text means a message in several modes costs one embedding per distinct chunk. A table in code can be versioned and tested; config can follow once the recipes settle.

**Trade-offs accepted**: Two passes per message (baseline, then enrichment) and a version stamp to track. Each recipe replaces today's chunking only when a local retrieval eval on BK's mail (top-5 hit rate, counts only) shows it at least as good.

## D114: Model work runs in a fast tier and a smart tier

**Chosen**: Bulk, low-nuance work (mode classification, Updates facts, baseline index extras) runs on the fast tier, the local model by default. Nuanced extraction (To do fields, Archive record fields, Messages' ask) runs on the smart tier: the user's cloud model when they configured one with their own API key, otherwise local with the fields marked unchecked. Only mail already classified into that mode reaches the smart tier. Config is `llm.tiers.fast` and `llm.tiers.smart`, each an `LlmOverrideConfig` inheriting from `[llm]`, with a fixed feature-to-tier table in code; `llm.overrides` stays for the features it covers today, with precedence override, then tier, then base. Each tier is pinned per request as `GistPolicy::pin` does; a cloud tier needs an API key, and enabling it names the tier and what it sends. Amounts and dates must appear verbatim and are checked in code, dates go through `natural_time`, the provenance chip names the model, and results are cached by content hash. Sign in with ChatGPT is parked until OpenAI publishes data terms for plan usage (`docs/issues/chatgpt-plan-usage-data-terms.md`).

**Considered**: One model for everything; a per-feature override for each new mode feature; Sign in with ChatGPT as the cloud credential.

**Why**: BK decided this on 2026-10-02. Classification reads every message, so it belongs on a local model; extraction of amounts, deadlines and links is where a stronger model pays, and restricting it to mail already in To do, Archive or Messages keeps the cloud's share small and bounded. A fixed table keeps the privacy story explainable; per-feature overrides for every new feature would multiply settings nobody tunes.

**Trade-offs accepted**: Users without a cloud model get local extraction with more unchecked fields. The default tier per task is not fixed in advance: `mxr modes eval --extract` compares local and cloud on the user's own mail and records counts, not content. A user may run both tiers in the cloud with no local model (BK, 2026-10-02): `[llm]` on a cloud endpoint plus `allow_cloud_background_classification = true`, with the opt-in stating that every incoming message is sent. No mode feature may require a local LLM server.

## D115: Every mode item has a relevancy window derived from its content

**Chosen**: Each item in To do, Updates, Reading and Now carries `relevant_from`, `relevant_until` and a `window_source`. The end comes from code, never a model: schema.org (`Event.endDate`, `Offer.validThrough`, `ParcelDelivery.expectedArrivalUntil`, `Invoice.paymentDueDate`), ICS `DTEND`, Gmail's `availabilityEnds`, or quoted words resolved by `natural_time`, with a per-kind default table (a one-time code 10 minutes, a verify link 3 days, a sign-in alert 2 days, a bill due plus 14 days, an offer 7 days) configurable per kind and per sender. Past its window an item leaves To do, Updates and Now on its own: to-dos move to an `expired` state, facts drop out of cuts and never break through, and record-worthy items file to Archive. Expiry never touches a to-do the user made or edited, never archives at the provider, and is shown as one "N expired since you last looked" line with restore, never a badge. Messages don't expire; the turn decays to Quiet. Nothing past its window enters Now. The design is "Items have a relevancy window, and the first run uses it" in [22-email-modes.md](22-email-modes.md).

**Considered**: One age window for everything (the desk's `DESK_WINDOW_DAYS = 30` today); no expiry, with "was due" rows and a reschedule-all button as Todoist does; letting a model judge staleness.

**Why**: BK, 2026-10-02: notifications "don't dismiss even after the thing is clearly stale... Notifications have a relevancy period." His store has 63 of 83 active deliveries with no event for over 30 days, because the active list has no age bound, and 50 unanswered RSVP requests, all for events already over. Android (`setTimeoutAfter`), ActivityKit (`staleDate`), Wallet (`expirationDate`, with expired passes hidden to a list you can unhide) and Gmail's deal annotations all put the end on the item; iOS notifications have no end time, which is why stale alerts linger. One age window is too long for a code and too short for a passport. Dates from code keep D111's rule that models don't invent dates.

**Trade-offs accepted**: The defaults without a source are judgement and will be wrong for some senders, so restores from the Expired list are counted to tune them. One timestamp per mode is stored to compute "since you last looked". A bill that expires unpaid relies on the biller's next email to reopen it.

## D116: The first run classifies newest first, lets windows clear history, and gives the undated rest one bounded catch-up

**Chosen**: Classification reads a queue ordered by message date, not sync order; the last 14 days run first through rules and the fast tier so Now is useful within minutes, then history runs newest to oldest in the background with progress. Windows apply during classification: an item already past its window is expired at birth, never surfaced and summarised in one line. Open-ended items (undated to-dos and promises, owed replies) from the last 14 days form one catch-up batch shown once, at most 25 rows, with keep or let go per row and "let go of all" behind a dry-run preview that equals the commit; older open-ended items are expired at birth. Beyond 90 days history is rules only (`modes.model_history_days`), and smart-tier record fields for older mail run lazily. Every item keeps a stable claim (`dedup_key`, `surfaced_at`, `expired_at`), so a rule or recipe change re-evaluates fields without re-surfacing anything, and new finds in old mail pass the same window and cap.

**Considered**: Classifying everything and letting the user triage it; HEY's fresh start (no history in the modes at all); Superhuman's Get Me To Zero (the user picks a cut-off and archives the rest); running the fast tier over all history.

**Why**: BK, 2026-10-02: "when we do the first classification I imagine there'll be tonnes of incoming todos and all that of old things." His store has 1,842 open promises, 93% undated, 90 of them from the last 14 days; as specified, `ListTodos` would open with all of them. Every product that handles a backlog bounds the window (HEY Screener 90 days, Superhuman a chosen time frame, Sunsama four days), saves what matters by a cheap signal, and makes the bulk action reversible; Todoist's own writer abandoned the app at around 50 overdue tasks. Over 90% of email replies come within a day (Kooti et al., WWW 2015), so an undated item older than two weeks is rarely still live. Model output on mail past its window would be discarded, so spending it there is waste; 90 days on BK's store is 4,605 messages against 110,285. Linear skips already-imported issues on re-import, the precedent for re-runs never re-flooding.

**Trade-offs accepted**: A live undated item older than 14 days is missed until a new message revives it or the user restores it from the Expired list. The IMAP adapter must page newest first, since its initial sync fetches ascending UIDs and returns the folder in one batch today. The cap of 25 is judgement, and on BK's mail undated promise precision decides whether the catch-up fits under it.

## D117: The open modes questions are decided, so building can start

**Chosen**: The values calls left in blueprint 22 are decided as listed in its "Decisions made on BK's behalf" section.
- **Money and links:** no one-click pay link in phase 1 (amended 2026-10-03: the action opens the email with the link highlighted, after two review rounds broke every gate; see `docs/issues/one-click-pay-link.md`), and no weekly money total.
- **Archive and Reading:** archive on last done stays on; highlights go to Archive search and a Markdown export.
- **Accuracy and identity:** a measured bar for the To do badge, and manual person merge with suggestions.
- **Interaction and privacy:** Got it stays on `.`; Reading engagement tracking is governed by `MXR_ACTIVITY`.
- **Expiry and first run:** promises and bills never expire silently; the catch-up window is asked once, defaulting to 14 days; the expired line stays.
- **Models and indexing:** GPT-6.1 Sol as the default escalation model, no TypeSafe Jev, and index recipes in code.
- **Deferred or declined:** Gmail offline deletion is deferred, and the Sign in with ChatGPT SDK is not built.

**Considered**: Leaving them open for BK.

**Why**: BK, 2026-10-02: "review and decide for me, I just want a working app." Each choice follows the research notes. Where they were silent, it takes the option that keeps data and asks less of the user.

**Trade-offs accepted**: Several defaults (the badge bar, the 14-day catch-up, Sol) are judgement calls that later use may overturn; each is a config value or one line in the plan.


## D118: The app teaches itself in place, with no tour

**Chosen**: mxr teaches the modes where each one is used and never with an onboarding tour. Every mode ships six surfaces: a header line under 12 words that names its job and verb; two empty states ("never had any" teaches what lands there, "clear for now" says when the next thing arrives); one first-encounter card of at most two sentences plus a key line, which closes on dismiss or on the mode's first verb and never returns, with seen state kept by the daemon per profile; a why line on every item with a what-next fragment; `?` leading with the mode's explanation before its keys; and keys shown with their verbs in the footer, tooltips and palette. Handoff toasts name where an item went, and always separate "Filed in Archive" from "Archived in Gmail". The first run is the one moment all five modes are shown together, as counts from the user's own mail with each mode's header line, followed by the catch-up. `mxr demo` and the glossary are offered, never pushed. The copy lives in one daemon table (`GetModeGuide`, `mxr modes explain --format json`) so every client and agent uses the same words. Each phase's definition of done includes its mode's surfaces and copy, and rubric v3 X13 grades them with a five-second check. The design and copy drafts are "The app teaches itself in place, with no tour" in [22-email-modes.md](22-email-modes.md).

**Considered**: A multi-step tour or deck of cards on first launch; chained coach marks; a tips page or "what's new" notifications; a human onboarding call as Superhuman ran; no explicit teaching, leaving the modes to explain themselves.

**Why**: BK, 2026-10-02: "since we're changing the email app to something people aren't used to, we need to explain ourselves a lot and teach people what the app is what things are and how to use them. I don't mean with a long intricate onboarding tour, I don't like those." NN/g finds tutorials interrupt, don't improve task performance and are forgotten, and that help triggered by the user's moment of need works better; Apple's HIG prefers context-specific tips and says not to show a tip for a feature someone already used. Carroll's minimal manual and Lazonder and van der Meij's replication found short, task-attached instruction teaches faster, while Kirschner, Sweller and Clark warn that no guidance fails novices, so each mode states its job explicitly. Explaining a classifier's decisions raised users' understanding by 52% (Kulesza et al., IUI 2015), which makes the why line the strongest surface. A call needs a person and the user's mail, which a local-first app has neither of. The research is [teaching-in-place.md](../research/email-modes/teaching-in-place.md).

**Trade-offs accepted**: Six surfaces per mode is more copy to write, test and keep true; the copy test and the daemon table keep it in one place. Someone who closes a card unread relies on `?` to find it again. The evidence is about contextual help in general, not about email modes, and the five-second check measures first impressions rather than use, so dogfooding has to confirm it.

**Amended 2026-10-07: hints at their element replace the first-encounter card, and the TUI tour is gone.** The card shown at the top of each mode is removed from the web app and the TUI, and so is the TUI's six-step "Start Here" walkthrough, which still described the pre-modes model. In their place each mode has hints: one sentence that names its key, attached to one element (the first row's why line on Now, the first runway bar in To do, a person's topic list in Messages, the first answer in Archive), shown the first time that element is needed and never again once dismissed by Esc or by acting on it. At most one shows at a time, none shows on arrival before the user has pressed a key or clicked on the page unless its element is the only thing there, and dismissing one never reveals the next. The web app renders a hint as an inline note under its element; the TUI puts it in the status line while the cursor is on the element. The first done here explains itself in its handoff toast. Seen state is kept per hint id in the daemon (`SetHintSeen`, `mxr modes hint`, migration 69 replacing `mode_guide_seen`), so web and TUI share it. The card's text stays as each mode's `about`, shown by `?` and `mxr modes explain`. The header line, empty states, why lines, `?` and keys with verbs are unchanged.

*Why*: BK, 2026-10-07: "Adding one big hint at the top of a page has essentially the same problem as a product tour: it front-loads information before the user needs it or even has enough context to understand where it applies... surface information at the moment of need, co-located with the UI element or action it relates to, rather than batching it upfront." The original evidence already pointed here: NN/g's help at the moment of need and Apple's context-specific tips. Audit A (ux-polish, 2026-10-07) found the TUI still opened a tour on first launch, which X13 fails outright.

*Trade-offs*: A hint only teaches the user who reaches its element, so a mode's model is no longer stated in full on first visit; the header line states the job and `?` holds the full explanation. Hints need an anchor per element in each client, which is more wiring than one card per mode; the anchors are named in one table in [22-email-modes.md](22-email-modes.md) ("The hints").


## D119: Sorting shows its work: freshness, an arrivals line that sums, and mail that is never sorted away

**Chosen**: Three trust signals, in order. First, freshness: the status bar's "Latest mail 5m ago · synced 30s ago" with sync warnings and the last five arrivals (`GetFreshness`, `feat/freshness-indicator`). Second, one quiet line on Now that accounts for every inbound email first seen since the user last opened Now: the counts per primary mode, plus Spam and screened out, sum to the total; To do and Archive are shown as "also" and never added; each count opens those emails in arrival order. Sync stores one arrivals row per inbound message (mode, rule, first-seen time, mode after a correction), because computed membership drops mail once it leaves the inbox. Third, four never-bury rules shown in the app: mail addressed to you from someone you've written to reaches Messages over any list or no-reply rule; security alerts and failed payments reach Now and dated deadlines reach To do, within their relevancy windows. Inbox rows name the mode each email went to. "Not sure" covers only rule conflicts (someone you've written to who only copied you; later, the fast-tier model disagreeing with the rules), at most three a day on Now. The weekly track record waits until per-email corrections are stored. "Clear" means accounted for, never "seen". Rubric v3 X14 grades it. The design and copy are "Sorting shows its work, so nothing feels hidden" in [22-email-modes.md](22-email-modes.md).

**Considered**: Leaving Inbox as the only reassurance; a daily digest of everything sorted away (SaneBox's model); a confidence score on every placement with a threshold; a "you've seen everything" marker; a track record shipped now from screener decisions; counting threads or people instead of emails.

**Why**: BK, 2026-10-07: "since it reorders emails, I'm always thinking: am I missing some important email? So from time to time I find myself jumping to the inbox tab just to see emails in order of arrival." He then said he only checks the newest timestamp there. Lee and See (2004) tie appropriate trust to showing an automation's purpose, process and past performance, which the rules, the arrivals line and the track record do. One discovered miss turns a whole tab into a place users sweep (Gmail tab complaints on Hacker News), so the mail where a miss costs most is never sorted away; Apple Mail already copies time-sensitive mail into Primary for the same reason. False alarms cause disuse (Parasuraman and Riley 1997), which is why "Not sure" is capped. Gmail's tabs and Outlook's Focused Inbox both kept a way back to everything, and Outlook tells users about mail sent to Other; SaneBox's digest exists so people don't miss mail and works because corrections happen inside it. The arrivals line keeps the user in the sorted view while showing that the sort lost nothing. On BK's mail (read-only copy, counts only, 2026-10-07): the last 24 hours had 50 arrivals (1 Spam, 8 Messages, 10 Updates, 31 Reading) and seven days had 461 (26 Spam, 54 Messages, 236 Updates of which 21 deliveries, 145 Reading), both summing, with no thread split across two primary modes. 242 of the 435 live seven-day arrivals had left the inbox and fall out of today's computed membership. Of 21 seven-day arrivals from people BK has written to, 3 went to Reading by List-Unsubscribe and 4 to Updates as copied; 12 security-looking alerts all went to Updates and none to To do. No confidence score exists, and BK's store has 0 sender decisions and 0 dismissed to-dos, so a track record today would read "you moved 0" for want of a way to move. The research is [trust-and-completeness.md](../research/email-modes/trust-and-completeness.md).

**Trade-offs accepted**: A stored arrivals row is a second record of placement next to computed membership (D097), so the two can drift; the row records where mail went, membership says where it is. The N2 security rule needs a detector that does not exist yet, and until it ships the in-app wording names only the rules that are true. Counting emails can read high on a busy day; the line stays muted and drops zero counts. The track record counts moves, not misses nobody noticed, and says so.
