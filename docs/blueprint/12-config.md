# mxr — Configuration

## Config file location

Following XDG:

- config: `$XDG_CONFIG_HOME/mxr/config.toml`
- data: `$XDG_DATA_HOME/mxr/`
- runtime: `$XDG_RUNTIME_DIR/mxr/mxr.sock`

macOS equivalents live under `~/Library/Application Support/mxr/`.

Data dir highlights:

- `mxr.db` — SQLite
- `search_index/` — Tantivy
- `models/` — local semantic model cache
- `attachments/` — downloaded attachments

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
```

## `[search]`

- `default_sort`
- `max_results`
- `default_mode`

`default_mode` controls the search mode used when a request does not specify one explicitly.

## `[search.semantic]`

### `enabled`

Semantic retrieval toggle.

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

This is deliberate. `enabled = false` does **not** mean “no semantic-ready data exists.” It means “do not generate/use embeddings right now.”

### `auto_download_models`

- `true`: first enable/profile use may download the selected local model automatically
- `false`: semantic commands/search will fail until the active local model is already installed

### `active_profile`

Current supported values:

- `bge-small-en-v1.5`
- `multilingual-e5-small`
- `bge-m3`

This is the local embedding profile mxr will use when semantic search is enabled.

### `max_pending_jobs`

Currently parsed and persisted in config, but not yet enforced by a separate semantic job queue. Keep it as configuration shape, not as an active runtime guarantee today.

### `query_timeout_ms`

Currently parsed and persisted in config, but the dense search path does not yet enforce a separate timeout budget from this value. Document it as reserved/currently inactive rather than pretending it is wired.

## What happens when semantic is enabled later

If you sync mail for a while with `enabled = false`, mxr still stores semantic chunks for changed messages.

When you later enable semantic search:

1. mxr installs the active local profile if needed
2. mxr backfills missing chunks for messages that do not already have them
3. mxr generates embeddings from stored chunks
4. mxr rebuilds the active ANN index

This is cheaper than rebuilding chunk text for every message from scratch.

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
- fallback: encrypted file in data dir

## Resolution order

Later wins:

1. built-in defaults
2. `config.toml`
3. environment
4. CLI flags

`mxr config` shows resolved values.
