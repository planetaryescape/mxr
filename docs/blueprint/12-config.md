# mxr — Configuration

## Config file location

Following XDG:

- Config: `$XDG_CONFIG_HOME/mxr/config.toml`
- Data: `$XDG_DATA_HOME/mxr/`
- Runtime: `$XDG_RUNTIME_DIR/mxr/mxr.sock`

Data directory contents:

- `mxr.db` — SQLite
- `search_index/` — Tantivy index
- `models/` — local semantic model cache
- `attachments/` — downloaded attachments

macOS equivalents live under `~/Library/Application Support/mxr/`.

## Example config

```toml
[general]
editor = "nvim"
default_account = "personal"
sync_interval = 60
attachment_dir = "~/mxr/attachments"

[render]
reader_mode = true
show_reader_stats = true

[search]
default_sort = "date_desc"
max_results = 200
default_mode = "lexical"   # lexical | hybrid | semantic

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

[appearance]
theme = "default"
sidebar = true
date_format = "%b %d"
date_format_full = "%Y-%m-%d %H:%M"
subject_max_width = 60
```

## Search config

### `[search]`

- `default_sort`
- `max_results`
- `default_mode`

`default_mode` controls what the daemon uses when a request does not specify a mode explicitly.

### `[search.semantic]`

- `enabled`: turn semantic indexing and retrieval on/off
- `auto_download_models`: allow first-use profile download
- `active_profile`: one of:
  - `bge-small-en-v1.5`
  - `multilingual-e5-small`
  - `bge-m3`
- `max_pending_jobs`: bound semantic indexing backlog
- `query_timeout_ms`: dense search budget

## Profile strategy

Default profile:

- `bge-small-en-v1.5`

Reason:

- smaller download
- faster local inference
- good default for a majority-English mailbox

Opt-in multilingual profile:

- `multilingual-e5-small`

Optional advanced profile:

- `bge-m3`

Rules:

- only the active configured profile is downloaded automatically
- switching to multilingual does not auto-download `bge-m3`
- switching profiles triggers semantic rebuild for the new profile
- existing lexical search keeps working while semantic is unavailable or rebuilding

## Profile cache and lifecycle

Models are cached under:

- Linux: `$XDG_DATA_HOME/mxr/models/`
- macOS: `~/Library/Application Support/mxr/models/`

Operational behavior:

1. User enables semantic search.
2. mxr installs the active profile if missing.
3. Semantic chunks and embeddings are built locally.
4. If the active profile changes later, mxr installs the new profile if needed and rebuilds semantic embeddings.

Embedding rows store the profile identity, so model switches do not corrupt existing semantic data.

## Privacy

The default semantic path is local:

- message content stays on the machine
- embeddings are stored in local SQLite
- model weights are cached locally

If cloud embedding backends are added later, they must be explicit configuration, not the default path.

## Keybindings

Keybindings still live in a separate `keys.toml`. See [08-tui.md](08-tui.md).

## Credentials

Credentials are never stored in `config.toml`.

- Linux: Secret Service / GNOME Keyring / KDE Wallet
- macOS: Keychain
- fallback: encrypted file in data dir

The config stores references only, not raw secrets.

## Resolution order

Later wins:

1. built-in defaults
2. `config.toml`
3. environment
4. CLI flags

`mxr config` shows resolved values.
