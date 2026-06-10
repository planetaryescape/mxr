---
title: Config
description: Top-level mxr configuration model.
---

## Location

mxr resolves a runtime identity first, then places config, data, socket,
token, and bridge files under that identity. Release builds default to
`mxr`; debug `cargo run` defaults to `mxr-dev`; demo mode uses
`mxr-demo`. Override with `MXR_INSTANCE` only when you want an explicit
profile.

To inspect the resolved path:

```bash
mxr config path
mxr status --format json
```

For the active `<instance>`, the default roots are:

| Kind | Linux / XDG | macOS |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/<instance>/config.toml` | `~/Library/Application Support/<instance>/config.toml` |
| Data | `$XDG_DATA_HOME/<instance>/` | `~/Library/Application Support/<instance>/` |
| Socket | `$XDG_RUNTIME_DIR/<instance>/mxr.sock` | `~/Library/Application Support/<instance>/mxr.sock` |

`MXR_CONFIG_DIR`, `MXR_DATA_DIR`, `MXR_TOKEN_DIR`, `MXR_SOCKET_PATH`,
`MXR_BRIDGE_TOKEN_PATH`, and `MXR_BRIDGE_PORT_PATH` override individual
paths when needed.

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
[notifications.chimes]
[llm]
[llm.overrides.answer_coverage]
[safety.recipients]
[safety.tone]
[deliveries]

[accounts.work]
```

### Annotated example

A working config that exercises every section. Drop into the path
that `mxr config path` prints, then adjust:

```toml
[general]
editor = "nvim"                    # or "code -w", "subl -w", "$EDITOR"
default_account = "personal"       # used when commands omit --account
sync_interval = 60                 # seconds; how often the daemon polls
hook_timeout = 30                  # seconds; max time a shell hook may run
safety_policy = "full"             # full | restricted | draft-only | read-only
attachment_dir = "~/mxr/attachments" # optional internal attachment cache override
download_dir = "~/Downloads"        # default destination for user-initiated saves

[render]
html_command = "w3m -dump"         # how plain-text HTML view is rendered
reader_mode = true                 # default to reader on open
show_reader_stats = true
html_remote_content = true         # allow remote images in HTML view; tracking pixels are still stripped

[search]
default_sort = "date_desc"
max_results = 200
default_mode = "lexical"           # lexical | hybrid | semantic

[search.semantic]
enabled = true
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
port = 42829                       # stable local web URL port
cors_allowlist = []
host_allowlist = []
auto_local_token = true            # loopback callers can auto-fetch the token
# token_path = "/absolute/path/to/custom-bridge-token"

[notifications.chimes]
enabled = false                    # opt-in audio feedback from the daemon
volume = 0.35                      # 0.0 .. 1.0
new_mail = "bell"
sent = "sent"
archived = "archive"
trashed = "thud"
spam = "alert"
snoozed = "pop"
unsnoozed = "glass"
reminder = "bell"
error = "alert"

[llm]
enabled = false
base_url = "http://localhost:11434/v1"   # Ollama default
model = "qwen2.5:3b-instruct"
api_key_env = ""                          # name of env var; empty for Ollama / LM Studio
context_window = 8192
request_timeout_secs = 120

[llm.overrides.answer_coverage]
# Optional per-feature LLM override. Same shape as [llm] above. Useful
# when answer-coverage benefits from a different model than the default
# (e.g. a smarter cloud model for the only LLM-backed safety check).
# enabled = true
# model = "gpt-5-mini"

[safety.recipients]
internal_domains = ["company.com"]        # paired with internal markers in body
sensitive_domains = ["competitor.com"]    # always Blocker
warn_on_first_time_external = true        # warn on never-seen-before domains

[safety.tone]
formality_delta_threshold = 0.25          # 0.0 = always warn, 1.0 = never

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
| `sync_interval` | integer (seconds) | `60` | How often the daemon polls each account |
| `hook_timeout` | integer (seconds) | `30` | Max wall-time for a shell hook |
| `attachment_dir` | path | `<data_dir>/attachments` | Internal cache for opened/inline attachments |
| `download_dir` | path | platform downloads dir | Default destination for user-initiated attachment saves |
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

"Read-only" covers every request that doesn't change state: listing and
reading mail and threads, search and aggregation, exports, draft and reply
*previews*, local AI analysis (summaries, briefings, sender/relationship
profiles, humanizer scoring), and read access to the calendar-invite list,
the activity log, saved searches, and saved activity filters. Requests that
fetch remote content (HTML image assets, attachments) are **not** read-only
— they trigger network egress, so an agent under `read-only` can't be used
to load tracking pixels. Every request is assigned its category by a single
exhaustive classifier in the daemon, so a new request type can never slip
through a policy by being forgotten in an allowlist.

The TUI greys out mutation actions when the policy disallows them.

## `agents.profiles`

Profiles constrain non-human daemon clients by IPC origin. The daemon selects
`[agents.profiles.agent]` for IPC messages with source `agent` and
`[agents.profiles.mcp]` for messages from `mxr mcp serve`. If the matching
profile is missing, the daemon rejects the request before any provider call.

```toml
[agents.profiles.agent]
safety_policy = "draft-only"
allowed_accounts = ["work"]
allow_send = false
allow_destructive = false

