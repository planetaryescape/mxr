# mxr — Configuration

## Runtime identity and file locations

mxr resolves a runtime identity first, then derives local paths from it.
Release builds default to `mxr`, debug builds default to `mxr-dev`, and
`mxr demo` uses `mxr-demo`. Override with `MXR_INSTANCE` when you need an
explicit profile.

Inspect the active paths before editing or deleting anything:

```bash
mxr config path
mxr status --format json
```

Following XDG, for the active `<instance>`:

- config: `$XDG_CONFIG_HOME/<instance>/config.toml`
- data: `$XDG_DATA_HOME/<instance>/`
- runtime: `$XDG_RUNTIME_DIR/<instance>/mxr.sock`

macOS config, data, and socket paths live under
`~/Library/Application Support/<instance>/`.

Data dir highlights:

- `mxr.db` — SQLite
- `search_index/` — Tantivy
- `models/` — local semantic model cache
- `attachments/` — downloaded attachments
- `tokens/` — OAuth disk fallback / Outlook token files for this instance

Bridge token and port files live in the active config dir by default:

- `<config_dir>/bridge-token`
- `<config_dir>/bridge-port`

Production keeps legacy keychain service names for compatibility.
Non-production instances scope credential refs and Gmail OAuth keychain
services by instance so `cargo run` cannot read or overwrite the
installed daemon's credentials.

## Example config

```toml
[general]
editor = "nvim"
default_account = "personal"
sync_interval = 60
attachment_dir = "~/mxr/attachments"
download_dir = "~/Downloads"

[render]
reader_mode = true
show_reader_stats = true

[search]
default_sort = "date_desc"
max_results = 200
default_mode = "lexical"   # lexical | hybrid | semantic

[search.semantic]
enabled = true
auto_download_models = true
active_profile = "bge-small-en-v1.5"
# Reserved/internal: parsed and persisted, not active tuning knobs.
max_pending_jobs = 256
query_timeout_ms = 1500

[notifications.chimes]
enabled = false
volume = 0.35
new_mail = "bell"
sent = "sent"
archived = "archive"
trashed = "thud"
spam = "alert"
snoozed = "pop"
unsnoozed = "glass"
reminder = "bell"
error = "alert"
```

## `[search]`

- `default_sort`
- `max_results`
- `default_mode`

`default_mode` controls the search mode used when a request does not specify one explicitly.

## `[search.semantic]`

### `enabled`

Semantic retrieval toggle. The built-in default is `true`, but `default_mode` remains `lexical`; dense retrieval is used only when a request asks for `hybrid` or `semantic`.

Current behavior:

- `false`
  - sync still prepares semantic chunks for changed messages
  - embeddings are not generated
  - dense retrieval is off
  - lexical search keeps working normally
- `true`
  - mxr installs the active local profile if needed
  - generates embeddings from stored chunks
  - rebuilds/uses the dense ANN index
  - falls back to lexical results if dense retrieval is unavailable or errors

This is deliberate. `enabled = false` does **not** mean “no semantic-ready data exists.” It means “do not generate/use embeddings right now.”

Default-on semantic is opportunistic. It must not block sync, read, send, or lexical search. Builds without the local semantic backend behave as degraded lexical-first builds even when the config value is `true`.

### `auto_download_models`

- `true`: first enable/profile use may download the selected local model automatically
- `false`: profile activation/reindex may fail until the active local model is already installed; hybrid/semantic search should fall back to lexical ranking on backend errors

### `active_profile`

Current supported values:

- `bge-small-en-v1.5`
- `multilingual-e5-small`
- `bge-m3`

This is the local embedding profile mxr will use when semantic search is enabled.

### `max_pending_jobs`

Reserved/internal. Currently parsed and persisted in config, but not yet enforced by a separate semantic job queue. Keep it as configuration shape, not as an active runtime guarantee today.

### `query_timeout_ms`

Reserved/internal. Currently parsed and persisted in config, but the dense search path does not yet enforce a separate timeout budget from this value. Document it as reserved/currently inactive rather than pretending it is wired.

## What happens when semantic is enabled later

If you explicitly sync mail for a while with `enabled = false`, mxr still stores semantic chunks for changed messages.

When you later enable semantic search:

1. mxr installs the active local profile if needed
2. mxr backfills missing chunks for messages that do not already have them
3. mxr generates embeddings from stored chunks
4. mxr rebuilds the active ANN index

This is cheaper than rebuilding chunk text for every message from scratch.

Operationally, that means:

- `enabled = false` still keeps semantic-ready chunk text warm
- later enablement is mostly an embedding/profile build step
- lexical search freshness is unaffected by semantic enablement

## Profile install, switching, and reindex

### Install / inspect

```bash
mxr semantic status
mxr semantic profile list
mxr semantic profile install bge-small-en-v1.5
```

### Switch profile

```bash
mxr semantic profile use multilingual-e5-small
```

Current behavior:

- installs the selected local model if needed
- backfills missing chunks if needed
- rebuilds embeddings for the selected profile from stored chunks
- enables semantic search in config

### Full reindex

```bash
mxr semantic reindex
mxr doctor --reindex-semantic
```

Use reindex when:

