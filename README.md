# mxr

**Your email, your editor, your terminal.**

A blazing fast, local-first terminal email client. Compose in your `$EDITOR`. Search in milliseconds. Script everything. Own your data.

## Install

```bash
# Homebrew
brew tap planetaryescape/mxr && brew install mxr

# Cargo
cargo install mxr

# From source
git clone https://github.com/planetaryescape/mxr
cd mxr && cargo build --release
```

Works with **Gmail**, **IMAP**, and **SMTP** out of the box. Community adapters can add any provider.

## Why mxr exists

Every time you open a web email client, you leave your flow. You type into a laggy textarea with none of the muscle memory you've built. Writing email feels like a chore because the tools make it one.

mutt solved this decades ago, but it feels like it. aerc is a step forward but doesn't reimagine the experience. himalaya is a CLI tool, not a full client.

**mxr sits in the gap** — modern terminal email with the keyboard feel of a well-configured editor, instant search powered by a real engine, a CLI that makes every operation scriptable, and a local-first architecture that means your email is always yours.

## Blazing fast

- **<50ms** to search across 10,000+ messages (Tantivy BM25 with field boosts)
- **0ms** to open any message (local SQLite, no network call)
- **Instant** compose — your `$EDITOR` launches immediately with markdown + YAML frontmatter

## What makes mxr different

**Compose in your editor.** `$EDITOR` opens with markdown and YAML frontmatter. Your keybindings, your plugins, your muscle memory. mxr handles parsing, markdown-to-multipart conversion, and sending.

**Search, don't browse.** Tantivy gives you BM25-ranked, sub-second full-text search across your entire mailbox. Saved searches live in the sidebar as programmable lenses. Search is navigation, not a bolt-on filter.

**Script everything.** Every single TUI action has a CLI equivalent. Pipe, chain, automate. Batch mutations with `--search`. Machine-readable output with `--format json`. Your AI coding agent can manage your inbox.

**Daemon-backed.** The daemon is the system. The TUI is just a thin client. Sync, indexing, rules, and snooze run in the background. Close the TUI — nothing stops.

**Local-first.** SQLite is the canonical store. Your email lives on your machine. Works offline. Cloud services are transports, not dependencies.

**Distraction-free.** Reader mode strips tracking pixels, banners, signatures, and quoted text. One key to unsubscribe from anything.

## AI-agent native

mxr's full CLI surface means your coding agent can do anything with your email. Install the [mxr skill](https://mxr-mail.vercel.app/guides/agent-skill/) and ask:

> "Go through my unread emails from the last 24 hours. Summarize each one, flag anything that needs a response, and draft replies for the urgent ones."

> "Find all CI failure emails, extract failing test names, cross-reference with my recent commits, and archive the ones that have since been fixed."

> "I'm prepping for my 1-on-1 with Sarah. Pull up all threads between us from the last two weeks, summarize open items, and draft an agenda."

> "Export the thread about the Q2 roadmap as markdown, summarize key decisions, and create a TODO list from the action items."

Every command supports `--format json`, `--dry-run`, and `--search` for batch operations. No screen scraping or browser automation — just the CLI.

## Built to extend

- **Build your own provider.** The adapter interface is a clean Rust trait. Gmail, IMAP, and SMTP are first-party. Write an adapter for Outlook, Fastmail, ProtonMail — the conformance test suite validates your implementation.
- **Build your own client.** The daemon speaks JSON over a Unix socket. The TUI is just one thin client. Build a web dashboard, a mobile bridge, a Raycast extension — anything that can open a socket.
- **Automate with rules.** Deterministic rules engine with TOML definitions. Dry-run before you commit. Shell hooks as escape hatches.

## Architecture

```
Clients              Engine                 Storage          Providers
-------              ------                 -------          ---------
TUI (thin client)                           SQLite           Gmail
CLI                  Daemon                 Tantivy          IMAP
AI Agent        <->  (sync, rules,     <->              <-> SMTP
Scripts               compose, snooze,                      Your Adapter
Your Client           export, IPC)
       \_____ Unix Socket (JSON) _____/
```

## How it compares

| | mutt | aerc | himalaya | **mxr** |
|---|:---:|:---:|:---:|:---:|
| Daemon architecture | no | no | no | **yes** |
| Local SQLite store | no | no | no | **yes** |
| Full-text search engine | no | no | no | **yes** |
| Full CLI for every action | partial | partial | yes | **yes** |
| Compose in $EDITOR | partial | partial | yes | **yes** |
| Markdown to multipart | no | no | no | **yes** |
| Saved searches as sidebar lenses | no | no | no | **yes** |
| One-key unsubscribe | no | no | no | **yes** |
| Rules with dry-run | procmail | no | no | **yes** |
| AI-agent compatible CLI | no | no | partial | **yes** |
| Pluggable provider adapters | no | partial | partial | **yes** |
| Custom client support | no | no | no | **yes** |

## Quick start

```bash
# Start the daemon
mxr daemon --foreground

# In another terminal — open the TUI
mxr

# Or use the CLI
mxr search "is:unread" --format json
mxr compose --to alice@example.com --subject "Hello"
mxr labels
mxr status
```

See the [docs](https://mxr-mail.vercel.app) for setup guides, CLI reference, and the full feature set.

## Open source

mxr is MIT / Apache-2.0 dual-licensed. No telemetry, no analytics, no "phone home."

We welcome contributions — bug fixes, new provider adapters, CLI improvements, documentation, and ideas. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Built with

Rust, SQLite (sqlx), Tantivy, Ratatui, Tokio, Stalwart mail-parser, Lettre

## License

MIT OR Apache-2.0
