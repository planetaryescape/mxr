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

`mxr doctor --check` exits 0 only when the report carries no Error-severity finding, so it works as a CI or cron gate:

```bash
mxr doctor --check --format json >/tmp/mxr-doctor.json \
  || jq '.findings[] | select(.severity == "error")' /tmp/mxr-doctor.json
```

### Diagnostics for a selected daemon

`mxr status` and `mxr doctor --check`, `--store-stats`, or `--index-stats` use the daemon selected by `MXR_DAEMON_ADDR`. For an explicit target, JSON keeps ownership clear: `daemon_target` identifies the selection, `client_local` contains paths on the CLI machine, and doctor places the daemon report or statistics under `daemon`. Any remediation and next-step commands in the daemon report, including table output, must be run on the selected daemon host.

```bash
MXR_DAEMON_ADDR="cmd://ssh -T host mxr daemon dial-stdio" mxr status --format json
MXR_DAEMON_ADDR="cmd://ssh -T host mxr daemon dial-stdio" mxr doctor --check --format json
```

If that target cannot be reached, the diagnostic command fails without switching to or starting the local default daemon. Connection metadata and client-generated connection errors hide `cmd://` arguments. `mxr doctor --reindex` operates on the local profile. Run it on the daemon host with `MXR_DAEMON_ADDR` unset and the intended `MXR_INSTANCE` or `MXR_CONFIG_DIR`/`MXR_DATA_DIR` settings.

### Is my mail current?

`mxr status --freshness` answers the quick "is new mail reaching me?" check: when the newest message arrived (any mailbox, any mode), each account's last good sync, the last sync error with its kind and retry time, and the last arrivals with the mode each one went to.

```bash
mxr status --freshness
# Latest mail 5m ago (09:37)
# Sync: Gmail paused: rate limited, retrying 09:50
# Last arrivals:
#   09:37  GitHub  Build passed  → Updates · automated

mxr status --freshness --account work --limit 20 --format json
```

With several accounts, the default scope shows the freshest mail and the worst sync state. The web app shows the same two signals in its status bar on every page, and the TUI shows them in its status line. The bridge serves it at `GET /api/v1/mail/freshness`.

### When the daemon says `degraded`

`mxr status` reads its account and message counts out of SQLite. A large sync can saturate the reader pool, and rather than block, the daemon answers with `degraded: true` and no snapshot. That is "the daemon did not say", not "there are no accounts":

```bash
mxr status --format json | jq '{degraded, accounts, total_messages}'
```

The table output prints `unknown` for the fields it has no reading for, instead of a zero that reads like an empty mailbox. Poll again once the sync settles — nothing needs fixing. The version, PID, and feature-health fields are always a real reading.

### Why a second `mxr now` is instant

Now, the rail, Messages, Updates and the Reading edition are built from every inbox message, which takes about a second on a large mailbox. The daemon keeps the last build of each one you read and answers the next read from it, as long as nothing it shows can have changed:

- Anything you change through mxr (a done, a move, a reply) makes the next read rebuild first.
- New mail and other stored changes do the same. Log, sync-bookkeeping and index writes don't count.
- A view older than a minute, or built before the current hour, is rebuilt first, because the views word time ("due soon", evening, today).

In the background, the daemon rebuilds the views you read in the last 30 minutes a couple of seconds after mail changes, so reads usually find one ready. `generated_at` in the JSON says when a view was built. A read that records something always runs: `mxr reading` counts as a visit; `mxr reading edition --peek` does not.

To watch the rebuilds, set `level = "debug"` under `[logging]` in `config.toml`, restart the daemon, and search the log:

```bash
mxr logs --no-follow --search 'mode view refreshed' --since 10m
```

## Read past logs without `tail -f`

Use `mxr logs` to read the active file and retained mxr archives on this machine.
It accepts `--level`, `--search`, `--limit`, and `--since` together.

```bash
# Last 200 warn-or-error lines in the last hour
mxr logs --no-follow --level warn --since 1h --limit 200

# Past errors mentioning Gmail
mxr logs --no-follow --level error --search 'gmail' --since 24h

# Stream JSON to jq for ad-hoc analytics
mxr logs --format jsonl --limit 1000 | jq -r 'select(.level=="ERROR")'
```