- chunk extraction rules changed
- attachment extraction behavior changed
- you want a full correctness rebuild for the active profile

Reindex rebuilds chunks from message content, then regenerates embeddings.

## Local model cache and privacy

The intended semantic path is local:

- message content stays on the machine
- model weights are cached locally
- embeddings are stored in local SQLite

No cloud embedding backend is the default path. This task does not change that.

## Enablement example

```toml
[search]
default_mode = "hybrid"

[search.semantic]
enabled = true
auto_download_models = true
active_profile = "bge-small-en-v1.5"
```

Expected first-enable behavior:

- local model install/download if missing
- embedding build from stored chunks
- ANN rebuild

Expected ongoing behavior:

- sync keeps preparing chunks
- active profile embeddings are updated only when semantic is enabled
- `mxr semantic status` shows whether the active profile is actually ready

## Fielded hybrid examples

```bash
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:house of cards" --mode hybrid --explain
mxr search "filename:house of cards" --mode hybrid --explain
```

Dense side intent:

- `body:` -> body chunks
- `subject:` -> header chunks
- `filename:` -> attachment chunks

Lexical side remains literal and field-aware through Tantivy.

## Keybindings and secrets

Keybindings still live in `keys.toml`.

Credentials are never stored raw in `config.toml`.

- Linux: Secret Service / GNOME Keyring / KDE Wallet
- macOS: Keychain
- Gmail OAuth: OS keychain first, with a private disk fallback under the active token dir
- Outlook OAuth: JSON token files under the active token dir
- IMAP/SMTP passwords: OS keychain/keyring via `password_ref`

## Resolution order

Later wins:

1. built-in defaults
2. active `config.toml`
3. environment overrides such as `MXR_EDITOR`, `MXR_SYNC_INTERVAL`, `MXR_DEFAULT_ACCOUNT`, `MXR_ATTACHMENT_DIR`, `MXR_DOWNLOAD_DIR`, and `MXR_SAFETY_POLICY`

Path roots are resolved outside the TOML model:

- `MXR_INSTANCE` picks the runtime identity (`mxr`, `mxr-dev`, `mxr-demo`, or custom)
- `MXR_CONFIG_DIR`, `MXR_DATA_DIR`, `MXR_TOKEN_DIR`, and `MXR_SOCKET_PATH` override individual roots
- `MXR_BRIDGE_TOKEN_PATH` and `MXR_BRIDGE_PORT_PATH` override bridge files
- `MXR_DAEMON_ADDR` selects the client transport for the CLI: `unix://<path>` (default), `tcp://<host:port>` (loopback + token), or `cmd://<command>` (spawn-and-pipe, e.g. `cmd://ssh -T host mxr daemon dial-stdio`). TUI/web/MCP honor `unix://` only. Precedence over `MXR_SOCKET_PATH` and the per-instance default.
- `MXR_DAEMON_TOKEN` supplies the daemon bearer token directly, overriding the token file. Shared by the HTTP bridge and the TCP transport.

`mxr config` shows resolved values.

## Client transports (`[transports]`)

The daemon always serves a Unix domain socket. Additional transports are opt-in
under `[transports]` (phase 5, transport adapters):

```toml
[transports.tcp]
enabled = false        # opt-in; when true, bind a loopback TCP listener
bind = "127.0.0.1"     # loopback only — 127.0.0.1 or ::1; non-loopback is refused
port = 42830           # one above the bridge's 42829
```

The TCP transport has **no implicit peer identity**, so it requires a bearer
token: a client sends an in-band `Authenticate` handshake before any request,
and the daemon rejects everything (and withholds events) with an `auth` error
until it succeeds. Connect with it via
`MXR_DAEMON_ADDR=tcp://127.0.0.1:42830 MXR_DAEMON_TOKEN=… mxr status`.

### Daemon IPC token (distinct from the bridge token)

The TCP transport authenticates with the daemon **IPC** token, resolution
precedence: `MXR_DAEMON_TOKEN` (env, non-empty) **>** a dedicated file at
`<config_dir>/daemon-token` (mode 0600, minted atomically on first daemon
start, `MXR_DAEMON_TOKEN_PATH` override).

This is a **different secret** from the HTTP bridge token
(`<config_dir>/bridge-token`). The bridge hands its token to any loopback
caller via `GET /api/v1/auth/local-token` to bootstrap the web SPA, so reusing
it for raw-IPC auth would let any local process fetch it over HTTP and then
reach the daemon over TCP. The IPC token is never exposed by any HTTP endpoint.
The token comparison in the auth gate is constant-time.

### Per-transport security policy

| Transport | Implicit auth | Required policy |
|---|---|---|
| UDS (`unix://`, always on) | filesystem perms (0600) + peer creds | none extra |
| In-memory (tests, in-process bridge) | in-process | none |
| stdio (`mxr daemon --stdio`, `dial-stdio`, `cmd://`) | inherits the spawner's trust | none extra — the spawner is the authenticator |
| TCP loopback (`tcp://`) | **none** — any local process, plus browsers via DNS-rebinding | bearer token even on loopback; **non-loopback bind refused** (no in-daemon remote — use `dial-stdio` over SSH) |
