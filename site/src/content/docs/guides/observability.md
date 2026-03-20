---
title: Observability
description: Inspect daemon health, logs, events, and diagnostics.
---

## Status and health

```bash
mxr status
mxr status --watch
mxr doctor --check
mxr doctor --check --format json
mxr doctor --index-stats
mxr doctor --store-stats
```

The TUI Diagnostics page surfaces the same runtime information in one place:

- account health
- status
- doctor data
- recent events
- recent logs
- bug-report trigger

## Event stream

```bash
mxr events
mxr events --type sync
mxr events --type sync,rule --format json
```

Use the event stream when you want structured, low-noise runtime signals instead of raw log lines.

## Logs

```bash
mxr logs
mxr logs --level error
mxr logs --purge
```

Text logs live under the mxr data directory. Structured event history is also retained in SQLite and pruned by the configured retention window.

## Notifications and quick counts

```bash
mxr notify
mxr notify --format json
mxr notify --watch
mxr count "label:inbox unread"
```

These commands are intended for status bars, shell prompts, and lightweight monitoring.

## Bug reports

When you need a shareable diagnostic bundle:

```bash
mxr bug-report
mxr bug-report --stdout
mxr bug-report --github
```

Or open Diagnostics in the TUI and trigger bug-report generation there.
