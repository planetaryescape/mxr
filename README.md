# mxr

[![CI](https://github.com/planetaryescape/mxr/actions/workflows/ci.yml/badge.svg)](https://github.com/planetaryescape/mxr/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/rust-1.88%2B-blue.svg)](https://github.com/planetaryescape/mxr/blob/main/Cargo.toml)

**Local-first email infrastructure.** Write `mxr`, say "Mixer".

mxr syncs Gmail and IMAP into SQLite on your machine. You read mail in the TUI or web app, script it from the CLI, and hand it to an agent when that helps. Same local data. Same daemon. Same commands.

Today, the shipped surfaces are the CLI, TUI, web app, daemon socket, and agent skill. A first-party MCP server is still on the roadmap.

## Install

```bash
# Homebrew (recommended)
brew tap planetaryescape/mxr
brew install mxr

# Cargo from source at a specific tag
# (replace vX.Y.Z with the latest tag from the releases page)
cargo install --git https://github.com/planetaryescape/mxr --tag vX.Y.Z --locked mxr
```

Linux source installs need native headers for audio and OS keyring support:

```bash
# Debian/Ubuntu
sudo apt-get install -y libasound2-dev libdbus-1-dev pkg-config
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
- Gmail sync/send (OAuth tokens stored in OS keychain)
- IMAP sync (CONDSTORE / QRESYNC + UID fallback + IDLE for real-time delivery)
- SMTP send
- lexical + hybrid + semantic search with Gmail-style operators (`is:unread`, `from:`, `older_than:30d`, etc.)
- CLI, TUI, web app, daemon socket, agent skill
- Inbox tooling: snooze with presets, undoable mutations (60s window), saved searches, deterministic rules with `--dry-run`
- Calendar invites: parse email invites, search `has:calendar`, inspect, backfill, and RSVP with dry-run previews
- LLM-assisted summarize and draft-assist surfaces with local relationship context and deterministic humanizer scoring
- Analytics: stale-thread queue, response-time percentiles, contact decay, year-in-review (`mxr wrapped`), storage rollups

## Public Rust crates

The `mxr` binary is installed from Git or release artifacts, not from
crates.io. A few bounded pieces of the email stack are public crates and
are consumed back into mxr from the registry:

| Crate | What it owns | mxr consumer |
|---|---|---|
| [`mail-query`](https://crates.io/crates/mail-query) | Gmail-style search parser and typed AST | `mxr-search` |
| [`mail-threading`](https://crates.io/crates/mail-threading) | RFC 5256 / JWZ client-side threading | `mxr-sync` |
| [`list-unsubscribe`](https://crates.io/crates/list-unsubscribe) | RFC 2369 / RFC 8058 unsubscribe header parsing | `mxr-mail-parse` |
| [`mailbox-formats`](https://crates.io/crates/mailbox-formats) | mbox variants and Maildir reader/writer | `mxr-export` |

Check the current dependency edge from this repo:

```bash
cargo tree -p mxr-search -i mail-query
cargo tree -p mxr-sync -i mail-threading
cargo tree -p mxr-mail-parse -i list-unsubscribe
cargo tree -p mxr-export -i mailbox-formats
```

## Search modes

mxr supports three local search modes:

- `lexical`: exact BM25/Tantivy retrieval
- `hybrid`: lexical + dense retrieval + RRF
- `semantic`: dense retrieval only

Semantic search is an `mxr-platform` feature layered on top of the mail runtime, not a core mail requirement. It is enabled in the default config so semantic-ready work happens opportunistically, while `lexical` remains the default search mode. Sync/read/send still work without a semantic backend.

Embeddings stay local. First profile activation may download the selected local model and build embeddings for the active profile. Sync prepares semantic chunks for changed messages even while semantic retrieval is explicitly off, so later enablement is cheaper.

High-level enablement:

```toml
[search]
default_mode = "lexical"

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

If semantic retrieval is disabled, unavailable in the current binary, or errors at query time, mxr falls back to lexical ranking and explains the fallback when `--explain` is requested.

## What happens after new mail syncs in?

Current lifecycle:

1. envelope + body are written to SQLite during sync
2. Tantivy is updated during that same batch
3. the lexical batch is committed before sync completes, so lexical search is fresh immediately after sync
4. the daemon then persists semantic chunks for the upserted messages
5. embeddings are generated if semantic retrieval is enabled and the local backend/profile is available

So:

- `mxr search ... --mode lexical` is the immediate freshness path
- `mxr search ... --mode hybrid` and `--mode semantic` depend on semantic profile readiness, and degrade to lexical when dense retrieval is unavailable
- turning semantic on later can reuse stored chunks instead of rebuilding all semantic prep from scratch

## Relationship-aware LLM drafting

`mxr summarize` and `mxr draft-assist` read from local SQLite and call the configured LLM provider only for generation. Relationship memory is local-first: contact style, relationship summaries, commitments, and user voice data live in SQLite and are exposed through daemon surfaces and sender/profile views.

Draft assist adds relationship data as weak background context. The current thread and user instruction always override it, and prompts explicitly tell the model not to invent familiarity. Generated drafts include deterministic humanizer scoring and voice-match metadata so clients can warn when output sounds robotic or drifts from the known voice profile.

Useful checks:

```bash
mxr status
mxr semantic status
mxr doctor --semantic-status
```

## Runtime identity

mxr keeps installed, development, and demo runtimes separate. Release
builds use the `mxr` instance; debug `cargo run` uses `mxr-dev`; demo
mode uses `mxr-demo`. The instance scopes config, data, SQLite, Tantivy,
semantic caches, sockets, bridge files, token roots, and non-production
credential refs.

Check what you are touching before running destructive commands:

```bash
mxr config path
mxr status --format json
```

Use `MXR_INSTANCE=<name>` only when you intentionally want a custom
runtime identity.

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

**Search is the universal selector.** Every list/search command writes one ID per line under `--format ids`; every read or mutate command takes an ID. Compose with anything:

```bash
mxr search '<query>' --format ids | xargs -I{} <command> {}
```

For mxr-on-mxr chaining, prefer `--search` directly — daemon-native, snapshot-consistent, with `--first` and `--limit N` modifiers:

```bash
mxr cat --search 'from:alice' --first              # body of the latest match
mxr summarize --search 'is:unread' --first         # LLM summary of the most recent unread thread
mxr archive --search 'from:no-reply older_than:30d' --yes
mxr search "is:unread from:buildkite" --format json | jq -r '.[].message_id'
```

`--search` is on every read command that takes an ID (`cat`, `thread`, `headers`, `summarize`, `draft-assist`, `open`, `attachments list`) and on every mutation. See [`docs/recipes`](https://mxr-mail.vercel.app/guides/recipes/) for the full cookbook (fzf / jq / xargs / cron / agent prompts).

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

## Web interface

After `mxr setup` and `mxr daemon`, launch the browser UI with:

```bash
mxr web
```

This starts the local web bridge in the background, opens
`http://mxr.localhost:42829` in your default browser, then returns control to
the terminal. Run `mxr web` again to reopen the same bridge, or
`mxr web stop` to stop it. On the same machine the SPA self-authenticates
against the daemon — no token paste required.

If the port is already in use, `mxr web` fails with a conflict message and
best-effort process details so the local URL stays stable. Pass
`--auto-port` to try the next free port. The bound port is also written to
`<config_dir>/bridge-port` for scripts and the Vite dev proxy.

Useful flags:

- `--port N` sets the fixed local web port.
- `--auto-port` tries the next available port on conflict.
- `--no-open` prints the URL without opening a browser.
- `--print-url` prints the URL without opening a browser.
- `--foreground` runs the bridge in the terminal for debugging.
- `--remote-host H` is for manually configured remote bridges; SSH/Tailscale tunnels are the supported remote path for now.

Troubleshooting:

- Blank page: confirm the daemon is running with `mxr status`.
- 401 / unauthorized: only happens for remote bridges or when `[bridge].auto_local_token = false`; the SPA redirects to `/settings/token`; paste the `bridge-token` file from the active profile config dir.
- Stale UI after upgrading: hard-refresh with Cmd-Shift-R or Ctrl-F5.

For strict-bearer setups (multi-user machines, etc.), set
`[bridge].auto_local_token = false` in the file printed by
`mxr config path` to disable the same-machine handshake.

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
