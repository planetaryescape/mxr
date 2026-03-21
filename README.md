# mxr

**Local-first email infrastructure.**

mxr syncs Gmail and IMAP into SQLite on your machine. You read mail in the TUI, script it from the CLI, and hand it to an agent when that helps. Same local data. Same daemon. Same commands.

Today, the shipped surfaces are the CLI, TUI, daemon socket, and agent skill. A first-party MCP server is still on the roadmap.

## Install

```bash
# Homebrew
brew tap planetaryescape/mxr && brew install mxr

# Cargo
cargo install mxr

# Pre-built binaries
# https://github.com/planetaryescape/mxr/releases/latest
```

Current release shape:

- macOS and Linux
- Gmail sync/send
- IMAP sync
- SMTP send
- CLI, TUI, daemon socket, agent skill

## Why this feels different

mxr connects to your provider directly, syncs mail into a local SQLite database, and indexes it with Tantivy. No hosted relay. No extra control plane in the middle. Your scripts, your terminal, and your agent all talk to the same local runtime.

That makes it a different tool from a classic terminal client and a different tool from a hosted connector layer. mutt, aerc, himalaya, notmuch, gog, and gws each got an important part of this right. Hosted tools like Nylas CLI, Composio, Zapier MCP, and EmailEngine solve a different problem. mxr sits in the middle: local mail runtime, broad CLI surface, daemon-backed state, and structured output.

## Use it from a shell or an agent

No SDK. No custom DSL. If a tool can run a command and parse JSON, it can work with mxr.

```bash
mxr search "is:unread from:buildkite" --format json \
  | jq -r '.[].message_id'
```

That same surface is what the agent skill uses. A coding agent can search, read, draft, export, and batch-mutate mail through the CLI that already exists.

Example prompt:

> "Look through unread mail from the last 24 hours. Tell me what needs a reply, draft answers for the urgent threads, and leave the rest alone."

## Local-first, in practice

- Search stays local after sync.
- Opening a message is a SQLite read, not a network round trip.
- Reader mode keeps HTML-heavy mail readable in the terminal.
- When you need the original rendering, open it in the browser and keep going.
- Provider adapters go through a conformance suite instead of one-off glue.

Read [ARCHITECTURE.md](ARCHITECTURE.md) for the design principles behind the daemon, store, provider model, and trust boundary.

## Where mxr fits

The short version:

- Use mxr if you want local mail state, a broad CLI, a daemon, and one surface that works for both people and agents.
- Use a classic terminal client if you mostly want an interactive mail UI and don't need a local mail runtime behind it.
- Use a hosted connector layer if you want managed auth, remote workflows, or lots of SaaS tools behind one endpoint.

### Direct mail tools

| Tool | Good fit when you want... | Less central there |
|---|---|---|
| mutt / neomutt | a long-established terminal workflow | local daemon + structured CLI |
| aerc | a modern terminal UI | local database + agent-oriented CLI surface |
| himalaya | a clean CLI-first mail client | daemon-backed local runtime |
| notmuch | local indexing and search over maildirs | provider sync + broad mutation CLI |
| gog / gws | Gmail scripting | non-Google provider support |
| **mxr** | one local runtime for CLI, TUI, scripts, and agents | hosted connector workflows |

### Connector layers and nearby tools

| Tool | Good fit when you want... | mxr difference |
|---|---|---|
| Nylas CLI | managed provider access + CLI/MCP workflow | mxr keeps the runtime local |
| Composio / Zapier MCP | hosted auth + cross-app automation | mxr is mail-first, local, and daemon-backed |
| EmailEngine | self-hosted email API for backend systems | mxr is for local human + agent workflows |
| Post | a local mail daemon + CLI on macOS | mxr aims for cross-provider Rust tooling |
| email-mcp | local MCP access to IMAP/SMTP | mxr is a broader mail platform than the bridge alone |

## Quick start

```bash
mxr daemon --foreground
mxr
mxr search "is:unread" --format json
mxr archive --search "older:30d label:notifications" --dry-run
mxr history --category mutation
```

## Docs

- Site: [mxr-mail.vercel.app](https://mxr-mail.vercel.app)
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- Blueprint: [docs/blueprint/README.md](docs/blueprint/README.md)

## Open source

mxr is MIT / Apache-2.0 dual-licensed. The codebase is open. There is no telemetry or phone-home service in the core architecture.

Contributions are welcome, especially around adapters, CLI ergonomics, docs, and tests. The adapter surface is meant to be readable and replaceable, not magical.

## Built with

Rust, SQLite via sqlx, Tantivy, Ratatui, Tokio, Stalwart mail-parser, Lettre

## License

MIT OR Apache-2.0
