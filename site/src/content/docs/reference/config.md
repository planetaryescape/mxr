---
title: Config
description: Top-level mxr configuration model.
---

## Location

mxr uses a TOML config file under the standard config directory for your platform.

To inspect the resolved path:

```bash
mxr config path
```

## Top-level sections

```toml
[general]
[render]
[search]
[search.semantic]
[snooze]
[logging]
[appearance]
[bridge]
[llm]

[accounts.work]
```

### Annotated example

A working config that exercises every section. Drop into the path
that `mxr config path` prints, then adjust:

```toml
[general]
editor = "nvim"                    # or "code -w", "subl -w", "$EDITOR"
default_account = "personal"       # used when commands omit --account
sync_interval = 300                # seconds; how often the daemon polls
hook_timeout = 30                  # seconds; max time a shell hook may run
safety_policy = "full"             # full | restricted | draft-only | read-only
attachment_dir = "~/Downloads/mxr"

[render]
html_command = "w3m -dump"         # how plain-text HTML view is rendered
reader_mode = true                 # default to reader on open
show_reader_stats = false
html_remote_content = false        # never fetch remote images by default

[search]
default_sort = "date_desc"
max_results = 200
default_mode = "lexical"           # lexical | hybrid | semantic

[search.semantic]
enabled = false
auto_download_models = true
active_profile = "bge-small-en-v1.5"
max_pending_jobs = 256
query_timeout_ms = 1500

[snooze]
morning_hour = 9
evening_hour = 18
weekend_day = "saturday"
weekend_hour = 10

[bridge]
enabled = true
bind = "127.0.0.1"
port = 42829                       # bridge walks up to next free port on EADDRINUSE
cors_allowlist = []
host_allowlist = []
auto_local_token = true            # loopback callers can auto-fetch the token
token_path = "~/.config/mxr/bridge-token"

[llm]
enabled = false
base_url = "http://localhost:11434/v1"   # Ollama default
model = "qwen2.5:3b-instruct"
api_key_env = ""                          # name of env var; empty for Ollama / LM Studio
context_window = 8192
request_timeout_secs = 120

[accounts.personal]
name = "Personal"
email = "me@example.com"
enabled = true                            # per-account on/off; survives across daemon restarts
[accounts.personal.sync]
type = "gmail"
credential_source = "bundled"             # bundled | custom
[accounts.personal.send]
type = "gmail"

[accounts.work]
name = "Work"
email = "me@work.example.com"
enabled = true
[accounts.work.sync]
type = "imap"
host = "imap.work.example.com"
port = 993
username = "me@work.example.com"
auth_required = true
use_tls = true
[accounts.work.send]
type = "smtp"
host = "smtp.work.example.com"
port = 587
username = "me@work.example.com"
auth_required = true
use_tls = true
```

## `general`

| Key | Type | Default | Purpose |
|-----|------|---------|---------|
| `editor` | string | `$EDITOR` | Used by `mxr compose`, `mxr config edit`, and the TUI compose flow |
| `default_account` | string | first enabled | Account selected when commands omit `--account` |
| `sync_interval` | integer (seconds) | `300` | How often the daemon polls each account |
| `hook_timeout` | integer (seconds) | `30` | Max wall-time for a shell hook |
| `attachment_dir` | path | `~/Downloads/mxr` | Where `mxr attachments download` lands files |
| `safety_policy` | enum | `full` | Daemon-wide guardrail — see below |

### `safety_policy`

Caps which IPC categories the daemon will service. Useful when running
mxr inside a CI sandbox or behind an agent you don't fully trust:

| Value | Allows |
|-------|--------|
| `full` | All categories (default) |
| `restricted` | All except destructive / mutation IPCs |
| `draft-only` | Read + compose; no provider sends |
| `read-only` | Read-only IPCs only — no mutations, no sends |

The TUI greys out mutation actions when the policy disallows them.

## `accounts`

Each `[accounts.<key>]` is a TOML subtable. Required:

