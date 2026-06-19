# mxr — Roadmap

> **Status note:** This file is now a historical roadmap plus a small current
> backlog, not a live Phase 0-4 task list. Phases 0-3 shipped or were
> superseded by the current code/docs. Unchecked boxes are reserved for the
> current backlog below. See [docs/implementation-journey.md](../implementation-journey.md)
> for the longer delivery and supersession record.

## Guiding principle

Each phase should produce something usable. Don't build infrastructure for future features — build features that work.

## Phase 0 — Prove the Architecture (weeks 1-3)

> **Historical status:** Shipped. Older names such as `mxr-core` refer to the
> current workspace crates under `crates/*`, with the repo-root `mxr` package
> as the install surface.

### Goal
Prove the bones. Daemon runs, TUI connects, data flows through the system end-to-end.

### Deliverables

- [x] **Cargo workspace** with all crate scaffolding
- [x] **`mxr-core` / `crates/core`**: Data-model types, typed IDs, provider traits, and error types
- [x] **`mxr-store` / `crates/store`**: SQLite setup with sqlx, migrations, and CRUD surfaces for accounts, messages, labels, bodies, drafts, and later tables
- [x] **`mxr-protocol` / `crates/protocol`**: IPC request/response/event contract
- [x] **`mxr-provider-fake` / `crates/provider-fake`**: Deterministic fake provider, canonical fixtures, and conformance helpers
- [x] **`mxr-daemon` / root `mxr` package**: Unix-socket daemon, JSON IPC dispatch, sync loop, wake loops, and client-facing handlers
- [x] **`mxr-search` / `crates/search`**: Tantivy schema, indexing, field queries, and rebuildable index path
- [x] **`mxr-tui` / `crates/tui`**: Ratatui client connected to daemon IPC; the current TUI supersedes the early two-pane shell
- [x] **Architecture docs**: `ARCHITECTURE.md` plus `docs/blueprint/`
- [x] **CI**: GitHub Actions for fmt/clippy/test/build/docs-adjacent checks
- [x] **README.md** with project description and setup/install instructions

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

> **Historical status:** Shipped. Some command names changed while preserving
> the product intent; saved-search CRUD lives under `mxr saved ...`.

### Goal
Read real email from Gmail. Search actually works.

### Deliverables

- [x] **`mxr-provider-gmail` / `crates/provider-gmail`**: OAuth2 flow, keychain-backed token storage, message/label listing, delta sync via Gmail history, body fetch, and List-Unsubscribe parsing
- [x] **`mxr-sync` / `crates/sync`**: Sync engine orchestrating provider, store, and search; initial and delta sync; envelope/body fetch; lexical freshness during sync
- [x] **Search query parser**: Text, phrases, field queries, boolean operators, date ranges, Gmail-style filters, and saved-search CRUD via `mxr saved`
- [x] **TUI enhancements**: Three-pane/current mailbox UI, thread/message views, search input, command palette, saved-search navigation, sync status, and label/sidebar counts
- [x] **Config file**: TOML parsing for accounts and settings with XDG/runtime paths
- [x] **Gmail account setup**: `mxr setup` and account-management flows supersede the early `mxr accounts add gmail` wording
- [x] **`mxr search`**: CLI search command
- [x] **`mxr sync`**: CLI sync command
- [x] **`mxr doctor`**: Diagnostics for config, auth/sync health, search/semantic status, and remediation

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

> **Historical status:** Shipped. IMAP also became first-party during this
> era; see `docs/implementation-journey.md` and `docs/blueprint/16-addendum.md`.

### Goal
Full read-write email client. You can use mxr as your primary email client.

### Deliverables

- [x] **`mxr-compose` / `crates/compose`**: `$EDITOR` compose flow, YAML frontmatter, reply context, draft persistence, markdown/multipart rendering, and attachment handling
- [x] **`mxr-provider-smtp` / `crates/provider-smtp`**: SMTP send via lettre and account config
- [x] **Gmail send**: Send via Gmail provider path
- [x] **Gmail mutations**: Archive, trash, star/unstar, mark read/unread, and apply/remove labels through daemon/CLI/TUI surfaces
- [x] **`mxr-reader` / `crates/reader`**: HTML-to-text reader pipeline, signature stripping, quote collapsing, boilerplate/tracking cleanup, and `ReaderOutput`
- [x] **Unsubscribe**: List-Unsubscribe parsing plus CLI/daemon/TUI flows. The early `U` keybinding text is superseded; the current Gmail-style TUI map uses `D` for unsubscribe and `U` for mark-unread paths.
- [x] **Snooze**: Local snooze, archive/inbox restoration flow, wake loop, CLI, and TUI affordances
- [x] **TUI enhancements**: Reader toggle, unsubscribe indicators/modals, snooze menu, compose/reply/forward flows, draft navigation, and attachment list/download/open
- [x] **CLI**: `mxr compose`, `mxr reply`, `mxr forward`, `mxr drafts`, `mxr send`, plus related read/write commands
- [x] **Keybinding config**: `keys.toml` parsing, default Gmail/vim-style maps, help display, and command-palette integration

### Definition of done

You can read, compose, reply, forward, archive, trash, star, search, snooze, and unsubscribe. Reader mode is on by default. You can use mxr as your daily email client for a Gmail account.

## Phase 3 — Export + Rules + Polish (weeks 12-15)

