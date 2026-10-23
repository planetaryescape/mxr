---
title: Browse your activity log
description: Browse, filter, export, and scrub selected local activity records.
---

mxr can record selected successful user-intent requests from supported
clients. The activity log is local, can be disabled or paused, and is not a
record of every action. Browse a time range, filter by action or client, and
redact rows you no longer want to keep.

The log is a local working history, not a compliance audit. By default,
subject, recipient email, and search-query context can be recorded; link URL
tracking is off. Pause drops ordinary new rows, some rows coalesce, and the
write queue is bounded. Change the metadata controls in `[activity]` or set
`MXR_ACTIVITY=off` to disable writes.

## What gets captured

State-changing or intent-expressing actions:

- **Mail**: `mail.read`, `mail.archive`, `mail.trash`, `mail.star`, `mail.label`, `mail.move`, `mail.snooze`, `mail.unsubscribe`, `mail.mark_spam`.
- **Send**: `mail.send`, `mail.reply`, `mail.forward`, `draft.create`, `draft.assist` (the action, model and how much came back, never the text), `draft.discard`, scheduled sends.
- **Search**: `search.run`, `search.save`, `search.delete`, `saved.open`.
- **Threads**: `thread.open`, `thread.summarize`, `thread.flag_reply_later`.
- **Accounts**: `account.add`, `account.remove`, `account.signin`, `account.sync`.
- **Rules**: `rule.create`, `rule.update`, `rule.delete`, `rule.run`, `rule.test`.
- **Snippets / signatures / screener / reminders**: edits and triage decisions.
- **Microsoft task links**: `todo.microsoft_link`, `todo.microsoft_sync` for confirmed link and manual sync requests. These record only the task ID, not its title, notes or email content. Previews and automatic reconciliation are not captured.

Mapping is closed: every IPC verb that produces activity has an explicit entry in `crates/daemon/src/activity/mapper.rs`. New IPC verbs default to "no activity" until someone decides what to log.

```bash
# Show the last day of activity in a table
mxr activity list --since 24h
```

What you get: reverse-chronological table with `TIME / SRC / ACTION / TARGET / CONTEXT` columns. Source badges (`tui`, `cli`, `web`, `daemon`, `agent`, `mcp`, `assistant` and the rest) make it obvious where each action originated. Changes the [assistant](/guides/assistant/) made after you approved them show as `assistant`; filter with `--source assistant`.

## What it does NOT capture

- Cursor moves, scroll, pane focus, palette opens-then-cancelled.
- Heartbeats, pings, status polls, background sync, FTS rebuilds.
- Read-side getters (`mxr cat`, listing envelopes) — they don't change state.
- New activity when `[activity].enabled = false` or `MXR_ACTIVITY=off` is set at daemon startup. Pause drops ordinary activity but keeps pause/resume markers.

## Filter to the slice you care about

Shared filter flags apply to `list`, `stats`, `top`, `export`, and `redact`. AND-combined; empty means any.

```bash
# Last week of mail mutations, JSON for piping
mxr activity list --since 7d --prefix mail. --limit 100 --format json

# Searches you ran for "invoice"
mxr activity list --action search.run --query 'invoice'

# Activity on a specific thread
mxr activity list --target-kind thread --target-id thr_abc123
```

What you get: each command emits the matching rows; `--format json` produces `{ entries: [...], next_cursor: { ts, id } | null }` for paging.

## See yesterday at a glance

Aggregate over a window to see what you actually did.

```bash
# Top actions over the last week
mxr activity top --since 7d --limit 10

# Hour-of-day histogram for the last month
mxr activity stats --group-by hour --since 30d --format json | jq

# Daily rollup
mxr activity stats --group-by day --since 30d
```

What you get: `KEY / COUNT` table in human mode, `{ buckets: [...] }` in JSON mode.

## Replay a session as prose

`replay` aggregates consecutive same-action rows into readable lines.

```bash
mxr activity replay --since 1h
```

What you get:

```text
Since 1h:
  09:42  Read 5 threads
  09:43  Searched "invoice 2026"
  09:44  Archived 12 threads
  09:46  Sent 1 message
```

## Time travel by natural-language phrase

`recall` resolves a curated set of fuzzy-time phrases.

```bash
mxr activity recall "yesterday afternoon"
mxr activity recall "last hour"
mxr activity recall "before lunch" --limit 20
```

Accepted phrases: `today`, `yesterday`, `this morning`/`afternoon`/`evening`, `lunch`, `breakfast`, `night`, `last <duration>`, `past <duration>`, `before|after|since|until <phrase>`. Anything else returns an error pointing at the supported grammar.

## Export your data

```bash
# CSV → stdout
mxr activity export --format csv > today.csv

# NDJSON → file (preferred for piping into jq/awk)
mxr activity export --format ndjson --out my-week.ndjson

# Filter first, export only the slice
mxr activity export --prefix mail. --since 7d --format ndjson --out mail-week.ndjson
```

What you get: the daemon writes the file with the matching rows in the requested format. CSV is RFC 4180. NDJSON is one `ActivityEntry` per line. JSON is a top-level pretty-printed array.

## Scrub what you don't want kept

Choose what to record, pause temporarily, or scrub existing rows.

### Choose what to record (config)

Edit the file printed by `mxr config path`:

```toml
[activity]
enabled = true
track_subjects = false
track_recipient_handles = false
track_search_queries = false
track_link_clicks = false
```

Restart the daemon after changing these controls. This example keeps actions, counts and opaque IDs without subjects, email handles, search text or URL metadata. Sender-address targets are omitted too; folder destinations and rule names remain. Set `enabled = false` instead to stop all new activity rows, including administrative markers.

