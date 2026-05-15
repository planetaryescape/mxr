---
title: Diagnose mxr fast
description: Inspect health, logs, events, and activity without digging through files.
---

When something feels off — sync hasn't run, a draft vanished, a rule misfired — `mxr` gives you four lenses on the same daemon: status, logs, events, and activity. They all live one keystroke or one URL away.

## Health check first

Start broad. `status` shows the daemon's current view; `doctor` checks the deeper invariants.

```bash
# Single snapshot
mxr status

# Watch it live
mxr status --watch

# Deep check (rebuild advisories, store stats, index freshness)
mxr doctor --check --format json
```

What you get: a single object with daemon uptime, account sync status, lexical/semantic index freshness, and any active findings.

## Read past logs without `tail -f`

The new `mxr logs` accepts `--level`, `--search`, `--limit`, and `--since` together, so you can replay past incidents without grepping the raw file.

```bash
# Last 200 warn-or-error lines in the last hour
mxr logs --level warn --since 1h --limit 200

# Past errors mentioning Gmail
mxr logs --level error --search 'gmail' --since 24h

# Stream JSON to jq for ad-hoc analytics
mxr logs --format jsonl --limit 1000 | jq -r 'select(.level=="ERROR")'
```

What you get: the most recent `--limit` lines that match your filter. With `--format json|jsonl`, each line becomes `{ timestamp, level, message, raw }` for piping.

When you want the file open in `$EDITOR`:

```bash
mxr logs --no-follow --limit 1000 > /tmp/mxr.log && $EDITOR /tmp/mxr.log
```

## Inspect persisted events

The event log keeps mutation, sync, and rule events with structured columns. Filter by level, category, time, or free-text:

```bash
# Mutation events in the last day
mxr history --category mutation --since 24h --format json

# Every sync error this week — search hits summary AND details
mxr history --category-prefix sync --level error --search 'timeout' --since 7d

# Page through (offset + limit)
mxr history --since 30d --limit 50 --offset 50
```

What you get: an array of `{ timestamp, level, category, account_id?, message_id?, rule_id?, summary, details }`. The `--search` flag searches both summary and details.

## Live event stream

Sometimes you want events as they happen — for example to wait on a sync that you just kicked off.

```bash
mxr events --type sync --format jsonl
```

What you get: one JSON object per event. Pipe it into your status bar, a Slack webhook, or `jq` for filtering.

## Activity: what *you* did

Distinct from logs (system events) and events (mutation history), the **activity log** is a local record of every *user-intent* action across the TUI, CLI, and web. See the [Activity Log guide](/guides/activity-log/) for the full design — short version:

```bash
mxr activity list --since 24h
mxr activity replay --since 1h        # prose narrative
mxr activity recall "yesterday afternoon"
```

What you get: a reverse-chrono view of reads, archives, searches, sends. Strictly local — never transmitted off-device.

## Surfaces

All four lenses are reachable from one place:

- **TUI** (`G d`): a six-pane Diagnostics screen — `Status / Doctor`, `Data / Storage`, `Sync Health`, `Recent Events`, `Recent Logs`, **`Activity Log`**. `Tab` / `Shift-Tab` to cycle, `Enter` to fullscreen one, `L` to open the raw log file in `$EDITOR`.
- **Web** (`/diagnostics`): a tabbed Diagnostics page — `Overview / Logs / Events / Activity`. The Logs tab supports level filter, search, row-limit, and a pause toggle for the live tail. The Events tab supports level + category + free-text search + time window + paging. The Activity tab embeds the full activity browser with filters, bulk select, and pause controls.
- **CLI**: `mxr status`, `mxr doctor`, `mxr logs`, `mxr history`, `mxr events`, `mxr activity ...`.

## In real life

- **"Why did that batch archive fail?"** — `mxr history --search 'archive' --level error --since 1h`, then drill into the `details` field. If it's a sync issue, follow up with `mxr doctor --check --format json | jq '.sync_statuses'`.
- **"What was the daemon doing during the last 10 minutes?"** — `mxr logs --since 10m --limit 500 --format jsonl | jq -r '"\(.timestamp) \(.level) \(.message)"'`.
- **"Did the prune actually run last night?"** — `mxr activity list --action activity.pruned --since 24h`. Synthesized markers show what the daemon did to its own data.
- **"I keep seeing intermittent failures. Make me a bug report."** — `mxr bug-report --github` opens a pre-filled issue with status, recent events, log tail, and config (sanitized).

## Notifications and quick counts

```bash
mxr notify                            # unread counts for the status bar
mxr notify --format json --watch      # JSON stream for tmux, polybar, sketchybar
mxr count "label:inbox is:unread"
```

## Agent prompts that work

```text
"Run `mxr doctor --check --format json` and `mxr history --since 24h
--level error --format json`. Summarise findings under three headings —
**Critical**, **Yellow**, **Green** — and propose at most one concrete
next command for each. Don't run mutating commands."
```

```text
"Use `mxr logs --since 10m --format jsonl` plus `mxr activity list
--since 10m --format json` to reconstruct what happened in the last ten
minutes. Tell me first what *I* did, then what the *daemon* did, then
flag anything unexpected."
```

## Bug reports

When you need a shareable diagnostic bundle:

```bash
mxr bug-report                         # writes a Markdown file
mxr bug-report --stdout                # prints to stdout
mxr bug-report --github                # opens a pre-filled GitHub issue
mxr bug-report --full-logs             # include unredacted log tail
```

Or open the TUI Diagnostics screen and press `b` for one-click bug-report.

## See also

- [Activity Log](/guides/activity-log/) — the user-intent record (separate from system events).
- [Security & Privacy](/guides/security-and-privacy/) — what stays local in mxr.
- [`mxr logs`](/reference/cli/logs/), [`mxr history`](/reference/cli/history/), [`mxr events`](/reference/cli/events/), [`mxr activity`](/reference/cli/activity/) — CLI references.
- [`[logging]`](/reference/config/#logging) and [`[activity]`](/reference/config/#activity) config keys.