[agents.profiles.mcp]
safety_policy = "read-only"
allowed_accounts = ["personal@example.com"]
allow_send = false
allow_destructive = false
```

Fields:

| Key | Meaning |
|-----|---------|
| `safety_policy` | Same enum as `[general].safety_policy`; default profile value is `read-only` |
| `allowed_accounts` | Account keys, account ids, or emails this origin may touch |
| `allow_send` | Required for `SendStoredDraft`, scheduled sends, and non-dry-run RSVP sends |
| `allow_destructive` | Required for mutations outside read/draft/send buckets |

MCP tools also require explicit `confirm=true` before send or mutation tools
apply changes.

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

The `fake` provider is a deterministic in-memory adapter used by `mxr demo`
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

`credential_source = "custom"` is the official Gmail v1 recommendation: use
your own Google Cloud project's client ID/secret. `bundled` uses the OAuth
client mxr ships when present; treat it as an unverified fallback that may show
Google's warning screen.

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
| `show_reader_stats` | bool | `true` | Display word/reading-time on opened messages |
| `html_remote_content` | bool | `true` | Allow remote image fetches in HTML view; tracking pixels are still stripped |

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
enabled = true
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
  - hybrid/semantic search falls back to lexical ranking if dense retrieval is unavailable or errors

The built-in default is `enabled = true`, but `default_mode` remains `lexical`. Semantic retrieval is used only for requests that ask for `hybrid` or `semantic` mode.

Current profiles:

- `bge-small-en-v1.5`
- `multilingual-e5-small`
- `bge-m3`

Notes:

- embeddings stay local
- OCR is not used for semantic indexing
- semantic readiness is opportunistic and must not block sync, read, send, or lexical search
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

## `activity`

Controls the user-activity log (`user_activity` table). Strictly local; never transmitted off-device. See the [Activity Log guide](/guides/activity-log/) for the full design.

```toml
[activity]
enabled = true
track_link_clicks = false       # opt-in; URLs reveal a lot
track_subjects = true
track_recipient_handles = true
track_search_queries = true

[activity.retention]
ephemeral_days = 30
standard_days = 90
important_days = 365
```

- `enabled` — global switch. When `false`, the recorder is spawned but every `record()` call is a no-op. `MXR_ACTIVITY=off` is the env-var equivalent.
- `track_link_clicks` — record `link.click` rows with the URL in `context_json`. Default `false` because URL history is sensitive.
- `track_subjects` — keep email subjects in `context_json`. Default `true`. Flip off if you'd rather not retain message subjects in the audit trail.
- `track_recipient_handles` — keep `name`/`email` recipient blocks in `context_json`. Default `true`.
- `track_search_queries` — keep search query text verbatim. Default `true`. Flip off for high-sensitivity work.
- `retention.ephemeral_days` / `standard_days` / `important_days` — daily prune sweep hard-deletes rows older than this. Defaults 30 / 90 / 365.

Verify it's wired:

```bash
mxr activity status                  # paused state + recent-30d row count
mxr activity prune --before 30d --dry-run
```

Hard kill at startup:

```bash
MXR_ACTIVITY=off mxr daemon          # recorder is spawned but writes are dropped
```

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
- `port` — fixed TCP port for the local web URL (default `42829`). `mxr web` fails on `EADDRINUSE` by default and can opt into walking up with `--auto-port`. The actual bound port is written to `<config_dir>/bridge-port` for clients to discover.
- `cors_allowlist` — additional origins (defaults already cover loopback).
- `host_allowlist` — additional hostnames for non-loopback binds.
- `auto_local_token` — when `true` (default), `GET /api/v1/auth/local-token` returns the bridge token to callers whose TCP peer is a loopback IP. Lets the web SPA bootstrap on the same machine without a paste prompt. Set to `false` for paranoid setups that want strict bearer auth even on loopback. Non-loopback peers never receive the token regardless of this setting.
- `token_path` — path to the auth token file. Omit it to use `<config_dir>/bridge-token` for the active runtime identity.

## `notifications.chimes`

Daemon-side audio feedback for local events and successful actions. Chimes
are off by default. Manage them without restarting the daemon:

```bash
mxr chimes status --format json
mxr chimes enable
mxr chimes set archived glass
mxr chimes test archived
mxr chimes disable
```

Supported events: `new-mail`, `sent`, `archived`, `trashed`, `spam`,
`snoozed`, `unsnoozed`, `reminder`, `error`.

Supported sounds: `none`, `bell`, `glass`, `pop`, `sent`, `archive`,
`thud`, `alert`.

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
allow_cloud_relationship_data = false
```

