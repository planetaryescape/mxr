# mxr

[![CI](https://github.com/planetaryescape/mxr/actions/workflows/ci.yml/badge.svg)](https://github.com/planetaryescape/mxr/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/rust-1.88%2B-blue.svg)](https://github.com/planetaryescape/mxr/blob/main/Cargo.toml)

**Local-first email infrastructure.**

mxr syncs Gmail and IMAP into SQLite on your machine. You read mail in the TUI, script it from the CLI, and hand it to an agent when that helps. Same local data. Same daemon. Same commands.

Today, the shipped surfaces are the CLI, TUI, daemon socket, and agent skill. A first-party MCP server is still on the roadmap.

## Install

```bash
# Homebrew
brew tap planetaryescape/mxr
brew install mxr

# Cargo (release tag)
cargo install --git https://github.com/planetaryescape/mxr --tag v0.4.53 --locked mxr
```

Pre-built release tarballs are also available for:

- macOS Apple Silicon
- macOS Intel (without the optional local semantic backend)
- Linux x86_64

[Download the latest release](https://github.com/planetaryescape/mxr/releases/latest)

If you want current `main` instead of the latest release:

```bash
cargo install --git https://github.com/planetaryescape/mxr --locked mxr

# or clone locally
git clone https://github.com/planetaryescape/mxr
cd mxr
cargo install --path . --locked
```

## Demo

A renderable terminal demo tape lives at [`docs/demo.tape`](docs/demo.tape). It runs `mxr` inside isolated temp config/data dirs so the walkthrough doesn't touch your real local state.

Current release shape:

- macOS and Linux
- Gmail sync/send
- IMAP sync
- SMTP send
- lexical + hybrid + semantic search
- CLI, TUI, daemon socket, agent skill

## Search modes

mxr supports three local search modes:

- `lexical`: exact BM25/Tantivy retrieval
- `hybrid`: lexical + dense retrieval + RRF
- `semantic`: dense retrieval only

Semantic search is an optional `mxr-platform` feature layered on top of the mail runtime, not a core mail requirement. Sync/read/send still work without it.

Embeddings stay local. First enable may download the selected local model and build embeddings for the active profile. Sync now prepares semantic chunks for changed messages even while semantic retrieval is off, so later enablement is cheaper.

High-level enablement:

```toml
[search]
default_mode = "hybrid"

[search.semantic]
enabled = true
active_profile = "bge-small-en-v1.5"
auto_download_models = true
```

Useful commands:

```bash
mxr semantic status
mxr semantic profile use multilingual-e5-small
mxr semantic reindex
```

OCR is not used for semantic indexing. Image attachments and scanned/image-only PDFs are skipped unless real text extraction succeeds.

## What happens after new mail syncs in?

Current lifecycle:

1. envelope + body are written to SQLite during sync
2. Tantivy is updated during that same batch
3. the lexical batch is committed before sync completes, so lexical search is fresh immediately after sync
4. the daemon then persists semantic chunks for the upserted messages
5. embeddings are generated only if semantic retrieval is enabled

So:

- `mxr search ... --mode lexical` is the immediate freshness path
- `mxr search ... --mode hybrid` and `--mode semantic` depend on semantic profile readiness
- turning semantic on later can reuse stored chunks instead of rebuilding all semantic prep from scratch

Useful checks:

```bash
mxr status
mxr semantic status
mxr doctor --semantic-status
```

## Reset local runtime state

When local mxr state gets messy during development or recovery, you can wipe the rebuildable runtime state without deleting config or credentials:

```bash
mxr reset --hard --dry-run
mxr reset --hard --including-config --dry-run
mxr burn --dry-run
```

Real execution is intentionally hard to trigger:

- `mxr reset --hard` is the primary command
- `mxr burn` is the memorable alias
- both stop the daemon first, then remove local runtime state under `MXR_DATA_DIR`
- both preserve `config.toml` and system keychain/keyring credentials by default
- `--including-config` also deletes `config.toml`, but still preserves system keychain/keyring credentials
- attachment dirs outside `MXR_DATA_DIR` stay preserved, even with `--including-config`
- `--dry-run` prints the exact delete plan first
- interactive destructive runs require typing `DELETE MY MXR DATA`
- interactive `--including-config` runs require typing `DELETE MY MXR DATA AND CONFIG`
- non-interactive destructive runs require `--yes-i-understand-this-destroys-local-state`

## Why this feels different

mxr connects to your provider directly, syncs mail into a local SQLite database, and indexes it with Tantivy. No hosted relay. No extra control plane in the middle. Your scripts, your terminal, and your agent all talk to the same local runtime.

That makes it a different tool from a classic terminal client and a different tool from a hosted connector layer. mutt, aerc, himalaya, notmuch, gog, and gws each got an important part of this right. Hosted tools like Nylas CLI, Composio, Zapier MCP, and EmailEngine solve a different problem. mxr sits in the middle: local mail runtime, broad CLI surface, daemon-backed state, and structured output.

Operating rules:

- CLI first. The TUI is built on the same daemon surface and should not be the only way to do something.
- Mutations should be previewable before commit.
- JSON is for piping, scripting, and agents, not just debugging.
- Unix composition beats framework lock-in.
- Daemon healing is event-driven: stale sockets are cleaned up, mismatched daemon builds are restarted, and bad indexes are repaired or rebuilt. No timed restarts. No self-updates.

## Use it from a shell or an agent

No SDK. No custom DSL. If a tool can run a command and parse JSON, it can work with mxr.

```bash
mxr search "is:unread from:buildkite" --format json \
  | jq -r '.[].message_id'
```

That same surface is what the agent skill uses. A coding agent can search, read, draft, export, and batch-mutate mail through the CLI that already exists.

Example prompt:

> "Look through unread mail from the last 24 hours. Tell me what needs a reply, draft answers for the urgent threads, and leave the rest alone."

That works because the CLI is the canonical surface: machine-readable when you need it, interactive when you want it.

## Local-first, in practice

- Search stays local after sync.
- Opening a message is a SQLite read, not a network round trip.
- `/` in Mailbox jumps into full-index Search. `Ctrl-f` only filters the current mailbox.
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
mxr sync
mxr
mxr search "is:unread" --format json
mxr archive --search "older:30d label:notifications" --dry-run
mxr reset --hard --dry-run
mxr history --category mutation
```

## Docs

- Site: [mxr-mail.vercel.app](https://mxr-mail.vercel.app)
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- Blueprint: [docs/blueprint/README.md](docs/blueprint/README.md)
- Tokio runtime guide: [docs/reference/tokio-runtime-guide.md](docs/reference/tokio-runtime-guide.md)
- Test standard: [docs/idiomatic-rust-tests.md](docs/idiomatic-rust-tests.md)

## Open source

mxr is MIT / Apache-2.0 dual-licensed. The codebase is open. There is no telemetry or phone-home service in the core architecture.

Contributions are welcome, especially around adapters, CLI ergonomics, docs, and tests. The adapter surface is meant to be readable and replaceable, not magical.

## Built with

Rust, SQLite via sqlx, Tantivy, Ratatui, Tokio, Stalwart mail-parser, Lettre

## License

MIT OR Apache-2.0
