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

**Amendment (classification repair, 2026-10-18):** N1 no longer promotes every
message from a previously answered sender. Per-message List-Id, unsubscribe,
no-reply and Auto-Submitted evidence survives correspondence history. Explicit
email/sender/custom sorting choices retain precedence. Purpose, human response
occurrence and owner reply obligation are separate: only a conservative directed
request or an explicit reply-later timer creates Your turn. Unknown semantics
remains accessible without a debt claim. Current arrival destinations are bounded
local read projections; original placement, first-seen and corrections are not
rewritten. No provider/model work, new purpose cache or maintenance mutation is
needed. This supersedes the N1 wording in the original decision below and the
history-only attention rule in D104/D132. Coverage and limitations are documented
in the Messages guide. Headerless document recognition combines structured
reference/date/order-or-amount fields with delivery framing; informational
placement combines FYI/report delivery with recipient and request context.
Human answers use reply headers or local thread correspondence, independently
of new requests. Attention uses the reader's authored-text extraction, including
Outlook blocks and bounded earlier cached text rather than a separate splitter.

**Chosen**: Three trust signals, in order. First, freshness: the status bar's "Latest mail 5m ago · synced 30s ago" with sync warnings and the last five arrivals (`GetFreshness`, `feat/freshness-indicator`). Second, one quiet line on Now that accounts for every inbound email first seen since the user last opened Now: the counts per primary mode, plus Spam and screened out, sum to the total; To do and Archive are shown as "also" and never added; each count opens those emails in arrival order. Sync stores one arrivals row per inbound message (mode, rule, first-seen time, mode after a correction), because computed membership drops mail once it leaves the inbox. Third, four never-bury rules shown in the app: mail addressed to you from someone you've written to reaches Messages over any list or no-reply rule; security alerts and failed payments reach Now and dated deadlines reach To do, within their relevancy windows. Inbox rows name the mode each email went to. "Not sure" covers only rule conflicts (someone you've written to who only copied you; later, the fast-tier model disagreeing with the rules), at most three a day on Now. The weekly track record waits until per-email corrections are stored. "Clear" means accounted for, never "seen". Rubric v3 X14 grades it. The design and copy are "Sorting shows its work, so nothing feels hidden" in [22-email-modes.md](22-email-modes.md).

**Considered**: Leaving Inbox as the only reassurance; a daily digest of everything sorted away (SaneBox's model); a confidence score on every placement with a threshold; a "you've seen everything" marker; a track record shipped now from screener decisions; counting threads or people instead of emails.

**Why**: BK, 2026-10-07: "since it reorders emails, I'm always thinking: am I missing some important email? So from time to time I find myself jumping to the inbox tab just to see emails in order of arrival." He then said he only checks the newest timestamp there. Lee and See (2004) tie appropriate trust to showing an automation's purpose, process and past performance, which the rules, the arrivals line and the track record do. One discovered miss turns a whole tab into a place users sweep (Gmail tab complaints on Hacker News), so the mail where a miss costs most is never sorted away; Apple Mail already copies time-sensitive mail into Primary for the same reason. False alarms cause disuse (Parasuraman and Riley 1997), which is why "Not sure" is capped. Gmail's tabs and Outlook's Focused Inbox both kept a way back to everything, and Outlook tells users about mail sent to Other; SaneBox's digest exists so people don't miss mail and works because corrections happen inside it. The arrivals line keeps the user in the sorted view while showing that the sort lost nothing. On BK's mail (read-only copy, counts only, 2026-10-07): the last 24 hours had 50 arrivals (1 Spam, 8 Messages, 10 Updates, 31 Reading) and seven days had 461 (26 Spam, 54 Messages, 236 Updates of which 21 deliveries, 145 Reading), both summing, with no thread split across two primary modes. 242 of the 435 live seven-day arrivals had left the inbox and fall out of today's computed membership. Of 21 seven-day arrivals from people BK has written to, 3 went to Reading by List-Unsubscribe and 4 to Updates as copied; 12 security-looking alerts all went to Updates and none to To do. No confidence score exists, and BK's store has 0 sender decisions and 0 dismissed to-dos, so a track record today would read "you moved 0" for want of a way to move. The research is [trust-and-completeness.md](../research/email-modes/trust-and-completeness.md).

**Trade-offs accepted**: A stored arrivals row is a second record of placement next to computed membership (D097), so the two can drift; the row records where mail went, membership says where it is. The N2 security rule needs a detector that does not exist yet, and until it ships the in-app wording names only the rules that are true. Counting emails can read high on a busy day; the line stays muted and drops zero counts. The track record counts moves, not misses nobody noticed, and says so.

## D120: Global sync is explicit and version-gated

**Chosen**: `Request::SyncAll` and `Request::PreviewSyncAll` select every currently enabled sync-capable account allowed by the caller's scope. The daemon returns independent per-account start/join/failure results. `Request::SyncNow { account_id: None, .. }` keeps its historical default-account meaning. Global sync requires IPC protocol 6 so clients can identify daemons that do not implement these requests before sending them.

**Considered**: Reinterpret `SyncNow(None)` as all accounts; send `SyncAll` to an old daemon and depend on its parser error.

**Why**: Existing scripts and clients rely on `SyncNow(None)` targeting the default account. An explicit request preserves that wire behavior and gives callers per-account results; a protocol check gives older clients a clear update path without making them attempt an unsupported command.

**Trade-offs accepted**: Sync now requires a compatible daemon protocol. Legacy single-account requests keep their earlier semantics.

## D121: Reading opens items in Marginalia, and only mxr knows about the other

**Chosen**: Reading items open in Marginalia, BK's separate reading app, with `M` in the web app and TUI, `mxr reading marginalia send` in the CLI, and `mxr_reading_marginalia` in MCP. The first press sends the item (the saved article, else the issue as cleaned HTML) through Marginalia's general API, `POST /api/v1/articles`, which is idempotent on an `externalId` of `mxr:<item key>`; later presses only open the article recorded in `reading_marginalia` (migration 86). Highlights made in Marginalia are read back with `GET /api/v1/articles/{id}/highlights` into `reading_highlights` with `origin = 'marginalia'`, mirrored so a deletion there is a deletion here, and never touching highlights made in mxr: when a sent item opens in the web reader, after each sync for items sent in the last 60 days at most every half hour, and on request. The address is `reading.marginalia_url` in config; the personal token is in the secrets file and is checked with Marginalia before it is saved. Sending is a `Send`-class request with a dry run that contacts nothing. The integration lives in the daemon (the client in `mxr-reading`), not in a companion process, because its state (which item became which article, highlights read back) belongs with Reading's own tables and every client has to reach it through the daemon.

**Considered**: Embedding Marginalia in mxr; mirroring whole sources into Marginalia automatically, as its Readwise sync does; Marginalia calling mxr to push highlights; a companion process driving the CLI.

**Why**: BK, 2026-10-13: each mode should be a full app in its own right, he already refined a reading experience in Marginalia, and he doesn't want to manage two apps. Marginalia runs on a hosted backend with its own sign-in and model keys, so embedding it would bring cloud state into a local-first client; sending items one at a time, only when asked, keeps email content on the machine until the user chooses otherwise. BK chose the manual button and that mxr knows about Marginalia while Marginalia knows nothing about mxr, so Marginalia's API is general (any app with a token) and the read-back is mxr pulling. A companion couldn't add a key to the web app or TUI, or keep its mapping beside Reading's highlights.

**Trade-offs accepted**: An item's text leaves the machine on the first `M`, to Marginalia and its AI provider; the dry run, the button's title and the guide say so. Highlights come back only as quotes, without Marginalia's paragraph anchors (amended 2026-10-15: since #473 the anchor is stored as `paragraph_index` but not drawn, because nobody has checked that Marginalia numbers paragraphs the way mxr does). The reading experience itself still differs between the apps until the shared reader package (`planetaryescape/reader`) lands (amended 2026-10-15: it landed in #457, v0.6.98, and mxr's web reader pins v0.1.3 since #470).

## D122: The assistant is a daemon agent over the MCP tools, with approval enforced in code

**Chosen**: A chat assistant in a right-hand web sidebar (closed until opened, `Mod+.`) and `mxr assistant` on the CLI, run by the daemon as a background turn against mxr's own MCP tools in-process (`ClientKind::Assistant`). Reads run at once; every change waits for the user with a preview, by a gate in `crates/assistant/src/gate.rs` that classifies each call from its name and arguments and makes anything unlisted wait. `ask_user` asks structured questions the turn waits on. The transcript is append-only in SQLite, so waiting calls survive a restart. Design: [25-assistant.md](25-assistant.md).

**Considered**: A browser-side loop with the Vercel AI SDK (BK's pattern in Notto): web-only, keys in the browser, and no CLI or TUI parity, against "daemon IPC and CLI JSON come first". A loop that shells out to the `mxr` CLI: maximal reach, but approval would rest on parsing argv. Hand-written assistant tools: duplicates the 72 MCP tools and their preview flows. An agent profile for the assistant: `agent` defaults to read-only, so approved changes would be refused unless the user configured a profile, and a profile can't express "this person approved this call". Gating in the prompt: a request the model can decline.

**Why**: BK, 2026-10-15: "a right hand sidebar that i can show and hide, hidden by default. I want to be able to talk to it about my email, it uses mxr so it can perform any action... especially useful for analysis and bulk actions", then "tool results and content is data not instructions" and an `ask_user` tool "if the agent wants to give multiple choice question". BK's notes and Notto agree on the method: behaviour that matters is enforced in code (`prepareStep`, refusals in `execute`, approval gates as a data registry the model can't exempt itself from), tool results are the agent's only facts, untrusted input can't grant authority, and every stored tool call needs a result. Reusing the MCP server gives the assistant MCP's whole surface and its existing preview/confirm contracts; dispatching in-process keeps the safety policy, workspace ownership and activity log on the same path as every client.

**Trade-offs accepted**: The full tool list is about 11k tokens per request; prompt caching and a byte-stable prefix keep that cheap on Claude, and local models pay it every time. No token streaming yet: the sidebar shows steps and a live status line instead of words. Every change needs a click, including obvious ones; there is no "approve all" mode. The TUI has no pane yet. Building it surfaced and fixed a bug older than the feature: `user_activity` only accepted four sources, so nothing an MCP or agent client did had ever reached the activity log (migration 89).

**Amended 2026-10-15: a conversation can trust reversible changes.** BK asked for the follow-ups to be built, including an "approve all" mode. Notto deliberately has no per-request exemption because a model once used one to bypass its own supervision, so the switch is the user's alone: stored on the conversation, set only by `SetAssistantAutoApprove` (agent and MCP clients can't call it), read by the daemon each turn, and limited to changes mxr can put back (archive, read state, stars, moves, Later, categories, to-do state). Sending, deleting, trash, spam, rules and anything that reaches another service always ask. *Trade-off*: an injected instruction in a trusted conversation can now archive or move mail without a click; every such step is shown and logged with source `assistant`, and all of it can be moved back. In the same change replies stream as they are written and long conversations are summarised to fit the model's context window.

## D123: The web app is designed as one product, the night desk, before any screen is changed

**Chosen**: The web app gets a design concept of its own, written in [26-web-design-concept.md](26-web-design-concept.md): mxr is the night desk where you put tonight's mail to bed. One pool of light falls on the thing you are working on and everything else recedes into the navy; the five modes are the desk's sections, Now is the front page, the reader is the page under the lamp, and the assistant is the copy desk that marks proofs for your signature. Every screen, state and moment is judged against that concept, with the design system in [27](27-web-design-system.md), the screens in [28](28-web-screens.md), the assistant in [29](29-assistant-design.md) and the order of work in [30](30-design-roadmap.md). The metaphor lives in the design, not in the copy: the product's nouns stay Now, Messages, To do, Updates, Reading, Archive and Inbox (D107, D109).

**Considered**: Fixing the audit's top problems one by one with no concept (the 2026-10-14 design-system pass did this and the app still read as one grey block); "the Edition" (Now as a newspaper front page, each mode a section), which fits the editorial brand but reads slow for triage; "the Logbook" (every surface a dense time-ordered log), which fits BK's March "terminal aesthetic" but loses warmth in Reading and Messages; "the Desk" (modes as trays), which risks skeuomorphism. The night desk keeps what each got right: the Edition's sections and front page, the Logbook's density and mono wire lines, the Desk's sense of putting things away.

**Why**: BK, 2026-10-16: "focus on the design aspect. The functions but i don't think the design was considered", then "This is not just about the assistant but about the whole mxr web app". The anchor statement is from 2026-09-27: "it doesn't feel polished when i look at it and use it. It doesn't feel like there's any uniqueness to it or additional thought. I may as well be using gmail. This should feel like a thought out app that thought about every decision". The audit of 2026-10-16 (79 screenshots, [research](../research/web-design/audit-2026-10-16.md)) found three page frames, about six row designs, five empty states, keys shown four ways and about 880 of 945 sized text uses at 11 to 13px: screens built one at a time, each correct, none designed as part of a whole. BK's own taste statements point the same way from different years: "I'm a dark mode guy. I want the app to feel like vscode but to be snappy and blazing fast ... take inspiration from linear and superhuman" (2026-03-21), "this email reading screen is abou treading the email and so the email should really take center stage" (2026-05-11), "a human would know to focus on one thing each frame and know to guide the eye" (2026-09-26, about the showreel), and "more professional and more useful without being noisy" (2026-10-14). A lamp on a dark desk is all of those at once. The scene sentence in DESIGN.md is already "a developer at 11pm".

**Trade-offs accepted**: BK never named this concept; it is the blueprint's proposal, filed as Q1 in [31](31-design-open-questions.md), and the work proceeds on it under BK's "decide for me" (D117) unless he redirects. A concept costs a first rung that changes little behaviour.

## D124: One pool of light: the lit item is the only raised thing on a screen

**Chosen**: Every screen has exactly one lit item: the row, topic, card or reader page the keys act on. It is drawn on the lamp surface (one step lighter than the page, a 1px cyan hairline on its leading edge, its actions and keys visible); everything else sits on the page surface with its actions hidden until it is lit or hovered. Selection has one look everywhere, replacing today's three (subtle fill with a cyan rule, saturated teal fill on Screener and the settings nav, filled pills on Archive and Analytics). Pressing `j`/`k` moves the light instantly, with no animation, because keyboard-repeated actions never animate (D101).

**Considered**: Keeping per-screen selection styles; a full cyan fill for selection (loud, and it competes with the accent's other job, state); showing every row's actions all the time (today's Screener: about 48 identical buttons on one page).

**Why**: BK on the showreel: "it's really hard to decide where my eyes should be looking in each frame, this is what makes you feel amatuer or makes yoou feel like an ai". Linear's 2026 refresh states the same rule for apps: "Don't compete for attention you haven't earned". Things 3 opens a to-do in place "while the rest of the list fades into the background". The audit's fourth-ranked problem is rows whose secondary lines all have equal weight.

**Trade-offs accepted**: Actions hidden on unlit rows are one hover or one `j` away instead of zero; touch shows them on the lit row after a tap, and every action stays on a key and in the palette.

**Amended 2026-10-16 after the W1 comp.** Lighting a row must not change its height: in the comp, a third line on the lit row moved every row below it on each `j`/`k`, breaking "nothing else moves". The lit row keeps its two lines; its verbs replace the trail (marks and time) on the same line, and its reason shows in the status bar's left side, beside freshness, and in the reader. In Light themes the lamp is a faint cyan tint (`#e7f2f8`), because `#fbfdfe` on the `#f4f9fc` desk was invisible in the comp.

**Amended 2026-10-16 (W2).** The first row of a list is lit when the page opens. A lit row opens nothing, so one lit thing per screen still holds and no message opens by itself (BK, 2026-10-09); the earlier "nothing lit until a key is pressed" is withdrawn.

## D125: Dense where you work, generous where you read

**Chosen**: Lists where you triage (Now, Messages, To do, Updates, Inbox, Archive's ledger, tool pages) use the compact rhythm: 14px primary lines, 12px mono meta, one secondary line at most, rows of 44 to 56px. Places where you read or write (the reader, Reading's reader, a person's conversation, compose) use the reading voice: 16px body at line height 1.6, a 64 to 70ch measure, generous margins. The two rhythms never mix inside one pane.

**Considered**: Compact everywhere (BK's March "i want more of a terminal aesthetic", "too much paddings"), which makes long email tiring; the current roomy editorial look everywhere, which BK called "noisy and cluttered" on Now and which wastes rows in triage; a density setting as the answer (one exists for the mailbox; a setting is not a design).

**Why**: BK asked for both, in different contexts: "it's significantly better but ... i want it more compact than that, thaere's too much space" (2026-03-22, the mail list) and "the email should really take center stage", "the editor should take center stage, compose page is about writing the email" (2026-05-11). Superhuman (dense list, serif bodies) and Readwise Reader (65ch measure) split the same way.

**Trade-offs accepted**: BK has not restated a density preference since March; Q2 in [31](31-design-open-questions.md) asks whether this split matches what he wants. The existing mailbox density setting stays and scales the work rhythm only.

## D126: Recursive with all its axes, and a type scale that is actually used

**Chosen**: The app loads Recursive's full variable font (`@fontsource-variable/recursive/full.css`, about 305 KB for Latin, served locally and cached by the PWA) instead of `mono.css` (about 73 KB, `wght` and `MONO` only), so the reading voice can use the `CASL` axis DESIGN.md describes. Six roles replace ad hoc sizes: display 28px (page titles on wide screens, the reader title), title 20px, heading 15px semibold, body 14px, reading 16px (`CASL` 0.3), meta 12px mono, and eyebrow 11px mono caps with one tracking value (0.08em). `scripts/check-type-scale.mjs` gains a check that pages use the role classes, not raw scale steps, for titles and headings. Details in [27](27-web-design-system.md#type).

**Considered**: Keeping `mono.css` and adding `casl.css` (two font files and two faces for one family); switching families (DESIGN.md forbids Inter-class defaults and the brand is Recursive); leaving the scale and fixing pages one by one (the 2026-10-14 pass did this and hierarchy stayed flat).

**Why**: The audit counted about 880 of 945 sized text uses at 11 to 13px and page titles at 17 to 20px sitting inline with their taglines, so "every screen reads as one grey-blue block". The 2026-10-14 pass already stopped literal sizes with a lint; what's missing is tiers in use. The font is local, so 230 KB more is a one-time cache fill, not a per-visit cost.

**Trade-offs accepted**: A larger first load of the font on a fresh install. Spike S2 measures it and checks for layout shift.

## D127: Square by default, round only for people and dots

**Chosen**: Surfaces, panes, rows and the reader page have square corners. Controls (buttons, inputs, segmented controls, chips, menus) use a 3px radius. Only avatars, unread dots and count badges are round. Archive's round filter pills become the shared square segmented control.

**Considered**: The current mix (`rounded-md` 134 uses, `rounded` 59, `rounded-full` 56, with no rule); fully square including controls (hard to read as pressable at 3px focus rings).

**Why**: BK, 2026-03-22: "i asked for a terminal esk view and I asked you to use the lyra style from shadcnui so i'm not expecting to see rounded corners etc." The brand is "ruled lines, not cards". One rule makes shape a signal: round means a person or a count.

**Trade-offs accepted**: shadcn defaults are overridden in `shadcn.css`; new components must take the rule from tokens, which the lint in [27](27-web-design-system.md#shape-and-depth) checks.

## D128: One page frame for every page

**Chosen**: `ModeFrame`, `Page` and the bespoke headers (Snoozed, Owed, Reply queue, mailbox lists, Focus, Search) merge into one `PageFrame` with fixed slots: eyebrow (optional), title, one-line purpose, one primary action, an overflow menu, optional tabs, and the freshness line where the page has data that can be stale. Every page header is the same height at the same width. Spec in [27](27-web-design-system.md#the-page-frame).

**Considered**: Two frames, one for modes and one for tools (the current split, which makes "moving between modes and tools feel like moving between apps"); per-page headers designed for each job.

**Why**: The audit measured header heights of about 92, 103, 125 and 147px across screens and ranked three page frames third of fifteen problems. The 2026-10-14 pass found "every page title shown twice".

**Trade-offs accepted**: Some pages lose a bespoke flourish (Focus's large eyebrow, Search's oversized input).

## D129: One row anatomy, one reason line, and mode marks instead of sentences

**Chosen**: Every list row is built from one anatomy: lead (avatar, checkbox or source mark), primary line, one secondary line, one reason line ("Here because ...", shown on the lit row and in the reader, never on every row), and trail (time, counts, mode marks). Cross-mode presence ("Also in To do: ...") becomes a mode mark: a small glyph per other mode the item is in, with the detail on hover and on the lit row. The new-sender question ("New sender. Keep in Messages? Messages Updates Reading Block") becomes a single `new` mark on the row and a one-line choice on the lit row with its keys. Each mode keeps its own unit and verbs (D112); the anatomy fixes the slots, not the content.

**Considered**: Keeping each mode's bespoke row (about six designs today); underlined inline links (the audit's "Wikipedia article" look, doubling row height on Now and Messages); dropping cross-mode presence (it is the five-modes model's main promise, D108).

**Why**: BK: "The Now page is quite useful but it's pretty noisy and cluttered and a little overwhelming" (2026-10-09) and "Glancing at the todo should give me enough information to act if possible ... visible and glanceable clearly without being cluttered" (2026-10-09). The audit found five underlined links per People row on Now and "Here because" used in Reading, Updates and Archive but not in Now, Messages, To do or Inbox, "the screens where it matters most".

**Trade-offs accepted**: The reason is one keypress or one hover away on unlit rows. D111's "every placement shows its reason" is kept by the lit row and the reader, not by every row at once.

**Amended 2026-10-16 after the W1 comp.** The lit row's reason moves to the status bar (D124's amendment), so a reason can be a full sentence without truncating in a 480px list.

## D130: Keys are shown once, where you are

**Chosen**: Key hints appear in three places only: on the lit row's actions, in the status bar's context strip (the current screen's three to five main verbs), and in the `?` sheet and command palette. In-page legends under lists, chips inside every button and hover chips on unlit rows are removed. Touch layouts show no key chips. The `?` sheet loses developer notes (TUI parity remarks) and states each mode's purpose once.

**Considered**: Today's four formats on one screen; hiding keys entirely behind `?` (BK wants them visible: "I would lve for the keyboard shortcuts of teach of these actions to be visible", 2026-05-11).

**Why**: BK's philosophy is "everythign is fully navigable by keyboaard. including anything you want to do" (2026-05-11), and the hints must teach without competing with content (audit problem 6). Superhuman's command menu teaches each shortcut beside its command.

**Trade-offs accepted**: A less frequent action's key is visible only when its row is lit or in `?`.

## D131: One empty state, one loading state, one error state, written for people

**Chosen**: Each page frame has exactly one empty state (top-aligned in the content column, a one-line statement of what would be here and the one action that fills it), one loading state (a skeleton in the shape of that page's rows, never the generic mail skeleton), and one error state (what happened in plain words, what still works, and the one fix that can work; "Try again" only for errors a retry can fix). When the daemon is unreachable, cached content stays on screen under one banner that says so; the sidebar never changes its items. Copy for each lives with the daemon's copy where the daemon owns the words (CLI and TUI say the same).

**Considered**: Today's five empty layouts, the mail skeleton for every page and "Try again" with raw errors such as `invalid thread id: not-a-real-thread-id`.

**Why**: BK, 2026-04-22: "while it's loading there's no loader or anything showin that something is loading, it just looks like it's stuck". The audit found an Archive skeleton that never resolves, two stacked daemon-down banners (one blaming a protocol upgrade) and a sidebar that drops Starred, Sent, All Mail, Spam and Trash when offline.

**Trade-offs accepted**: Errors that today show raw text need a mapping to plain words; unknown errors still show the raw text under "Details".

## D132: The app never contradicts itself

**Chosen**: Every count and every claim about the user's mail comes from one daemon definition and is worded the same on every surface. "Waiting on you" means Messages' Your turn everywhere it appears, counted as the rows Messages lists (a person once, a group thread once) and split in one place into "people you write with" (the You owe lane) and "others who wrote to you" (all of New from people, first-time or repeat). Now's headline and People total, the rail's Messages badge and Messages' Your turn header show the total. Owed replies and Focus & reply are the people you write with and say where the others are ("3 others who wrote to you are in Messages"). The thread's relationship line says "you owe a reply" only when the thread is in You owe or New from people; threads outside the desk's window have no such line. Waiting on has its own empty sentence. Screener's page lists the set its badge counts. Units are always named ("49 conversations", "2 of 35 conversations").

**Considered**: Leaving each view its own definition and explaining the differences in copy.

**Why**: The audit's second-ranked problem: Now says "12 people waiting on you" while Focus says "Nobody is waiting on a reply from you" and Owed says "You don't owe anyone a reply"; Screener says 28 senders while its badge says 17. BK's trust anxiety (D119) is about exactly this: a sorted view must make its scope legible, and two views that disagree make neither trustworthy.

**Trade-offs accepted**: This is behaviour, not paint: it needs daemon work to share one query. Q3 in [31](31-design-open-questions.md) records the assumed definition.

## D133: Navigation recedes, and every destination has a place on every width

**Chosen**: The rail is dimmer than the page (Linear's rule: orientation recedes, the task stays in focus). It lists Now and the five modes, then Inbox, then three named groups replacing "More": Mail (Starred, Sent, Drafts, Snoozed, Reply later, All Mail, Spam, Trash), Views (Waiting on, Owed replies, Screener, Subscriptions, Invites, Deliveries) and Tools (Search, Analytics, Rules, Accounts, Activity, Jobs, Diagnostics, Settings), with Labels after. The collapsed rail shows the modes and Inbox only. On phones the tabs are Now, Messages, To do, Reading and More; More is a sheet listing Updates, Archive, Inbox and the three groups, and the active tab always matches the page (Updates and Archive light More, not Find). Settings has one navigation: the settings sections replace the page's content, not a second sidebar beside the app's.

**Considered**: The current junk-drawer "More" with 14 mixed items; six phone tabs (too narrow at 390px); keeping Find as the phone hub (two search fields on one screen).

**Why**: The audit: a 30-item rail whose More "mixes mail folders, views, tools and AI extracts", Settings, Screener, Snoozed, Drafts, Sent, Labels and Tools unreachable on a phone, and the Find tab lit on Updates, Archive, Inbox and threads. D109 already made the rail the modes; this finishes it.

**Trade-offs accepted**: Some items move one level deeper; every one stays on a `g` key and in the palette.

## D134: Motion explains change and the end of a section is the reward

**Chosen**: Motion has four jobs only: arriving (the reader page slides in from the right, 180ms, ease-out), leaving (a done row folds away in 120ms and the light moves to the next row in the same frame), landing (a change made elsewhere, by the assistant or another client, shows on its rows with a brief cyan underline that fades over 600ms) and finishing (the last item of a section put away brings the low-tide scene, 1800ms, once per section per day). Nothing moves on keyboard repeat, nothing animates layout properties, and reduced motion keeps fades only. Sound stays off by default (D101).

**Considered**: Celebrations on every completion (BK's 2026-09-27 example of "a little colorful animation with a satisfying ding"), which repeated triage would make noise; no motion at all.

**Why**: BK asked for micro-interactions that make the app "feel good" (2026-09-27), and on video said "things were also changing too fast". D101 already settled "delight is rare, earned and optional". The audit found motion "barely visible": the tokens exist and nothing uses them for these moments.

**Trade-offs accepted**: The finishing moment needs a definition of "section cleared" per mode, which [27](27-web-design-system.md#motion) gives.

## D135: The assistant is the copy desk: wire lines, the app's own objects, and proofs to sign

**Chosen**: The assistant panel is redesigned in [29](29-assistant-design.md). Each step is one wire line in mono: a verb that changes tense ("Searching your mail for invoices" then "Searched your mail: 14 found"), never a tool name or JSON; the daemon provides both tenses with the readout. While a turn runs, one live line rewrites itself with what it is doing now, replacing "Thinking". Emails and people in replies and readouts are drawn as the app's own rows and chips (avatar, name, subject), not underlined links. A change waiting for approval is a proof: what changes, in words, the items it touches as rows, and one apply with its key; a whole group is applied or sent back with a note. The TUI pane follows the same grammar.

**Considered**: Keeping the chat-app look (right-aligned bubbles, underlined links, a "Thinking" spinner); Marginalia (an editor's margin notes) and a Proof tray as the whole panel, from the research; per-item drop inside an approval (it would edit the stored call, which D122 forbids: "Approving runs exactly the call stored in the transcript").

**Why**: BK: "this is not a good ui" (the raw-JSON step), "you tools return json internally I kno but in the agent interface we can surely make it look nice" and "make the UI nice please . like the rest of the app" (2026-10-15). Linear's agent grammar (action, parameter, result, with present and past tense and ephemeral live steps) is the clearest published answer; Linear also draws links an agent writes as its own issue and person chips.

**Trade-offs accepted**: Verb phrases per tool are new daemon copy (72 tools); a tool without phrases falls back to its readout words. Q5 in [31](31-design-open-questions.md) asks whether a proof should allow dropping single items, which needs a protocol change.

## D136: Model text is always marked and never raw

**Chosen**: Text a model wrote (gists, summaries, assistant replies, draft suggestions) is set in the reading voice with a `model` mark naming where it ran (local or cloud), and its provenance is one click away. No default view shows JSON, ids, tool names or raw errors; they live under "Details".

**Considered**: Marking model text by colour alone (Granola's grey, which a critic called "gray on gray").

**Why**: D100's standard ("model text shown to a user is either a verified quote from the message or labelled as a summary with its provenance") already holds for gists; the audit found machine phrasing in list snippets ("Preview of: ...", "The conversation history shows ...") with no mark.

**Trade-offs accepted**: One more small mark on rows that show gists.

## D137: The app has its own design source, separate from the site's DESIGN.md

**Chosen**: Chapters 26 and 27 are the web app's design source. `apps/web/DESIGN.md` is a short pointer to them plus the token file, so tools that look for a DESIGN.md beside the code find the app's system, not the marketing site's. The root `DESIGN.md` stays the site's. `tokens.css`'s header stops saying it "follows DESIGN.md" and points at chapter 27.

**Considered**: Amending the root DESIGN.md to cover both (the two have different jobs and audiences).

**Why**: The audit and the 2026-10-14 pass both found the root DESIGN.md is "derived from `site/src/styles/custom.css`" and that little beyond palette and font reached the app.

**Trade-offs accepted**: Two design sources to keep consistent; the palette and font stay shared through `tokens.css`.

## D138: A rung that changes what people see passes a design gate, not only tests

**Chosen**: Every rung in [30](30-design-roadmap.md) that touches a screen follows the gate in [26](26-web-design-concept.md#the-design-gate): an HTML comp or prototype of its screens and moments, published and reviewed against the concept before building; after building, screenshots of each touched screen in Midnight, Light, Paper and Eclipse at 390, 1024 and 1440px, read against the concept and the hierarchy table; the key moments used by hand; and the main journeys walked end to end. A review agent grades it against [26](26-web-design-concept.md) and records the result in the rung's PR.

**Considered**: "Readable and not broken" screenshot checks, which the app has passed for months while BK called it unpolished.

**Why**: BK's lesson from keyloom (2026-10-16): "it's like if you describe functionality and someone just makes a thing that does exactly what you asked behaviourally and no more".

**Trade-offs accepted**: Each rung takes longer; the comps are throwaway.

## D139: Four themes from one generator, Midnight first

**Chosen**: Midnight stays the default, with Light, Paper and Eclipse. All four are generated from three inputs each (base, accent, contrast) in OKLCH, as Linear did, so a new theme is three values. Every theme is checked at the same widths in the design gate. Paper stops using its accent for unread and badges (they read as error red today). Editor themes (BK's April list: catppuccin, gruvbox, nightfox, kanagawa) wait for Q4.

**Considered**: Hand-tuned variables per theme (510 lines in `tokens.css` today); adding editor themes now.

**Why**: BK, 2026-10-07: "our dark mode is not great, I think there's lots of room for us to make it more delightful", then "what happened to the dark mode work I asked for?". A generator makes every theme get the same hierarchy for free.

**Trade-offs accepted**: The generator must reproduce today's Midnight closely enough that it doesn't feel like a different app; S3 checks it.

## D140: Toasts confirm and undo, one at a time, in one corner

**Chosen**: Toasts stay bottom-right (top-right in Focus, where the composer sits at the bottom), one at a time; a new toast replaces the old one. They say what happened and offer Undo when the change can be undone, auto-dismiss after 5 seconds (8 with Undo) and never cover the lit row. Errors that need a decision are not toasts.

**Considered**: Top-centre rich-colour toasts (BK's 2026-10-07 ask, reversed by BK on 2026-10-09: "The toasts are a bit jarring to be honest, and they appear on top of content and they don't go away. Maybe they should be top right or bottom right").

**Why**: BK's 2026-10-09 words and commit `4b4b52452` already moved them; this writes the rule down so new features don't invent their own feedback.

**Trade-offs accepted**: Bursts of changes show only the last toast; the activity log keeps them all.

## D141: No icon stands for AI

**Chosen**: A model feature is named for what it does, with a task icon if the slot needs one: `PenLine` to draft or write, `TextQuote` to summarize. The sparkle (lucide `Sparkles`, `Sparkle`, `WandSparkles`) and `BotMessageSquare` are not used anywhere, and neither is the sparkle emoji. Who wrote the output is shown in words by `ModelMark` (`model · local` or `model · cloud`, D136). The accent colour is not used to mark a model feature. `scripts/check-ai-marks.mjs` in `npm run lint` fails an import of those icons or a sparkle emoji in `apps/web/src`.

**Considered**: The sparkle, the industry default for "AI".

**Why**: BK, 2026-10-16: "the sparkly emoji/icon is really overdone for AI, it screams vibe coded". D136 already marks provenance in words, so an icon adds nothing a reader can use, and a shared symbol makes every product look the same.

**Trade-offs accepted**: A model feature is no longer recognisable at a glance from its icon, so its label and `ModelMark` have to carry it.

## D142: A thin Tauri desktop app for macOS ships, and the daemon stays the system

**Chosen**: `apps/desktop/` is a Tauri 2 shell that runs `mxr web --print-url --no-open`, loads the URL it prints in one window and registers a global new-message shortcut (Control+Option+Command+M). It bundles no frontend and no daemon, has no updater (Homebrew updates `mxr`) and shows a bundled down page when `mxr` is missing or `mxr web` fails. No page gets Tauri IPC; the window may only show the bridge origin and the down page, and other links open in the browser. macOS first; the unsigned build is D1 and signing, notarisation, the release and a Homebrew cask are D2.

**Considered**: Staying with the browser and the PWA (the 2026-05-12 decision, which has no D number: commits `261e5f599` and `da61537ae`, which removed the Electron app and wrote the "no native desktop app" guide). An Electron shell again.

**Why**: BK, 2026-10-16: "I don't think there's a huge cost to a tauri app, the backend is separate and is already there and working, the actual web app that we'd put in the web view is already there and working and mxr web can still work if you wan to open in the browser since it's the same app." The Electron app's cost came from bundling `mxr`, managing the daemon (`bridge-manager.ts`, 675 lines) and its own updater. The shell does none of the three, so what is left is a window, a shortcut and a down page.

**Trade-offs accepted**: A second artifact to build and, from D2, to sign and release. The shell depends on the `/compose/new` deep link, so that route is now a contract. Closing the window hides it rather than quitting, so the shortcut keeps working.

## D143: Web Archive acts at once; the safety record stays behind the scenes

**Chosen**: Pressing `e` (or the Archive button, swipe or hover action) in the web app archives with no dialog. The row folds and the view advances as before, and the toast offers Undo. Under that, the reviewed path is unchanged: preview (server review), retain the review in the local journal, submit bound to the reviewed selection, poll the outcome. An incomplete outcome still reports the recovery error and points to Archive recovery. While an Archive of a message is in flight, a second Archive of it does nothing, so `u` cannot be taken by a duplicate.

**Considered**: The browser `window.confirm` listing the reviewed message ids that #384 (`0153a25a9`) added before every submit. A richer in-app confirmation with subject and sender.

**Why**: BK, 2026-10-21: "No pop-up: e archives at once and the toast offers Undo, as the design chapters describe. The safety record is still kept behind the scenes, so recovery still works." Chapters 26 and 27 make keyboard archive instant, and a native dialog of UUIDs on every `e` contradicted them. The review still binds the submission, and the journal still makes an unknown outcome recoverable, so the dialog added friction rather than safety.

**Trade-offs accepted**: There is no pre-submit cancel in the web app; Undo and Archive recovery are the ways back. This supersedes the browser confirmation from #384 and retires the follow-ups about cancelling review, message details in the confirmation and a second list Archive.

## D144: Five settings that never did anything are retired, not built

**Chosen**: `appearance.sidebar`, `appearance.date_format`, `appearance.date_format_full`, `appearance.subject_max_width` and `humanizer.apply_to_summaries` are removed from the config structs, defaults, `mxr config get` and `mxr config set`. Config files that still set them keep loading. The loader warns once per process, naming the keys with "ignored: no longer used", and `config get` or `config set` on one returns an error that says it was retired. `appearance.theme` stays.

**Considered**: Building the settings: a sidebar hide toggle, strftime date formats for the list and the reader, a subject width cap, and summary humanizing once summaries exist.

**Why**: BK decided on 2026-10-21 to retire these five settings rather than build them. The TUI already formats dates by age (`ui/messages_lens.rs`), sizes columns to the terminal, has no sidebar hide feature, and nothing produces summaries to humanize. Each setting that claims to work and does not is a broken promise, and building them adds maintenance with no user asking.

**Trade-offs accepted**: Users who set a retired key get a warning, not a fix. A hide-sidebar or date-format request now starts from a new decision, not from these keys.
||||||| parent of 65602220e (docs: document the assistant model picker and the OpenAI key (D144))
||||||| parent of 53fad1ff6 (docs: document the assistant model picker and the OpenAI key (D143))

## D145: The assistant's model is one setting, picked from a list, local or hosted

**Chosen**: The assistant runs on one model for every conversation, chosen from a list the user can edit (`llm.assistant_models`, default `gemma4:latest`, `gpt-6-luna`, `gpt-5.6-terra`, `claude-haiku-5-5`, `claude-sonnet-5-5`). A pick is stored as `[llm.overrides.assistant] model`. OpenAI becomes a stored-key route provider beside Anthropic and OpenRouter (`mxr llm key set openai`, `OPENAI_API_KEY`): `gpt-<number>...` ids go to OpenAI first and to OpenRouter as `openai/<id>` second, unless `llm.base_url` is a remote server of the user's own. OpenAI's requests carry `max_completion_tokens` and no temperature. The picker is in the web composer's model mark, `M` in the TUI pane, and `mxr assistant model [set <id>]`, all over `GetAssistantModels` and `SetAssistantModel`. A model whose key is missing, or that is off this machine while `llm.allow_cloud_relationship_data` is off, is listed as unavailable with the command or setting that fixes it, and picking it is refused; mxr never turns the setting on. A reply keeps the model and place that wrote it (`leaves_machine` in the message meta).

**Considered**: A model per conversation. An unrestricted free-text model field only (what Settings > LLM already had). Sending `gpt-*` through the single OpenAI-compatible endpoint, which would need the user to repoint `base_url` for every switch. Turning the cloud setting on when a cloud model is picked.

**Why**: BK, 2026-10-21: "I want to have a model picker so that I can pick between local with gemma4 or cloud with gpt-6-luna or claude haiku 5.5 or claude sonnet 5.5 or gpt-6-terra". His local gemma4 writes weak replies, so switching has to be one action, not a config edit. There is no `gpt-6-terra`; asked, BK chose `gpt-5.6-terra` in its place. One model for all conversations keeps the footer mark, the cloud gate and `mxr assistant status` describing one thing. The cloud gate stays because the assistant reads across the mailbox.

**Trade-offs accepted**: Switching mid-conversation sends the old turns to the new provider rebuilt from text and tool calls, so Claude's thinking blocks are replayed only to Claude. A `gpt-<number>` id on the default Ollama or another server on this machine now goes to OpenAI instead of that server; a remote `llm.base_url` keeps it, and local names like `gpt-oss` never match. The default list names models that may not exist on a given account, so an unavailable or failing model shows as such rather than being hidden.

## D146: Unsubscribe receipts are separate from history

**Chosen**: Each unsubscribe claims its message in `unsubscribe_receipts` before anything is sent: an insert with method `pending`. A request that finds a claim or receipt sends nothing and acknowledges. The send then either confirms the claim (the method and time of the send) or releases it, and it releases only when the send certainly did not happen: connection refused or DNS failure before the request, a 4xx/5xx response, or a provider's `NotAccepted`. Any other outcome (a timeout or reset after the request, or a provider's `Unknown`) keeps the claim, marked `<method>_unknown`, and the error says mxr can't tell whether the unsubscribe went through and won't send it again. Purging event_log never touches the table. Deleting a message deletes its receipt, and the table references messages with ON DELETE CASCADE, so deleting an account removes its receipts too. Rows that existed before the migration are backfilled from the event_log entries the old check matched, for messages that still exist, with method `unknown`.

**Considered**: Keep the event_log check and stop `mxr logs --purge` from deleting unsubscribe rows. Keep the receipt in the event_log row with a longer retention for that category.

**Why**: Purging event_log made mxr send the same unsubscribe mail or POST a second time. The promise is that mxr sends an unsubscribe for a message once, whatever happens to logs and history. A purge that deletes history must not also delete the guard.

**Trade-offs accepted**: A small table that is never pruned by purge, and grows by one row per unsubscribe. A receipt is removed with its message, so a message that is deleted and later synced back under the same id can be unsubscribed again. An unsubscribe whose outcome mxr can't tell is never re-sent, so if the sender did not act, the user has to unsubscribe by other means; the error says so. A daemon that stops between the claim and the send leaves the claim in place, which reads as unknown and blocks a resend. If the receipt cannot be recorded after a send, the error is not retryable, and a retry acknowledges without sending.

## D147: Each account has one identity, a colour and a tag, owned by the daemon

**Chosen**: Every account has a colour, one of eight named slots, and a tag of one to three characters. The daemon assigns the colour once, when the account is created, and keeps it; it derives the tag from the account's name. Both can be changed in config. Clients receive both on `AccountSummaryData.identity` and draw one mark, a coloured chip holding the tag, at the trail of every row (before the time), but only while the view mixes accounts. The reader header and compose From always show the account. The palette is reserved for accounts. Chapter 33 is the design.

**Considered**: A coloured stripe on the row's leading edge (Spark, K-9), which collides with the unread bar and the lit row's hairline. Colour alone, which fails WCAG 1.4.1 and fails in `NO_COLOR` terminals. A colour hashed from the account id, which can't be chosen and which changes meaning when someone renames or re-adds an account. Colouring the sender avatar, which already carries the chart colours and would make people read a colour as a person. Showing the mark with one account, which adds noise to every row for no information.

**Why**: BK, 2026-10-22: "when i look at the emails in the different modes and all the different views in the app, you need to add some kind of color coding or something to show which account each email belongs to so that it's easily glanceable ... integrate it well into the architecture of the app". He approved the comp as drawn. Prior art (Mimestream, Spark, Thunderbird, K-9) shows a per-account colour the user can pick, reused across surfaces and hidden for a single account. The user complaints in that research were about marks that were too small, hard to find, or missing on some surfaces. The daemon owns the identity so the web app, the TUI and the CLI can't drift apart.

**Trade-offs accepted**: A ninth account repeats a colour, and its tag tells it apart. The chip takes about 28px of every marked row's trail. An older daemon sends no identity, so a new client draws no mark until the daemon is updated.

## D148: An open email always shows when it arrived, in plain sight

**Chosen**: Every view where an email is open shows its arrival as an absolute local date and time (`Tue 21 Oct, 14:02`, the year when it isn't this one, "· 2h ago" for today's), in meta type, with no hover or click. Lists keep their compact time, and the lit row no longer hides it. The TUI reader shows local time. Chapter 27, "When an email arrived", is the rule.

**Considered**: Relative times alone ("Mon", "Sep 3"), which is what the reader showed: they can't say which Monday, or what time. The full stamp in a tooltip only, which is what hid it. A large date line, which BK ruled out.

**Why**: BK, 2026-10-22: "sometimes I open an email and I really struggle to figure out when exactly an email was received because some views don't have a date/time shown clearly somewhere on the view. I think this is important information everytime for the way I use email. I'm not asking for it to be a huge text but I want to be able to see it somewhere easily, I don't want to click a button or do something, it should be there." An audit of origin/main 8c05bcb06 found no reader on either client that met that: the web reader showed "Mon" with the date in a tooltip, the Messages conversation never showed a date, Updates showed no time at all, and the TUI reader printed UTC.

**Trade-offs accepted**: Message headers grow by about ten characters. The lit row's actions sit beside the time instead of over it, which leaves less room for actions on narrow screens; the overflow menu takes the rest.

