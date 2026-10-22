# mxr: Technical Blueprint

> A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

For verified implementation gaps, shipped features that these plans understate,
and docs-site drift, see the [2026-10-20 implementation audit](../research/blueprint-implementation-audit-2026-10-20.md).
It distinguishes missing code from historical designs and explicitly deferred work.

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
| 17 | [Release Pipeline](17-release-pipeline.md) | CI/CD pipeline: PR checks, release automation, scoped binaries, Homebrew, changelog, docs build checks, v1 unsigned macOS policy (D066-D071) |
| 18a | [OAuth Addendum](18-addendum-oauth.md) | Gmail BYOC-first OAuth guidance, bundled fallback posture, credential storage, redirect flow, and provider scopes |
| 18b | [Bug Reporting](18-bug-reporting.md) | `mxr bug-report` command, log sanitization, log retention, diagnostic capture workflow (D072-D074) |
| 20 | [Transports](20-transports.md) | Transport-adapter system: frozen protocol + byte-stream adapters + HTTP gateway, trait reference, per-transport security policy, the token gate, the community adapter kit + conformance, and the backlog (D052-D057) |
| 21 | [Web Experience](21-web-experience.md) | The desk model, what shipped v0.6.34 to v0.6.42, standards every web change keeps, and the ordered plan to pass the experience rubric (D095-D102) |
| 22 | [Email Modes](22-email-modes.md) | Email as five modes (Messages, To do, Updates, Reading, Archive), each with its own view shape, key map, index recipe and model tier; multi-mode membership and handoff; rules-first classification; to-do lead times; and the phased plan (D107-D114) |
| 22r | Email modes research | One note per mode with sources, mockups and risks: [messages](../research/email-modes/messages.md), [to do](../research/email-modes/todo.md), [updates](../research/email-modes/updates.md), [reading](../research/email-modes/reading.md), [archive](../research/email-modes/archive.md), [now and handoff](../research/email-modes/now-and-handoff.md), plus [sign in with ChatGPT](../research/chatgpt-sign-in.md) |
| 23 | [Daemon portability](23-daemon-portability.md) | Durable drafts, recoverable actions, portable files, API contracts, client targeting and headless restore; ordered implementation slices for a future SwiftUI client or personal VPS |
| 24 | [Remote hosting and clients](24-remote-hosting-and-clients.md) | Proposed `mxr serve` / `mxr connect`, authenticated Tailscale hosting, saved targets, registrations, presence, shutdown/reconnect and deregistration |
| 25 | [The assistant](25-assistant.md) | The in-app chat agent: daemon loop over the MCP tools, approval enforced in code, ask_user questions, append-only history, web sidebar and `mxr assistant` |
| 26 | [Web design concept](26-web-design-concept.md) | The night desk: what the web app is to the person, the mental model, map and journeys, principles, key moments, motion, hierarchy and the design gate (D123-D141) |
| 27 | [Web design system](27-web-design-system.md) | Tokens (three ink depths, three text tiers, type roles, shape, motion) and the shared parts every screen uses: page frame, row, reason and model marks, keys, controls, overlays, states, and the lints that keep them shared |
| 28 | [Web screens](28-web-screens.md) | Every main screen under the lamp, with mockups: what the eye lands on, what recedes, and what changes from today |
| 29 | [The assistant as the copy desk](29-assistant-design.md) | The assistant's presentation in web and TUI: wire-line steps in the daemon's words, a live line, the app's own objects, proofs to sign |
| 30 | [Design roadmap](30-design-roadmap.md) | Rungs W1-W8 from one page frame to finishing moments, each with its demo and design gate |
| 31 | [Design open questions](31-design-open-questions.md) | Spikes S1-S4 and BK's product questions Q1-Q6 for the design work |
| 32 | [Classifier and attention](32-classifier.md) | One design for understanding each email (five questions, one stored assessment, every email a home), Now as an attention scheduler across modes, teaching versus action rules, and the label, scenario-corpus and eval loop that improves both (draft, not built) |
| 33 | [Account identity](33-account-identity.md) | One colour and tag per account, owned by the daemon and drawn the same way in every row of the web app, TUI and CLI while a view mixes accounts (D147) |
| 35 | [Keyboard](35-keyboard.md) | Keys as one grammar of verbs: the verb table, Shift and `g` rules, one context tree for both clients, overlay grammar, one definition in `crates/keymap` served by `mxr keys`, accessibility and layout rules, and the K1-K4 rungs (D163-D173; draft, not built) |
| 35r | Keyboard research | [How good shortcut systems are designed, and the audit of mxr's keys](../research/keyboard-shortcuts.md) |
| 26r | Web design research | [Audit of every screen, 2026-10-16](../research/web-design/audit-2026-10-16.md) and [agent panels and the best mail apps](../research/web-design/agents-and-mail-apps.md) |
|  | [Internal Model Audit](internal-model-audit.md) | Keep/document/tighten/adjust judgment on the current provider-agnostic mail model |
|  | [IPC Audit](ipc-audit.md) | Current protocol inventory classified into `core-mail`, `mxr-platform`, `admin-maintenance`, and `client-specific` |
|  | [Sync / Index Lifecycle Audit](sync-index-lifecycle-audit.md) | Code-truth audit of sync guarantees, lexical freshness, semantic chunk persistence, and repair behavior |
|  | [Semantic Search Audit](semantic-search-audit.md) | Code-truth audit of semantic ingestion, profiles, OCR removal, and hybrid query semantics |

## For coding agents

This blueprint is designed to be consumed by a coding agent. Every feature is specified in detail. Every design decision includes context on what was considered and rejected. The decision log (15) exists specifically so that an agent doesn't re-debate settled decisions.

Start with 00 (overview) and 14 (roadmap) for the big picture. Use 15 (decision log) as a reference when making implementation choices. Check 16 (addendum) for post-blueprint amendments that override or extend the main docs. Check 17 (release pipeline) for CI/CD and release automation. Consult the specific domain docs (02-13) for detailed specifications.

For daemon portability, future native clients or running the daemon on another host, start with [23](23-daemon-portability.md). Select one ready slice and use its contract, entry points and acceptance checks.

For anything a person sees in the web app, start with [26](26-web-design-concept.md) and [27](27-web-design-system.md): the design concept and the shared parts. Build screens from those parts, follow the rung order in [30](30-design-roadmap.md), and pass the design gate in 26 before calling a rung done. Behaving as specified is the floor, not the goal.

For the next remote-access work after the completed portability preparation, use [24](24-remote-hosting-and-clients.md). Its proposed R01 stages cover hosting and client lifecycles; the SwiftUI app and production VPS migration remain separate work.

When blueprint docs conflict with the current repo, prefer code as source of truth. The IPC audit exists specifically to document current implemented contract boundaries.

## Core stack summary

| Component | Technology | Crate |
|---|---|---|
| Language | Rust | |
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
