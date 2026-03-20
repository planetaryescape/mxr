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
