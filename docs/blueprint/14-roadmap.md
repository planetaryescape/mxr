# mxr — Roadmap

## Guiding principle

Each phase should produce something usable. Don't build infrastructure for future features — build features that work.

## Phase 0 — Prove the Architecture (weeks 1-3)

### Goal
Prove the bones. Daemon runs, TUI connects, data flows through the system end-to-end.

### Deliverables

- [ ] **Cargo workspace** with all crate scaffolding
- [ ] **`mxr-core`**: All types from 02-data-model.md implemented (Envelope, Label, Thread, Draft, SavedSearch, MessageFlags, typed IDs, provider traits, error types)
- [ ] **`mxr-store`**: SQLite setup with sqlx, initial migration (full schema from 02-data-model.md), basic CRUD queries (insert/query accounts, messages, labels, bodies, drafts)
- [ ] **`mxr-protocol`**: Request/Response/Command enums for IPC
- [ ] **`mxr-provider-fake`**: In-memory provider with pre-loaded fixture messages (50+ messages, 10+ threads, multiple labels). Implements both MailSyncProvider and MailSendProvider.
- [ ] **`mxr-daemon`**: Socket server skeleton (listen on Unix socket, parse JSON commands, dispatch to handlers, return JSON responses). Sync loop running against fake provider. Snooze wake loop (can be tested with fake data).
- [ ] **`mxr-search`**: Tantivy index creation, schema from 05-search.md, basic indexing (insert documents), simple text query + field query
- [ ] **`mxr-tui`**: Basic ratatui shell with two-pane layout (sidebar + message list). Vim navigation (j/k/gg/G/Ctrl-d/Ctrl-u). Connects to daemon via socket. Displays fake provider messages.
- [ ] **`docs/architecture.md`**: This blueprint (or a summary of it) committed to repo
- [ ] **CI**: GitHub Actions for fmt, clippy, test, build
- [ ] **README.md** with project description and setup instructions

### NOT in Phase 0

- Real Gmail adapter (no network calls yet)
- SMTP sending
- Compose flow
- Rules engine
- Reader mode
- Real search query parser
- Keybinding configuration
- Config file parsing (use hardcoded defaults)

### Definition of done

You can run `mxr` and see a TUI displaying fake email data. You can navigate with j/k, open a message, and see its content. The daemon is running in the background serving this data from SQLite. Tantivy has indexed the fake messages and a basic search works.

## Phase 1 — Gmail Read-Only + Search (weeks 4-7)

### Goal
Read real email from Gmail. Search actually works.

### Deliverables

- [ ] **`mxr-provider-gmail`**: OAuth2 flow (browser redirect, localhost callback, token storage in keyring). Message listing. Delta sync via history.list. Full message fetch. Label listing. List-Unsubscribe header parsing.
- [ ] **`mxr-sync`**: Sync engine orchestrating Gmail provider ↔ store ↔ search. Initial sync (full mailbox). Delta sync (incremental). Envelope + body fetch during sync. Progressive sync (messages appear in TUI as they arrive).
- [ ] **Search query parser**: Support for: text, "phrases", field:value (from:, to:, subject:, label:, is:, has:), boolean (AND, OR, NOT), date ranges (after:, before:, date:today). Saved search CRUD.
- [ ] **TUI enhancements**: Three-pane layout (sidebar + list + message view). Thread view. Search input (/ keybinding). Command palette (Ctrl-P) with nucleo fuzzy matching. Saved searches in sidebar and palette. Sync status in status bar. Label list in sidebar with unread counts.
- [ ] **Config file**: TOML parsing for accounts, general settings, render settings. XDG paths.
- [ ] **`mxr accounts add gmail`**: Interactive CLI setup flow for Gmail.
- [ ] **`mxr search`**: CLI search command.
- [ ] **`mxr sync`**: CLI sync command.
- [ ] **`mxr doctor`**: Basic diagnostics (config validation, auth status, last sync time).

### NOT in Phase 1

- Compose/send (read-only)
- SMTP
- Mutations (archive, trash, star, label)
- Reader mode
- Unsubscribe action
- Snooze
- Rules engine
- Export
- Keybinding customization

### Definition of done

You can add your Gmail account, sync your inbox, browse messages in the TUI, read message bodies, search with the query syntax, create saved searches, and use the command palette. All from real Gmail data.

## Phase 2 — Compose + Mutations + Reader Mode (weeks 8-11)

### Goal
Full read-write email client. You can use mxr as your primary email client.

### Deliverables

