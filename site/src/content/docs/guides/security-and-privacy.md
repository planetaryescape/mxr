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

**IMAP/SMTP passwords are stored on disk, keychain-optional.** They live in
`<config_dir>/secrets.toml` — a plaintext TOML file at mode `0600` (owner
read/write only), keyed by `password_ref` + username. This is the same model
normal CLIs use (`~/.aws/credentials`, `~/.config/gh/hosts.yml`): the deliberate
tradeoff is plaintext-at-rest, protected only by filesystem permissions rather
than OS encryption. mxr chose disk-first because an ad-hoc-signed release binary
loses its OS-keychain read access on every upgrade — which used to hard-fail
daemon startup for password accounts. A `0600` file survives upgrades untouched
and is readable only by your own processes.

The OS-native secret store is an **optional fallback**:

- **macOS**: Keychain (Keychain Access)
- **Linux**: Secret Service (e.g. GNOME Keyring, KWallet)

On the first read after upgrading from a keychain-only version, an IMAP/SMTP
credential found in the keychain is automatically migrated (mirrored) into
`secrets.toml` and served from disk thereafter. `mxr accounts add` and
`mxr accounts repair NAME` write `secrets.toml` (disk-authoritative) with a
best-effort keychain mirror that never blocks the operation. The on-disk
`config.toml` only references credentials by `password_ref`; it never stores the
password itself.

Gmail OAuth refresh tokens are stored in the OS keychain with a private disk
fallback under the active token dir, so a noninteractive keychain failure does
not strand an otherwise valid account. Outlook OAuth tokens are JSON files under
the active token dir (`<data_dir>/tokens` by default, `MXR_TOKEN_DIR` when set).

`secrets.toml` lives in the config dir, so `mxr reset` and `mxr reset --hard`
preserve it — your credentials survive a runtime-state wipe. Override its
location with `MXR_SECRETS_PATH`. Protect it like any dotfile secret: keep the
`0600` mode and do not commit it to version control.

### Backup and restore

mxr does not run a hosted backup service. Your local profile is the
recovery boundary.

To find the active paths:

```bash
mxr status --format json | jq -r '.config_path, .data_dir'
```

For a clean backup, stop mxr processes first, then copy:

- the config directory containing `config.toml` **and `secrets.toml`**
  (your IMAP/SMTP passwords — keep its `0600` mode and treat it as
  sensitive)
- the data directory containing `mxr.db`, `attachments/`, `logs/`,
  `tokens/`, `search_index/`, and `models/`
- any OS keychain entries for Gmail if you are moving to a different
  machine (IMAP/SMTP secrets travel in `secrets.toml`)

`search_index/` and `models/` are rebuildable, so you can omit them from
space-constrained backups. Keep `mxr.db`, `attachments/`, `tokens/`, and
`config.toml` together. Do not copy `mxr.db` while the daemon is writing
unless your filesystem backup tool provides a consistent snapshot.

To restore, install the same or a newer mxr version, stop the daemon,
put the config/data directories back at the resolved paths (or set
`MXR_CONFIG_DIR` / `MXR_DATA_DIR`). If you restored `secrets.toml`, your
IMAP/SMTP passwords are already in place; otherwise run
`mxr accounts repair NAME` (and restore any Gmail keychain entries), then
run:

```bash
mxr doctor
mxr sync
```

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