- `name` — display name in the sidebar
- `email` — primary address
- `enabled` (default `true`) — when `false` the daemon skips the account
  on sync. Useful for keeping a dormant account configured without
  paying its sync cost.

Sub-tables:

- `[accounts.<key>.sync]` — inbound provider; `type = "gmail" | "imap" | "outlook_personal" | "outlook_work" | "fake"`
- `[accounts.<key>.send]` — outbound provider; `type = "gmail" | "smtp" | "outlook_personal" | "outlook_work" | "fake"`

The `fake` provider is a deterministic in-memory adapter used by `mxr setup --demo`
and the test suite — useful when you want a dry-run install without real credentials.

### Gmail sync provider

```toml
[accounts.personal.sync]
type = "gmail"
credential_source = "bundled"     # bundled | custom
client_id = "..."                 # only when credential_source = "custom"
client_secret = "..."             # only when credential_source = "custom"
token_ref = "gmail:personal"      # keychain entry name; auto-set
```

`credential_source = "bundled"` uses the OAuth client mxr ships. `custom`
accepts a Google Cloud project's client ID/secret
when you'd rather not depend on the bundled credentials.

### IMAP sync provider

```toml
[accounts.work.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "me@example.com"
auth_required = true              # set to false for relays that pre-authenticate
use_tls = true
password_ref = "imap:work"
```

### SMTP send provider

Same shape as the IMAP block but with the SMTP host/port and
`password_ref`. `auth_required = false` is the rare relay case where
the SMTP server accepts the message without credentials.

## `render`

| Key | Type | Default | Purpose |
|-----|------|---------|---------|
| `html_command` | string | `w3m -dump` | Shell command to render HTML to plain text |
| `reader_mode` | bool | `true` | Default to reader mode when opening a message |
| `show_reader_stats` | bool | `false` | Display word/reading-time on opened messages |
| `html_remote_content` | bool | `false` | Allow remote image fetches; off by default for privacy |

## `search`

- `default_sort`
- `max_results`
- `default_mode`

Example:

```toml
[search]
default_sort = "date_desc"
max_results = 200
default_mode = "lexical"
```

`default_mode` may be `lexical`, `hybrid`, or `semantic`.

## `search.semantic`

```toml
[search.semantic]
enabled = false
auto_download_models = true
active_profile = "bge-small-en-v1.5"
max_pending_jobs = 256
query_timeout_ms = 1500
```

- `enabled`
- `auto_download_models`
- `active_profile`
- `max_pending_jobs`
- `query_timeout_ms`

Current runtime meaning:

- `enabled = false`
  - sync still prepares semantic chunks for changed messages
  - embeddings are not generated
  - dense retrieval stays off
- `enabled = true`
  - mxr installs the active local model if needed
  - generates embeddings from stored chunks
  - rebuilds/uses the dense ANN index

Current profiles:

- `bge-small-en-v1.5`
- `multilingual-e5-small`
- `bge-m3`

Notes:

- embeddings stay local
- OCR is not used for semantic indexing
- `max_pending_jobs` and `query_timeout_ms` are currently parsed config fields, not active runtime guarantees yet

## `snooze`

- `morning_hour`
- `evening_hour`
- `weekend_day`
- `weekend_hour`

## `logging`

- `level`
- `max_size_mb`
- `max_files`
- `stderr`
- `event_retention_days`

## `appearance`

- `theme`
- `sidebar`
- `date_format`
- `date_format_full`
- `subject_max_width`

## `bridge`

HTTP bridge configuration.

- `enabled` — start the bridge alongside the daemon (default `true`).
- `bind` — bind address (default `127.0.0.1`).
- `port` — preferred TCP port (default `42829`). On `EADDRINUSE` the bridge walks up to the next free port (up to 32 attempts). The actual bound port is written to `<config_dir>/bridge-port` for clients to discover.
- `cors_allowlist` — additional origins (defaults already cover loopback).
- `host_allowlist` — additional hostnames for non-loopback binds.
- `auto_local_token` — when `true` (default), `GET /api/v1/auth/local-token` returns the bridge token to callers whose TCP peer is a loopback IP. Lets the web SPA bootstrap on the same machine without a paste prompt. Set to `false` for paranoid setups that want strict bearer auth even on loopback. Non-loopback peers never receive the token regardless of this setting.
- `token_path` — path to the auth token file (default `~/.config/mxr/bridge-token`).

