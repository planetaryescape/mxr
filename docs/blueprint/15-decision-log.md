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
- Mirrors the provider kit's `run_sync_conformance<P>` shape: generic functions in the reference-impl leaf crate, consumed via one dev-dependency and a `#[tokio::test]`. Proven end-to-end by an out-of-tree scratch crate that implements its own transport and passes the suite depending only on `mxr-transport`.

**Trade-offs accepted**: two conformance entry points instead of one. Justified — they test genuinely different contracts (byte-stream lifecycle vs. protocol semantics) and have different, correct homes.