What you get: the most recent `--limit` lines that match your filter. With `--format json|jsonl`, each line becomes `{ timestamp, level, message, raw }` for piping.

`--level warn` includes WARN and ERROR, using the record's actual level rather
than words in its message. `--since` accepts RFC 3339 or durations such as `1h`;
records without a parseable timestamp cannot pass that time filter. Legacy
space-delimited logs remain readable. `--limit` is the initial matching tail,
from 1 to 10,000 lines (default 200), not a limit on later follow output. Reading
keeps only that tail in memory, not every matching line. Legacy records over
64 KiB are skipped.

Without `--no-follow`, the command polls every 500 ms. It tracks file identity
and complete-line offsets through rotation, replacement and daemon restart.
Partial trailing lines wait for a newline; retained old lines are not replayed
after a rename. Logs already removed by retention between polls cannot be
recovered. Read snapshots may briefly keep a rotated file open until their scan
finishes; daemon writers do not retain an old active-file handle.

When you want the file open in `$EDITOR`:

```bash
mxr logs --no-follow --limit 1000 > /tmp/mxr.log && $EDITOR /tmp/mxr.log
```

### Bound local text logs

```toml
[logging]
max_size_mb = 250
max_files = 10
stderr = true
event_retention_days = 90
```

These are the existing defaults, not the older blueprint's 50/5 sketch.
`max_size_mb` is a per-file MiB cap; `max_files` includes active `mxr.log`.
The defaults retain at most 2,500 MiB in named mxr text-log files. Zero values
and an overflowing aggregate cap are errors, not requests for unlimited logs.

The owned archive namespace is `mxr.` plus a 20-digit decimal sequence plus
`.log`, for example `mxr.00000000000000000000.log`. `.mxr-log.lock` serializes
writers across processes. A complete JSON record fits before rollover, including
one that lands exactly on the cap. Records larger than 64 KiB are replaced by
a small valid JSON warning, without copying the original content. Startup and
subsequent writes enforce reduced caps too: oversized existing owned files and
excess old archives can be discarded. An oversized active file at startup is
replaced with a bounded diagnostic. Unrelated names and symlinks are not rotated
or purged. Do not move the shared lock file while mxr writers are running.

Size/count/stderr settings are reread on writes; log level and `RUST_LOG` are
selected at process startup. Malformed TOML at startup uses tracing defaults so
config recovery commands remain available. If TOML becomes malformed while a
writer runs, it keeps its startup logging settings; config read I/O errors still
fail the write. Every record reopens the active path under the
shared lock, so a CLI rollover does not strand the daemon on an unlinked file.
When upgrading from append-only logging, run `mxr restart` after installing the
new binary so every writer uses this lock. Installation alone does not replace
an already-running older daemon; its old logger does not enforce these bounds.
An I/O failure can drop a record; it never recursively logs its own failure.
Where stderr is permitted, a fixed diagnostic reports that failure.

`stderr = true` mirrors the same bounded JSON records to stderr for ordinary
CLI commands and a foreground daemon. Tracing never writes stdout. Background
daemon launch, TUI/demo, stdio and dial-stdio paths stay file-only, even with
stderr enabled. `stderr = false` disables tracing mirroring, not command results
or explicit CLI error messages. Raw async-imap and tungstenite TRACE events
remain blocked before either sink. Log inspection and purge preview do not
initialize or rotate the logs they inspect.

### Preview manual cleanup

```bash
mxr logs --purge --dry-run --format json
mxr logs --purge --format json
```

Preview reports `selected_log_files`, `event_cutoff`, `purged_log_files` and
`purged_event_rows`, with `dry_run: true`; the counts are what would be removed.
Apply uses the same archive selection and event timestamp predicate. Both use
`event_retention_days` (default 90). Time and data can change between invocations;
check a fresh preview before applying. This is not an immutable preview token.

Only old regular owned archives are selected. The active file, lock, unknown
names, symlinks and recent archives are preserved. Preview does not delete files,
create a missing database, migrate it or change event rows. Apply manually prunes
old `event_log` rows as before; it does not prune `sync_log`, AI usage or activity.
Filesystem and database cleanup are not a single transaction: an I/O error can
leave partial progress. Automatic operational-history retention is a separate
follow-up, not part of text rotation. Activity's existing pruning is unchanged.

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