- [ ] **`mxr-compose`**: $EDITOR compose flow. YAML frontmatter parsing. Context block for replies (with reader-mode-cleaned thread content). Draft persistence in SQLite. Markdown to multipart rendering (comrak for HTML, raw markdown for plain text). Attachment handling via frontmatter.
- [ ] **`mxr-provider-smtp`**: SMTP send via lettre. Config from accounts TOML.
- [ ] **Gmail send**: Send via Gmail API (alternative to SMTP).
- [ ] **Gmail mutations**: Archive (remove INBOX label). Trash. Star/unstar. Mark read/unread. Apply/remove labels.
- [ ] **`mxr-reader`**: Reader mode pipeline. HTML → plain text. Signature stripping. Quote collapsing. Boilerplate removal. ReaderOutput struct with stats.
- [ ] **Unsubscribe**: Parse List-Unsubscribe header (already done in Phase 1 sync). `U` keybinding with confirmation. OneClick POST, Mailto auto-send, HttpLink browser open.
- [ ] **Snooze**: Local snooze with Gmail archive integration. `Z` keybinding with time picker. Wake loop in daemon. Inbox restoration on both mxr and Gmail.
- [ ] **TUI enhancements**: Reader mode toggle (`R`). Unsubscribe indicator in message list. Snooze menu. Compose/reply/forward keybindings. Draft list. Attachment download + open.
- [ ] **CLI**: `mxr compose`, `mxr reply`, `mxr forward`, `mxr drafts`, `mxr send`.
- [ ] **Keybinding config**: `keys.toml` parsing and custom keybinding support.

### Definition of done

You can read, compose, reply, forward, archive, trash, star, search, snooze, and unsubscribe. Reader mode is on by default. You can use mxr as your daily email client for a Gmail account.

## Phase 3 — Export + Rules + Polish (weeks 12-15)

### Goal
mxr becomes a productivity platform, not just a client.

### Deliverables

- [ ] **`mxr-export`**: Thread export in Markdown, JSON, Mbox, LLM Context formats. TUI export keybinding (`e`). CLI `mxr export`.
- [ ] **`mxr-rules`**: Declarative rules engine. Conditions + Actions as data. TOML rule definitions. Dry-run mode. Rule evaluation on sync. Execution logging.
- [ ] **Shell hooks**: ShellHook action type. Message JSON piped to stdin.
- [ ] **Multi-account**: Support for multiple Gmail accounts + SMTP configs. Account switcher in TUI and command palette.
- [ ] **Hybrid search baseline**: Local semantic retrieval with English default profile (`bge-small-en-v1.5`), opt-in multilingual profile (`multilingual-e5-small`), optional advanced profile (`bge-m3`), and RRF fusion with Tantivy BM25.
- [ ] **Semantic operations**: `mxr semantic ...`, `mxr doctor --semantic-status`, `mxr doctor --reindex-semantic`, saved searches with per-search mode, TUI mode toggle/status.
- [ ] **HTML rendering config**: External html_command support (w3m, lynx).
- [ ] **`mxr doctor --reindex`**: Full Tantivy reindex from SQLite.
- [ ] **Shell completions**: bash, zsh, fish.
- [ ] **Performance**: Optimize for large mailboxes (10k+ messages). Profile and fix bottlenecks.
- [ ] **Error UX**: Better error messages throughout. Auth expiry handling. Network failure recovery.

### Definition of done

You can export threads for AI, define rules that auto-organize your inbox, and switch between lexical, hybrid, and semantic search without giving up the local-first architecture. Multi-account works. The client handles large mailboxes smoothly.

## Phase 4 — Community & Polish (weeks 16-20)

### Goal
Ready for public release.

### Deliverables

- [ ] **Adapter kit**: Conformance test suite. Fixture data. "How to build an adapter" documentation.
- [ ] **CONTRIBUTING.md**: Full contributor guide.
- [ ] **Binary releases**: Pre-built binaries for Linux (x86_64, aarch64) and macOS (x86_64, aarch64).
- [ ] **Install methods**: cargo install, homebrew formula, AUR package.
- [ ] **Documentation site**: User guide, configuration reference, adapter guide.
- [ ] **README**: Screenshots/GIFs, install instructions, quick start.
- [ ] **Announcement**: Blog post, Hacker News, r/rust, r/commandline.

## Future (post-v1.0)

These are explicitly NOT on the roadmap for v1. They may grow out of the core later:

- **Cloud embedding backends**: Optional remote providers behind the same semantic profile abstraction
- **Richer attachment extraction**: PDF/OCR/office-doc text extraction for semantic chunks
- **Scripting runtime**: Embedded Lua or Rhai for in-process automation
- **Notifications**: Desktop notifications via notify-rust, terminal bell
- **Conditional snooze**: "Snooze until reply from X" via rules engine
- **Contact enrichment / CRM**: Auto-enrich sender profiles
- **Audio summaries**: TTS for newsletter content
- **Todo extraction**: Parse action items from emails
- **Web dashboard**: Alternate frontend connecting to the same daemon
- **IMAP adapter**: First community adapter candidate
- **Encryption**: PGP integration with good UX (not homebrew crypto)
- **Calendar integration**: Parse .ics attachments, show event context