> **Historical status:** Shipped or superseded by current operational surfaces.
> Future performance/error work should be tracked as concrete bugs or profiles,
> not as a generic Phase 3 checkbox.

### Goal
mxr becomes a productivity platform, not just a client.

### Deliverables

- [x] **`mxr-export` / `crates/export`**: Thread/search export in Markdown, JSON, Mbox, and LLM-context formats; TUI export and CLI `mxr export`
- [x] **`mxr-rules` / `crates/rules`**: Declarative rules engine, data conditions/actions, dry-run mode, sync-time execution, and execution history
- [x] **Shell hooks**: ShellHook action type with structured message payloads
- [x] **Multi-account**: Multiple account configs/providers, default-account/account-management surfaces, and TUI account flows. The original "account switcher in palette" wording is superseded by the current account UX.
- [x] **Hybrid search baseline**: Local semantic retrieval profiles, optional dense recall, and RRF fusion with Tantivy BM25
- [x] **Semantic operations**: `mxr semantic ...`, `mxr doctor --semantic-status`, `mxr doctor --reindex-semantic`, saved-search mode selection, and TUI semantic actions
- [x] **HTML rendering config**: External `render.html_command` support such as w3m/lynx with built-in fallback
- [x] **`mxr doctor --reindex`**: Full Tantivy reindex from SQLite
- [x] **Shell completions**: bash, zsh, and fish generation
- [x] **Performance baseline**: Search indexes, sync batching, daemon lanes, and rebuild paths are live; future tuning belongs to specific measured issues
- [x] **Error UX baseline**: `mxr doctor`, `mxr status`, `mxr logs`, events, and remediation copy are live; future improvements belong to specific support/bug work

### Definition of done

You can export threads for AI, define rules that auto-organize your inbox, and switch between lexical, hybrid, and semantic search without giving up the local-first architecture. Multi-account works. The client handles large mailboxes smoothly.

## Phase 4 — Community & Polish (weeks 16-20)

> **Current state note (`v0.6.1`)**
> Public release is already live. `brew install planetaryescape/mxr/mxr`, `cargo install --git https://github.com/planetaryescape/mxr --locked mxr`, and GitHub release tarballs are working. Current binary targets are macOS Apple Silicon and Linux x86_64; Intel macOS and Linux aarch64 release artifacts are intentionally not built. Treat the checklist below as shipped release/community baseline plus scoped backlog references, not as a claim that mxr is still unreleased.

### Goal
Ready for public release.

### Deliverables

- [x] **Adapter kit**: Conformance helpers, fixture data, adapter skeleton, and adapter/conformance documentation
- [x] **CONTRIBUTING.md**: Contributor guide exists; keep expanding it through normal contributor-feedback work
- [x] **Binary releases**: Pre-built binaries for Linux x86_64 and macOS Apple Silicon; Linux aarch64 and Intel macOS remain future packaging work
- [x] **Install methods (current)**: `cargo install --git`, `cargo install --path`, Homebrew formula/tap, and GitHub release tarballs
- [x] **Documentation site**: Astro/Starlight user docs, configuration/reference pages, adapter guide, and API explorer under `site/`
- [x] **README**: Install instructions, quick start, supported surfaces, and release guidance
- [x] **Public release baseline**: Release/install/docs are live; optional launch/community posts are backlog, not a release blocker

## Current Backlog

These are open items after the public release baseline. They are separated from
the historical Phase 0-4 checklists so future work is visible without making
shipped phases look unfinished.

- [ ] **Arch/Nix packaging**: AUR and Nix packaging remain planned packaging work; Homebrew/git/release tarballs are already shipped.
- [ ] **Additional binary targets**: Linux aarch64 and Intel macOS artifacts are future packaging work only if demand justifies the maintenance cost.
- [ ] **Community launch assets**: Blog/Hacker News/reddit-style announcements are optional community/marketing work, not a product release gate.
- [ ] **Adapter ecosystem expansion**: The adapter kit exists; new community adapters or MSP/reference-adapter work should be validated against real maintainer/user demand before adding maintenance surface.
- [ ] **Measured polish work**: Performance and error-UX improvements should enter as specific profiles, bugs, or support observations, not generic "make it better" roadmap boxes.

## Future (post-v1.0)

These are explicitly NOT on the roadmap for v1. They may grow out of the core later:

- **Cloud embedding backends**: Optional remote providers behind the same semantic profile abstraction
- **Richer attachment extraction**: office-doc text extraction and deeper structure-preserving parsing for semantic chunks
- **Scripting runtime**: Embedded Lua or Rhai for in-process automation
- **Notifications**: Desktop notifications via notify-rust, terminal bell
- **Conditional snooze**: "Snooze until reply from X" via rules engine
- **Contact enrichment / CRM**: Auto-enrich sender profiles
- **Audio summaries**: TTS for newsletter content
- **Todo extraction**: Parse action items from emails
- **Additional web dashboards**: Surfaces beyond the first-party local SPA/bridge
- **Additional community adapters**: JMAP/Fastmail/Apple Mail/etc. after real demand appears
- **Encryption**: PGP integration with good UX (not homebrew crypto)
- **Calendar sync**: Full calendar state, CalDAV, external calendar APIs, reminders, and calendar-grid UI. Email-derived invite parsing/display/RSVP already shipped separately under `docs/calendar-email/`.
