# mxr

**Your email, your editor, your terminal.**

mxr is a local-first terminal email client that lets you write email the same way you write code — in your own editor, with your own keybindings, at your own speed.

## Why mxr exists

Writing email shouldn't require a context switch. But every time you open Gmail or Outlook, that's exactly what happens. You leave vim. You leave your terminal. You type into a laggy textarea with none of the muscle memory you've spent years building. Formatting is unpredictable. There's no composability. It's a different world.

mutt and neomutt solved this in 1995. They still work, but they feel like it. Configuration is arcane. The UX hasn't evolved. aerc is a step forward but still feels like mutt with a fresh coat of paint. himalaya is more of a CLI tool than a full client.

mxr sits in the gap: a modern terminal email client with the keyboard feel of a well-configured neovim, instant search powered by a real engine, and a local-first architecture that means your email is always yours — even offline.

## What makes mxr different

**Compose in your editor.** `$EDITOR` opens with a markdown file. YAML frontmatter carries your metadata. Write your email the way you write everything else. mxr handles the rest: parsing, markdown-to-multipart conversion, sending.

**Search, don't browse.** Power users don't click through folder trees. They search. mxr uses Tantivy (the same engine behind quickwit) to give you BM25-ranked, sub-second search across your entire mailbox. Search is navigation, not an afterthought.

**Distraction-free reading.** mxr shows you what the email says, not what it's trying to sell you. No tracking pixels. No hero banners. No animated GIFs. Just the words. One key to unsubscribe from newsletters you're done with.

**Daemon-backed.** The TUI is a client, not the system. Sync, indexing, rules, and snooze all run in the background daemon. The CLI talks to the same daemon. Scripts talk to the same daemon. Close the TUI — nothing stops.

**Local-first.** SQLite is the canonical store. Your email lives on your machine. The search index rebuilds from SQLite. Works offline. Cloud services are transports, not dependencies.

**Provider-agnostic.** Gmail and IMAP sync today. SMTP sends. Everything maps into one internal model. Adapters are swappable — no provider lock-in.

## How it compares

| | mutt/neomutt | aerc | himalaya | mxr |
|---|---|---|---|---|
| Daemon architecture | - | - | - | yes |
| Local SQLite store | - | - | - | yes |
| Tantivy search engine | - | - | - | yes |
| Compose in $EDITOR | partial | partial | yes | yes |
| Markdown to multipart | - | - | - | yes |
| YAML frontmatter metadata | - | - | - | yes |
| Saved searches as sidebar lenses | - | - | - | yes |
| Command palette (Ctrl-P) | - | - | - | yes |
| One-key unsubscribe | - | - | - | yes |
| Reader mode (strip signatures/quotes) | - | - | - | yes |
| Deterministic rules with dry-run | procmail | - | - | yes |
| Thread export for LLM context | - | - | - | yes |
| Vim-native keybindings | partial | partial | - | yes |
| Scriptable via CLI + shell hooks | partial | partial | partial | yes |

## Install

**Homebrew:**

```bash
brew tap planetaryescape/mxr
brew install mxr
```

**Cargo:**

```bash
cargo install mxr-daemon
```

**Pre-built binaries:**

Download from [GitHub Releases](https://github.com/planetaryescape/mxr/releases) — available for macOS (Intel + Apple Silicon) and Linux (x86_64 + aarch64).

**cargo-binstall:**

```bash
cargo binstall mxr-daemon
```

## Quick start

```bash
# Start the daemon
mxr daemon --foreground

# In another terminal — open the TUI
mxr

# Or use the CLI
mxr search "label:inbox is:unread"
mxr compose
mxr labels
mxr status
```

See the [docs](https://mxr-mail.vercel.app) for Gmail setup, IMAP configuration, and the full guide.

## Built with

- **Rust** — fast, safe, no runtime
- **SQLite** (sqlx) — local-first canonical store, compile-time checked queries
- **Tantivy** — BM25 full-text search with field boosts
- **Ratatui** — terminal UI framework
- **Tokio** — async runtime, daemon, background sync
- **Stalwart mail-parser** — RFC-compliant email parsing
- **Lettre** — SMTP sending

## Documentation

- [Docs site](https://mxr-mail.vercel.app) — guides, reference, getting started
- [Blueprint](docs/blueprint/) — design decisions and architecture
- [Contributing](CONTRIBUTING.md) — how to contribute

## License

MIT OR Apache-2.0
