# mxr — Project Overview

## One-line pitch

A local-first, open-source, keyboard-native email client for terminal users, built around a daemon, a clean provider-agnostic model, and a programmable core.

## Extended pitch

"Superhuman for terminal people, but local-first and scriptable."

mxr is a fast, distraction-free terminal email client that treats your inbox as structured data. It syncs to a local SQLite database, indexes with Tantivy for blazing fast search, composes in your $EDITOR with markdown, and runs as a daemon so scripts, TUI, and CLI all talk to the same engine. First-party Gmail sync and SMTP send. Other providers plug in via a clean adapter interface.

## What mxr is

- A local-first terminal email client
- A keyboard-native interface with vim motions
- A daemon-backed architecture (TUI is a client, not the system)
- A fast search engine for your email (BM25 via Tantivy)
- A $EDITOR compose workflow with markdown-to-multipart rendering
- A programmable core with deterministic rules, saved searches, and shell hooks
- An open-source project designed for contribution from day one

## What mxr is NOT

- Not a universal mail compatibility layer (we build for Gmail + SMTP, community builds the rest)
- Not an AI product (AI features may come later, but the core is deterministic and local)
- Not a CRM, not a newsletter engine, not an automation platform — yet. These may grow out of the core, but they are not the launch identity.
- Not trying to render pixel-perfect HTML in the terminal

## Why this exists

Every terminal email client today falls into one of two camps:

1. **Legacy tools** (mutt, neomutt) — powerful but feel like they were designed in 1995 because they were. Configuration is arcane. The learning curve is steep. They work but they don't feel modern.

2. **Modern attempts** (aerc, himalaya) — better UX but either thin CLI wrappers or mutt with a fresh coat of paint. himalaya is more of a CLI tool than a full client. aerc is closer but doesn't reimagine the experience.

mxr sits in the gap: a modern terminal email client with the keyboard UX of a well-configured neovim, the search speed of a dedicated engine, the composability of Unix tools, and a local-first architecture that means your email is always yours.

## Differentiators from existing tools

| Feature | mutt/neomutt | aerc | himalaya | mxr |
|---|---|---|---|---|
| Daemon architecture | No | No | No | Yes |
| Local-first SQLite store | No | No | No | Yes |
| Tantivy search engine | No | No | No | Yes |
| Saved searches as core primitive | No | No | No | Yes |
| Command palette (Ctrl-P) | No | No | No | Yes |
| $EDITOR compose with YAML frontmatter | Partial | Partial | Yes | Yes |
| Reader mode (signature/quote stripping) | No | No | No | Yes |
| One-key unsubscribe (RFC 2369) | No | No | No | Yes |
| Local snooze with inbox-zero workflow | No | No | No | Yes |
| Deterministic rules engine | procmail | No | No | Yes |
| Thread export for LLM context | No | No | No | Yes |
| Markdown compose → multipart | No | No | No | Yes |
| Configurable vim keybindings | Partial | Partial | No | Yes |
| Adapter interface for community providers | No | Partial | Partial | Yes |

## Core Principles

These are non-negotiable. They guide every design decision. If a feature or implementation conflicts with these, the feature loses.

### 1. Local-first

Your email lives on your machine. SQLite is the canonical state store. The search index is rebuildable from SQLite. mxr works offline. Cloud services are optional transports, not requirements.

### 2. Provider-agnostic internal model

All application logic speaks one language: the mxr internal model. Gmail labels, IMAP folders, and flags all normalize into this model. No provider-specific concepts leak into core code. If a provider disappears, only its adapter crate needs rewriting.

### 3. Daemon-backed architecture

The daemon is the system. The TUI is a client. The CLI is a client. Scripts are clients. This separation means background sync, indexing, and rule execution happen regardless of whether the TUI is open. It also means future alternate frontends (web dashboard, mobile bridge) are architecturally possible.

### 4. $EDITOR for writing

mxr does not compete with your text editor. Compose opens $EDITOR with a markdown file. YAML frontmatter carries metadata (to, cc, subject). The daemon handles the rest: parse frontmatter, convert markdown to multipart (text/plain + text/html), send via provider.

### 5. Fast search is a first-class feature

Search is not an afterthought bolted onto a folder browser. Tantivy provides BM25 ranking, field-level boosts, faceted filtering, and sub-second results across large mailboxes. Every email is indexed at sync time. Search is how power users navigate — not folder trees.

### 6. Saved searches are a core primitive

Saved searches are user-programmed inbox lenses. They live in the sidebar, appear in the command palette, and are the primary way users organize their view of email. They are not a "nice to have" — they are central to the UX.

### 7. Rules engine is deterministic first

Rules are data, not scripts. They are inspectable, replayable, idempotent, and dry-runnable. "Show me what this rule would do" must work before "run this rule." Shell hooks and scripting come later as escape hatches, not as the foundation. Users need to trust automation before they rely on it.

### 8. Shell hooks over premature plugin systems

Don't build a plugin framework. Pipe data to shell commands. Let users write automation in whatever language they want. Unix composition over framework lock-in.

### 9. Adapters are swappable

No provider-specific logic outside adapter crates. Ever. The adapter interface is the contract. If gws (Google Workspace CLI) disappears, if Gmail changes their API, if someone wants Outlook — only the adapter crate changes. Core code is untouched.

### 10. Correctness beats cleverness

No clever macro towers. No "you need to understand my architecture philosophy before fixing a bug." Plain, legible Rust code. Compile-time checked SQL queries. Explicit error types. When in doubt, be boring.

## Non-negotiables (for contributors)

These should be in the README and CONTRIBUTING.md from day one:

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

## Language

Rust.

### Why Rust was chosen over Go

Both were strong candidates. Go would have meant faster iteration (faster compile times, simpler concurrency model, lower learning curve for contributors). However:

- **Tantivy**: The search engine is a Rust library. There's no Go equivalent at the same quality level. Go has Bleve, which is decent but slower and less featureful. Since blazing fast BM25 search is a first-class feature, Rust gives us the best engine natively.
- **Ratatui**: The TUI framework is more capable than Go's Bubbletea for complex, multi-pane UIs with real-time updates.
- **Ecosystem**: `mail-parser` (Stalwart), `lettre`, `sqlx`, `comrak`, `nucleo` — the Rust crate ecosystem for this specific project is unusually strong.
- **No GC**: For a long-running daemon managing a large email corpus, predictable memory behavior matters.
- **Single binary**: Both Rust and Go produce single binaries, but Rust binaries are smaller.

The tradeoff is slower iteration speed and a higher contributor barrier. We accept this because the core technical differentiators (search speed, TUI quality) benefit directly from the Rust ecosystem.

## Name

**mxr** (pronounced "mixer" or as letters "M-X-R").

- Short, distinctive, terminal-friendly, easy to type
- Subtle connection to MX records (the DNS record type for mail servers) without being on-the-nose
- "Mixer" works as a metaphor: multiple backends, mail + automation, local engine + TUI, structured search + workflows
- Available on crates.io
- No conflicts with existing CLI tools or significant GitHub repos

## License

Dual MIT + Apache-2.0 (Rust ecosystem convention, maximally permissive for contributors).