## `llm`

Optional LLM features (thread summarisation, draft assist). Disabled
by default. Speaks the OpenAI Chat Completions schema, so any of these
backends works:

- **Ollama** (local): `http://localhost:11434/v1`, no API key
- **LM Studio** (local): `http://localhost:1234/v1`, no API key
- **OpenAI**: `https://api.openai.com/v1`, `OPENAI_API_KEY`
- **Groq**: `https://api.groq.com/openai/v1`, `GROQ_API_KEY`
- **OpenRouter**: `https://openrouter.ai/api/v1`, `OPENROUTER_API_KEY`
- **Together AI**, **Mistral La Plateforme**, **Anthropic via OpenAI proxy**, etc.

```toml
[llm]
enabled = true
base_url = "http://localhost:11434/v1"
model = "qwen2.5:3b-instruct"
api_key_env = ""                    # name of the env var; empty = no auth header
context_window = 8192
request_timeout_secs = 120
```

The API key is read from `api_key_env` at runtime and is never persisted
to the config file. Empty `api_key_env` means no `Authorization` header
is sent — correct for Ollama and LM Studio.

Use `mxr llm status` to inspect the running provider, model, context
window, timeout, and whether the configured API-key environment variable
is present. Daemon config reloads rebuild the LLM provider, so changing
`base_url`, `model`, or `api_key_env` is reflected after reload without
restarting the process.

The web app's Settings > LLM panel edits this same section through the
daemon and reloads the provider immediately after save.

## Custom keybindings

Default TUI keybindings can be overridden via `~/.config/mxr/keys.toml` (or wherever `mxr config path` resolves). The file is split into three view contexts that match the TUI's input router:

```toml
[mail_list]
"j" = "move_down"
"k" = "move_up"
"6" = "open_tab_6"          # bind Analytics to a digit (default is unbound)
"Ctrl-Shift-A" = "open_tab_6"
"e" = "archive"

[message_view]
"R" = "toggle_reader_mode"
"H" = "toggle_html_view"

[thread_view]
"E" = "export_thread"
```

Anything you _don't_ list keeps its default. To remove a binding, set it to `""`.

### Key-string grammar

- Bare characters: `"j"`, `"E"`, `"#"`, `";"`, `","`
- Modifier prefixes: `Ctrl-` and `Ctrl-Alt-`; bare uppercase letters imply Shift.
- Special keys: `Enter`, `Esc`, `Escape`, and `Tab`.
- Chords: concatenate (no separator) — `"gg"`, `"gi"`, `"zz"`. Chords are limited to two characters today.

### Action names

Bindings reference actions by name. Most TUI actions have a serializable name registered in `crates/tui/src/keybindings.rs`. As of today, ~47 of the ~122 internal `Action` variants are exposed as user-rebindable. The reachable names cover the common surfaces: navigation, mail mutations, screen switching (`open_tab_1`–`open_tab_6`), reader-mode toggles, and the standard chords.

If you bind a string the system doesn't recognise, the daemon logs a warning at startup and silently keeps the default for that key. Run with `mxr daemon --foreground` while iterating to see the warnings.

### Reading what's bound

```bash
mxr --help                 # command surface
# In the TUI:
?                          # context-aware help modal
Ctrl-p                     # command palette searches by action name
```

The palette is the fastest way to discover a binding while you're using the TUI; the [keybindings reference](/reference/keybindings/) is the printable cheat sheet.

## Notes

- Runtime account inventory is not identical to config entries.
- Gmail browser-auth accounts may exist at runtime without being editable config-backed entries.
- IMAP/SMTP entries are the main editable config-backed account type.
