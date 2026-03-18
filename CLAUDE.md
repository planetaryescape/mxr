# mxr — Project Context for AI Agents

> A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

## Language & Stack

- **Language**: Rust (edition 2021)
- **Async runtime**: Tokio
- **Database**: SQLite via sqlx (two-pool: single writer + concurrent reader pool, WAL mode)
- **Search**: Tantivy (BM25, field boosts, progressive indexing)
- **TUI**: Ratatui + crossterm
- **Email parsing**: Stalwart mail-parser
- **SMTP**: Lettre
- **Gmail**: Direct REST API via reqwest + yup-oauth2
- **IMAP**: async-imap (first-party adapter, CONDSTORE/QRESYNC + UID fallback + IDLE)
- **IPC**: Length-delimited JSON over Unix domain socket

## Architecture

Daemon-backed. The daemon is the system. TUI and CLI are clients connected via Unix socket.

```
TUI / CLI / Scripts  <--- Unix socket (JSON) --->  Daemon
                                                      |
                                         Store (SQLite) + Search (Tantivy)
                                                      |
                                              Providers (Gmail, SMTP, Fake)
```

Single unified binary: `mxr` with subcommands (`mxr` = TUI, `mxr daemon`, `mxr sync`, `mxr search`, etc.).

## Core Principles (NON-NEGOTIABLE)

1. **Local-first**: SQLite is the canonical state store. Search index is rebuildable from SQLite. Works offline.
2. **Provider-agnostic internal model**: All app logic speaks one language (the mxr internal model). No provider-specific concepts leak into core code.
3. **Daemon-backed architecture**: TUI is a client of the daemon, not the system itself. Background sync, indexing, and rules run regardless of TUI state.
4. **$EDITOR for writing**: Compose opens $EDITOR with markdown + YAML frontmatter. mxr does not compete with your text editor.
5. **Fast search is first-class**: Tantivy BM25 with field boosts. Search is navigation, not an afterthought.
6. **Saved searches are a core primitive**: User-programmed inbox lenses in sidebar and command palette.
7. **Rules engine is deterministic first**: Rules are data (inspectable, replayable, idempotent, dry-runnable). Scripts are escape hatches, not the foundation.
8. **Shell hooks over premature plugin systems**: Pipe data to shell commands. Unix composition over framework lock-in.
9. **Adapters are swappable**: No provider-specific logic outside adapter crates. Ever.
10. **Correctness beats cleverness**: Plain, legible Rust. Compile-time checked SQL. Explicit error types. When in doubt, be boring.

## Data Model Design Philosophy

The internal model is the most important design decision in mxr. All application logic speaks this language. Gmail and IMAP (and any future provider) map INTO this model. The model never bends to accommodate provider quirks — that's the adapter's job.

### Key principles

1. **Provider-agnostic**: No Gmail-specific or IMAP-specific concepts in the core types.
2. **Correctness over cleverness**: We store enough data to round-trip back to the provider without loss.
3. **Lazy hydration**: Envelopes (headers/metadata) are always cached. Bodies are fetched on demand and cached after first access. This keeps sync fast and storage manageable.
4. **Typed IDs**: Newtypes prevent mixing up account IDs with message IDs at compile time.
5. **Time-sortable IDs**: UUIDv7 gives naturally ordered primary keys.

### Crate dependency rules

These are strict. Violations should be caught in code review:

1. **`core` depends on nothing internal.** It is the leaf node. All other crates depend on it.
2. **`protocol` depends only on `core`.** It defines the IPC contract between daemon and clients.
3. **Provider crates depend only on `core`.** They implement traits defined in core. They do NOT depend on store, search, or sync. This is what makes them swappable and independently buildable.
4. **`store` and `search` depend only on `core`.** They are storage backends, not business logic.
5. **`sync` depends on `core`, `store`, `search`.** It orchestrates data flow between providers and local state.
6. **`daemon` is the integration point.** It depends on most crates. This is expected and acceptable — it's the application entry point.
7. **`tui` depends only on `core` and `protocol`.** It talks to the daemon via IPC, never directly to providers, store, or search. This enforces the client-server boundary.

## Non-negotiables for Contributors

- Local-first by default
- SQLite is the canonical state store
- Search index is rebuildable from SQLite
- Provider adapters are replaceable
- No provider-specific logic outside adapter crates
- Compose uses $EDITOR
- Core features do not depend on proprietary services
- Rules are deterministic before they are intelligent
- TUI is a client of the daemon, not the system itself
- Distraction-free rendering: plain text first, reader mode, no inline images

## Key Design Decisions (settled, do not re-debate)

See `docs/blueprint/15-decision-log.md` for full context. Highlights:

- **Rust over Go** (D001): Tantivy, ratatui, ecosystem fit
- **Daemon over monolithic** (D002): background sync, multiple clients, headless operation
- **sqlx over ORMs** (D003): async, compile-time checked, explicit SQL
- **Tantivy over FTS5** (D004): BM25 with field boosts, faceted search, scales to 100k+
- **Direct Gmail API over gws CLI** (D006): delta sync via history.list, no external deps
- **Split MailSyncProvider / MailSendProvider traits** (D007): SMTP can't sync
- **YAML frontmatter for compose** (D009): Hugo/Obsidian pattern, widely known
- **Plain text first rendering** (D010): distraction-free is a feature
- **JSON over Unix socket for IPC** (D017): simple, debuggable, fast enough
- **Keybinding hierarchy** (D035): vim-native for navigation, Gmail for email actions, custom last
- **IMAP first-party** (D048): overrides D015, IMAP is now official adapter in Phase 2
- **Every TUI action has CLI equivalent** (D026): full scriptability via CLI flags + `--search` for batch

## Documentation

- `docs/blueprint/` — What to build (requirements, design, decisions)
- `docs/blueprint/16-addendum.md` — Post-blueprint amendments A001-A008 (inline compose, full CLI surface, vim+Gmail keybindings, observability, batch ops, IMAP)
- `docs/implementation/` — How to build it (step-by-step implementation plans per phase)

## Workspace Layout

```
crates/
  core/           # Types, traits, errors (leaf node)
  store/          # SQLite persistence via sqlx
  search/         # Tantivy indexing and query
  protocol/       # IPC types (Request, Response, Command)
  providers/
    gmail/        # Gmail API adapter (first-party)
    imap/         # IMAP adapter (first-party, Phase 2)
    smtp/         # SMTP send adapter
    fake/         # In-memory test provider
  sync/           # Sync engine (providers <-> store <-> search)
  compose/        # $EDITOR workflow, frontmatter, markdown->multipart
  reader/         # Reader mode (HTML->text, signature/quote stripping)
  rules/          # Deterministic rules engine
  export/         # Thread export (markdown, JSON, mbox, LLM context)
  daemon/         # Background process, socket server
  tui/            # Ratatui frontend
  cli/            # CLI subcommand dispatch
```
