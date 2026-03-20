# mxr

**The CLI for your email.**

A programmable, agent-native email client that works across every provider. One binary, full CLI, hackable with any language you already use.

## Install

```bash
# Homebrew
brew tap planetaryescape/mxr && brew install mxr

# Cargo
cargo install mxr

# Pre-built binaries: macOS (Intel + Apple Silicon), Linux x86_64
# https://github.com/planetaryescape/mxr/releases/latest
```

Works with Gmail, IMAP, and SMTP out of the box. One local database, one CLI, all your accounts.

## A CLI for all your email

Tools like mutt and aerc pioneered terminal email. himalaya brought a clean CLI-first approach. [gog](https://github.com/steipete/gogcli) and [Google Workspace CLI](https://github.com/googleworkspace/cli) made Gmail fully scriptable. [notmuch](https://notmuchmail.org/) proved that local indexing and search changes everything. mxr combines these ideas: a single CLI that works across Gmail, IMAP, and SMTP, backed by a local database and a real search engine.

One binary. One CLI that can search, compose, reply, label, archive, snooze, unsubscribe, and export across all your accounts. One interface your scripts and agents can talk to, regardless of what provider sits behind it.

A real CLI with `--format json`, `--dry-run`, `--search` for batch operations, and every output piped through stdout. Hack it with Python, Bash, Go, TypeScript, whatever you already use.

## Agent-native email

Your coding agent can already write code, run tests, and commit to git. Now it can manage your email too. Install the [mxr skill](https://mxr-mail.vercel.app/guides/agent-skill/) and ask:

> "Go through my unread emails from the last 24 hours. Summarize each one, flag anything that needs a response, and draft replies for the urgent ones."

> "Find all CI failure emails, extract failing test names, cross-reference with my recent commits, and archive the ones that have since been fixed."

> "I'm prepping for my 1-on-1 with Sarah. Pull up all threads between us from the last two weeks, summarize open items, and draft an agenda."

> "Export the thread about the Q2 roadmap as markdown, summarize key decisions, and create a TODO list from the action items."

This works because the CLI *is* the interface. Every operation supports `--format json`, `--dry-run`, and `--search` for batch operations. Your agent doesn't need screen scraping or browser automation, just `mxr`.

## Local-first, fast

- **<50ms** to search across 10,000+ messages (Tantivy BM25 with field boosts)
- **0ms** to open any message (local SQLite, no network call)
- **Instant** compose, your `$EDITOR` launches immediately with markdown + YAML frontmatter

SQLite is the canonical store. Your email lives on your machine. Works offline. No spinners. No loading states.

## Everything else

**Compose in your editor.** `$EDITOR` opens with markdown and YAML frontmatter. Your keybindings, your plugins, your muscle memory.

**Daemon-backed.** The daemon is the system. The TUI is a thin client. Sync, indexing, rules, and snooze run in the background. Close the TUI, nothing stops.

**Provider-agnostic.** Gmail, IMAP, SMTP all normalize into one internal model. Mix providers freely. Write new adapters with a clean Rust trait.

**Distraction-free.** Reader mode strips tracking pixels, banners, and quoted text. One key to unsubscribe.

**Build your own client.** The daemon speaks JSON over a Unix socket. Build a web dashboard, a mobile bridge, a Raycast extension, anything that can open a socket.

## Where mxr fits in

| | gog / gws | notmuch | mutt | aerc | himalaya | meli | **mxr** |
|---|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| Works beyond Google | no | **yes** | **yes** | **yes** | **yes** | **yes** | **yes** |
| Full CLI for every action | **yes** (Gmail) | most | partial | partial | **yes** | no | **yes** |
| JSON output for scripting | **yes** | **yes** | no | no | **yes** | no | **yes** |
| Compose and send from CLI | **yes** | no | **yes** | partial | **yes** | no | **yes** |
| Batch operations via search | partial | tag only | no | no | no | no | **yes** |
| Daemon architecture | no | no | no | no | no | no | **yes** |
| Local database | no | **Xapian** | no | no | no | optional | **SQLite** |
| Full-text search engine | no | **yes** | no | no | no | optional | **yes** |
| Compose in $EDITOR | no | via Emacs | **yes** | **yes** | **yes** | partial | **yes** |
| Pluggable provider adapters | no | no | no | partial | partial | partial | **yes** |
| Custom client support | no | **yes** | no | no | no | no | **yes** |

## Quick start

```bash
mxr daemon --foreground    # start the daemon
mxr                        # open the TUI
mxr search "is:unread" --format json
mxr compose --to alice@example.com --subject "Hello"
mxr archive --search "older:30d label:notifications" --dry-run
```

See the [docs](https://mxr-mail.vercel.app) for setup guides, CLI reference, and the full feature set.

## Open source

mxr is MIT / Apache-2.0 dual-licensed. Built with Rust, one binary, no runtime, runs on every platform. No telemetry, no analytics, no "phone home."

Contributions welcome: bug fixes, new provider adapters, CLI improvements, documentation, ideas. See [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## Built with

Rust, SQLite (sqlx), Tantivy, Ratatui, Tokio, Stalwart mail-parser, Lettre

## License

MIT OR Apache-2.0