The API key is read from `api_key_env` at runtime and is never persisted
to the config file. Empty `api_key_env` means no `Authorization` header
is sent — correct for Ollama and LM Studio.

`allow_cloud_relationship_data = false` blocks relationship/profile context
from being sent to non-local LLM endpoints. Set it to `true` only when you want
cloud providers to receive that context for relationship-aware summaries,
briefings, and draft assistance.

Use `mxr llm status` to inspect the running provider, model, context
window, timeout, and whether the configured API-key environment variable
is present. Daemon config reloads rebuild the LLM provider, so changing
`base_url`, `model`, or `api_key_env` is reflected after reload without
restarting the process.

The web app's Settings > LLM panel edits this same section through the
daemon and reloads the provider immediately after save.

### Per-feature overrides

Every LLM-backed feature can override `[llm]` independently. Useful
when one feature benefits from a different model — for example, you
might run thread summaries through a local 3B model but want
answer-coverage on a smarter cloud model.

```toml
[llm.overrides.answer_coverage]
enabled = true
base_url = "https://api.openai.com/v1"
model = "gpt-5-mini"
api_key_env = "OPENAI_API_KEY"
```

Any field omitted from an override falls back to the top-level `[llm]`
section. Feature keys: `summarize`, `draft_assist`, `draft_new`,
`draft_refine`, `voice_match`, `answer_coverage`, `commitments`,
`delivery_extraction` (confirm/enrich for [delivery tracking](/guides/deliveries/)).

## `safety`

Pre-send safety pipeline tuning. See the
[Pre-send safety guide](/guides/pre-send-safety/) for the full check
inventory and the override-token flow.

### `safety.recipients`

```toml
[safety.recipients]
internal_domains = ["company.com"]
sensitive_domains = ["competitor.com"]
warn_on_first_time_external = true
```

| Key | Type | Default | Purpose |
|---|---|---|---|
| `internal_domains` | string[] | `[]` | Domains flagged as internal. Body markers like `INTERNAL` or `CONFIDENTIAL` paired with a recipient OUTSIDE this list trigger a Blocker. |
| `sensitive_domains` | string[] | `[]` | Any recipient at one of these domains is a Blocker. Common use: known competitors, regulators, journalists, ex-employers — anywhere a misfire would be expensive. |
| `warn_on_first_time_external` | bool | `false` | Warn when sending to a domain you have no prior history with. Cheap nudge that catches "did I get the right alice?" cases. |

### `safety.tone`

```toml
[safety.tone]
formality_delta_threshold = 0.25
```

| Key | Type | Default | Purpose |
|---|---|---|---|
| `formality_delta_threshold` | float [0.0, 1.0] | `0.25` | How different the draft's formality score must be from the recipient's baseline before the warning fires. `0.0` = always warn (every send vs. baseline). `1.0` = never warn. Lower it if you want stricter checking, raise it to mute noise. |

Tone match needs at least 3 prior messages to a recipient — fewer than
that and the check silently skips (no point warning when there's no
stable baseline to compare against).

## `deliveries`

Controls package/shipment detection. See the [Deliveries guide](/guides/deliveries/) for the full pipeline.

```toml
[deliveries]
enabled = true
```

- `enabled` — scan new mail for deliveries during the post-sync pass. Default `true`. Detection is local and cheap; turn it off to skip it entirely. The optional LLM confirm/enrich step is gated separately by `[llm]` (and its [`delivery_extraction` override](#per-feature-overrides)) — with no LLM configured, detection still runs heuristics plus checksum-valid tracking numbers.

```bash
mxr deliveries scan --since-days 30 --dry-run    # preview detection without writing
mxr deliveries list                              # what's been found
```

## Custom keybindings

Default TUI keybindings can be overridden via `keys.toml` next to the active config file. Run `mxr config path` and put `keys.toml` in that directory. The file is split into three view contexts that match the TUI's input router:

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
