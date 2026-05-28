---
title: Security & Privacy
description: What stays local in mxr, what guardrails exist today, and which safety features are still pending.
---

mxr is local-first by design.

Mail syncs from the provider into SQLite on your machine. Search runs against the local index. The daemon, TUI, CLI, and agent workflows all operate on that local state. There is no hosted mxr relay in the middle.

## What stays local

- SQLite is the canonical store
- Tantivy index is local and rebuildable
- The daemon runs on your machine
- The TUI and CLI talk to the daemon over a local Unix socket
- The web app talks to the same daemon through a loopback HTTP/WebSocket bridge

## What still talks to a provider

- Sync
- Send
- Provider-side mutations like archive, trash, labels, and spam
- Browser handoff for HTML or unsubscribe pages when needed

That is the intended boundary. The network is for talking to your provider, not to a hosted mxr service.

## Guardrails that exist today

- `--dry-run` on risky mutation commands (including `mxr send` and `mxr unsnooze --all`)
- Interactive confirmation for destructive and batch mutation flows unless `--yes` is set
- Undoable mutations: `archive`, `trash`, `spam`, `read`, `read-archive` print a `mutation_id` you can pass to `mxr undo` for ~60s
- Persisted mutation history through `mxr history`
- Event and log views through diagnostics and CLI commands
- Plain-text-first reader mode, with browser escape hatch for original HTML
- Daemon IPC socket permissions are set to `0600` on Unix.
- The bridge requires bearer auth for every authority-bearing route. Only `/api/v1/health`, `/api/v1/auth/local-token`, and `/api/v1/i18n` are unauthenticated bootstrap/read-only routes.
- The bridge checks Host/CORS allowlists and adds frame, content-sniffing, and referrer-policy headers.
- Saved attachments and rendered HTML assets are written `0600` on Unix.
- User-initiated attachment downloads are limited to the configured downloads directory, the current directory, or the system temp directory.
- Remote HTML assets are capped before writing, even when the server omits `Content-Length`.

### Where credentials live

All account credentials are stored in your OS-native secret store, not
in plaintext on disk:

- **macOS**: Keychain (Keychain Access)
- **Linux**: Secret Service (e.g. GNOME Keyring, KWallet)

That includes Gmail OAuth refresh tokens, IMAP passwords, and SMTP
passwords. Gmail keeps a private disk fallback under the active token
dir so a noninteractive keychain failure does not strand an otherwise
valid account. Outlook OAuth tokens are JSON files under the active
token dir (`<data_dir>/tokens` by default, `MXR_TOKEN_DIR` when set).
`mxr accounts repair NAME` re-prompts for keychain-backed credentials
and overwrites the keychain entry. The on-disk `config.toml` only
references IMAP/SMTP credentials by `password_ref`; it never stores the
password itself.

If you previously ran a version older than the Gmail keychain migration,
legacy Gmail token files may be mirrored into the keychain on first
startup. The private disk fallback can remain available by design.

### Bridge and local IPC boundary

The Unix socket is a local user boundary. Any process that can connect
as the same OS user can drive the daemon with that user's authority, so
mxr keeps the socket owner-only and expects it to live under a user-owned
runtime directory.

The HTTP bridge is broader because browsers cannot open Unix sockets. It
binds to loopback by default, uses a bearer token stored under the active
profile config directory, rejects DNS-rebinding-shaped Host headers, and
keeps API docs behind the same auth gate as the rest of the API.

### Attachments and remote content

Attachment names are sanitized before mxr writes local files, including
Windows reserved names such as `CON` and `LPT1`. Explicit downloads are
constrained to safe destination roots. Inline and remote HTML assets live
under mxr's attachment cache, get private file permissions, and remote
asset fetches have a fixed body-size cap.

## Not shipped yet

- First-party MCP server
- Read-only mode for agents
- Draft-only mode for agents
- Account-scoped agent permissions
- Explicit send approval flow
- Config-based blocking of risky commands

Those are real gaps. The current model is "broad CLI with dry-run and history," not "fully permissioned agent mail sandbox."

## Practical advice

- Use `--dry-run` before any batch mutation.
- Use app passwords or provider-specific credentials where your provider recommends them.
- Keep your system keyring clean and scoped to the accounts you use.
- If an agent is involved, prefer workflows that search, read, export, and draft before workflows that mutate.