These controls affect new rows only. To remove old context, use the dry-run and tombstone commands below. Disabling or pausing recording also stops Reading engagement, but you can still save Later items and highlights. Link-click tracking is off by default; no current producer emits `link.click` rows, so enabling it does not add browser tracking.

### Hard kill (env var)

```bash
MXR_ACTIVITY=off mxr daemon
```

The kill switch overrides config for the lifetime of the daemon, including forced pause/resume markers. Set it when starting the daemon, not just on a client command talking to an already-running daemon.

### Soft pause (runtime)

```bash
# Stop recording for 2 hours; auto-resumes
mxr activity pause --for 2h

# Indefinite — stays paused until `resume`
mxr activity pause

# Bring it back
mxr activity resume
```

Pause writes one `activity.paused` marker, then drops new writes until the window elapses or you `resume`. Auto-resume also writes a synthesized `activity.resumed` marker so you can always see when the gap began and ended.

### Tombstone after the fact

Always dry-run first. Tombstones are irreversible (the audit-trail columns survive but `context_json` is cleared).

```bash
# Preview what would be tombstoned
mxr activity clear --last 1h --dry-run

# Tombstone the last hour (preserves important-tier rows by default)
mxr activity clear --last 1h --yes

# Nuke everything, including important rows
mxr activity clear --last all --include-important --yes

# Surgical: tombstone two specific rows
mxr activity redact --ids 42,43 --yes

# Retention prune (hard delete, not tombstone)
mxr activity prune --before 90d --dry-run
mxr activity prune --before 90d --yes
```

## Save filters you use a lot

Saved activity filters mirror saved searches.

```bash
# Save the current filter under a slug
mxr activity saved save mail-week --name "Mail this week" --since 7d --prefix mail.

# Run a saved filter
mxr activity saved open mail-week

# Manage
mxr activity saved list
mxr activity saved delete mail-week
```

## Recipes

### What was I doing right before that bug?

```bash
mxr activity replay --since 30m
```

Quick prose narrative of the last half-hour. Catches you up after a context switch.

### Which threads did I archive without reading?

```bash
mxr activity list --since 7d --action mail.archive --format json \
  | jq -r '.entries[] | select(.context.read_then_archive != true) | .target_id'
```

What you get: thread IDs you archived without the `read_then_archive` flag — candidates for unarchive if you went too fast.

### Surface my most-searched terms this month

```bash
mxr activity list --since 30d --action search.run --format json \
  | jq -r '.entries[].context.query' \
  | sort | uniq -c | sort -nr | head -20
```

What you get: top-20 queries you ran in the last 30 days, ranked.

### Audit what mxr did the last time it crashed

```bash
mxr activity list --since 24h --include-redacted --format json \
  | jq -r '.entries[] | select(.action | startswith("activity.")) | "\(.ts)\t\(.action)\t\(.context)"'
```

What you get: synthesized markers (`activity.paused`, `.resumed`, `.pruned`, `.redacted`, `.exported`, `.cleared`) — the daemon's diary about itself.

## Agent prompts that work

```text
"Use `mxr activity list --since 24h --format json` to summarise what I did
in the last day, then suggest one filter I should save with
`mxr activity saved save`. Don't run any mutating commands. If you find
sensitive context, propose a `mxr activity redact --ids ... --dry-run`
without executing it."
```

```text
"Run `mxr activity replay --since 4h` and paste the output. Then run
`mxr activity stats --group-by hour --since 7d` and tell me my peak
inbox-processing hour."
```

## Surfaces

- **CLI**: `mxr activity ...` (alias `mxr act`). Scripts use `--format json`; `mxr activity export` also takes `--format ndjson`.
- **TUI**: press `g a` from any screen to open the activity modal. `j`/`k` to navigate, `p` to toggle pause, `Esc` to close. Also visible inside the Diagnostics page (`G d`) as the **Activity** pane.
- **Web**: navigate to `/activity` in the web app, or open the Diagnostics page and switch to the **Activity** tab. Filter sidebar, bulk-select with checkboxes, pause/resume from the top bar.

## Storage and retention

Three tiers with separate retention windows. Configure in the file printed by `mxr config path`:

```toml
[activity]
enabled = true
track_link_clicks = false      # opt-in; URLs reveal a lot
track_subjects = true
track_recipient_handles = true
track_search_queries = true

[activity.retention]
ephemeral_days = 30      # views, palette opens, navigation
standard_days = 90       # searches, thread opens, attachment views
important_days = 365     # mail mutations, sends, account changes
```

A daily prune sweep hard-deletes rows older than the cutoffs. Each deletion writes one `activity.pruned` marker so you can audit what was removed.

## Forbidden in `context_json`

Activity producers omit credential material and mail content:

- OAuth/refresh tokens, password hashes, API keys, session ids.
- Attachment bytes (filenames + sizes only).
- Mail bodies and draft text. Send activity keeps content kind and counts, not body excerpts.

A PII-audit test asserts the table never contains the forbidden keys after a full mapper run.

## Limits today

- No encryption at rest. Relies on OS-level disk encryption (FileVault, LUKS).
- No cross-device sync. By design.
- The TUI modal is read-only in v1 (browse + pause). Redaction and export currently flow through the CLI; the web surface has bulk-redact via the data table.

## See also

- [Security & Privacy](/guides/security-and-privacy/) — the broader local-first guarantees mxr makes.
- [Diagnose mxr fast](/guides/observability/) — the page that hosts the Activity tab alongside Logs and Events.
- [`mxr activity` CLI reference](/reference/cli/activity/) — full flag inventory, auto-generated from `--help`.
- [`[activity]` config keys](/reference/config/#activity) — defaults and toggles.
