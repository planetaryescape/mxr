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

[accounts.work]
```

## `general`

- `editor`
- `default_account`
- `sync_interval`
- `hook_timeout`
- `attachment_dir`

## `accounts`

Each account entry has:

- `name`
- `email`
- `sync`
- `send`

Sync provider types:

- `gmail`
- `imap`

Send provider types:

- `gmail`
- `smtp`

## `render`

- `html_command`
- `reader_mode`
- `show_reader_stats`

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

## Notes

- Runtime account inventory is not identical to config entries.
- Gmail browser-auth accounts may exist at runtime without being editable config-backed entries.
- IMAP/SMTP entries are the main editable config-backed account type.
