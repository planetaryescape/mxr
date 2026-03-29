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

**Chosen**: Labels as the unified organizer surface in the app model, with explicit honesty seams for folder-backed providers (`LabelKind::Folder`, provider IDs, `SyncCapabilities.labels == false`).

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
