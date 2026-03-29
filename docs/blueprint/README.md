# mxr — Technical Blueprint

> A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

## Document Index

| # | Document | What it covers |
|---|---|---|
| 00 | [Overview](00-overview.md) | Project identity, pitch, core principles, differentiators, language choice, name |
| 01 | [Architecture](01-architecture.md) | Daemon design, crate map, dependency rules, external crate choices |
| 02 | [Data Model](02-data-model.md) | Internal types, SQLite schema, identity/capability seams, typed IDs, UnsubscribeMethod, Snooze |
| 03 | [Providers](03-providers.md) | Split traits, Gmail, IMAP, SMTP, fake provider, adapter kit, adapter strategy |
| 04 | [Sync](04-sync.md) | Sync lifecycle, delta sync, eager body fetch, snooze wake loop, error handling, diagnostics |
| 05 | [Search](05-search.md) | Tantivy BM25, semantic profiles, search modes, RRF hybrid search, saved searches, operator notes |
| 06 | [Compose](06-compose.md) | $EDITOR flow, YAML frontmatter, context block, markdown→multipart, draft management |
| 07 | [Rendering](07-rendering.md) | Plain text first, reader mode, HTML conversion, unsubscribe feature, distraction-free philosophy |
| 08 | [TUI](08-tui.md) | Layout, vim motions, keybinding system, command palette, action dispatch, daemon events |
| 09 | [CLI](09-cli.md) | Subcommands, semantic/profile commands, output formats, shell integration |
| 10 | [Rules Engine](10-rules-engine.md) | Deterministic rules, conditions/actions, dry-run, shell hooks, phasing |
| 11 | [Export](11-export.md) | Thread export formats: Markdown, JSON, Mbox, LLM Context |
| 12 | [Config](12-config.md) | TOML structure, semantic search config, model cache, keybindings, credential storage |
| 13 | [Open Source](13-open-source.md) | Contributor experience, adapter kit, licensing, CI, repo structure |
| 14 | [Roadmap](14-roadmap.md) | Phased milestones with checklists: Phase 0-4 + future ideas |
| 15 | [Decision Log](15-decision-log.md) | Every "we considered X, chose Y because Z" decision, including hybrid-search model and delivery choices |
| 16 | [Addendum](16-addendum.md) | Post-blueprint amendments (A001-A009): inline CLI compose, full CLI surface, vim+Gmail keybindings, daemon observability, TUI batch ops, IMAP first-party, bug reporting |
| 17 | [Release Pipeline](17-release-pipeline.md) | CI/CD pipeline: PR checks, release automation, cross-compiled binaries, Homebrew, changelog, docs deployment (D066-D071) |
| 18 | [Bug Reporting](18-bug-reporting.md) | `mxr bug-report` command, log sanitization, log retention, diagnostic capture workflow (D072-D074) |
| — | [Internal Model Audit](internal-model-audit.md) | Keep/document/tighten/adjust judgment on the current provider-agnostic mail model |
| — | [IPC Audit](ipc-audit.md) | Current protocol inventory classified into `core-mail`, `mxr-platform`, `admin-maintenance`, and `client-specific` |

## For coding agents

This blueprint is designed to be consumed by a coding agent. Every feature is specified in detail. Every design decision includes context on what was considered and rejected. The decision log (15) exists specifically so that an agent doesn't re-debate settled decisions.

Start with 00 (overview) and 14 (roadmap) for the big picture. Use 15 (decision log) as a reference when making implementation choices. Check 16 (addendum) for post-blueprint amendments that override or extend the main docs. Check 17 (release pipeline) for CI/CD and release automation. Consult the specific domain docs (02-13) for detailed specifications.

When blueprint docs conflict with the current repo, prefer code as source of truth. The IPC audit exists specifically to document current implemented contract boundaries.

## Core stack summary

| Component | Technology | Crate |
|---|---|---|
| Language | Rust | — |
| Async runtime | Tokio | `tokio` |
| Database | SQLite | `sqlx` |
| Search engine | Tantivy | `tantivy` |
| TUI framework | Ratatui | `ratatui` + `crossterm` |
| Email parsing | Stalwart mail-parser | `mail-parser` |
| SMTP | Lettre | `lettre` |
| Gmail API | Direct REST | `reqwest` + `oauth2` |
| Markdown → HTML | Comrak | `comrak` |
| Fuzzy matching | Nucleo (from Helix) | `nucleo` |
| Credentials | System keyring | `keyring` |
| HTML → text | html2text | `html2text` |
