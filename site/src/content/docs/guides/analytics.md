---
title: Analytics
description: Local-first inbox analytics in mxr — storage, stale threads, contact asymmetry, response time, and decay.
---

mxr ships a small set of analytics commands that turn the local mail corpus
into actionable signal. The orientation is **decisions first** — every
metric ends in a verb (unsubscribe, archive, reply, demote a contact). If a
number doesn't drive an action, it's not in the surface.

Cloud analytics tools (Email Meter, EmailAnalytics, Sanebox) are
structurally limited to headers + subject for legal/PR reasons. mxr has the
full body, attachments, and graph locally — the analytics here are the ones
SaaS tools won't ship.

## Bootstrap

Analytics are computed against your local store. After a fresh install or
upgrade, three commands prime the pipeline:

```bash
# 1. Tell mxr which addresses are *yours* — direction inference depends
#    on this. `MessageFlags::SENT` is unreliable across providers
#    (Gmail = label-based, IMAP = mailbox-name-based), so mxr classifies
#    inbound vs outbound by comparing `from_email` against this set.
mxr accounts addresses add you@example.com --primary
mxr accounts addresses add aliased@you-own.com   # repeat for each alias

# 2. Verify the set.
mxr accounts addresses list

# 3. One-shot rebuild against existing data. Idempotent — safe to rerun.
#    Usually unnecessary: the daemon self-heals after each sync (see below).
mxr doctor --rebuild-analytics
```

`--rebuild-analytics` performs six steps in order, streaming live
progress:

1. **Reclassify directions** — every `direction='unknown'` row gets
   inbound/outbound based on the address set.
2. **Backfill `list_id`** — promotes `List-Id` from cached body metadata
   into an indexed column.
3. **Backfill reply pairs from messages** — scans every classified
   message with `in_reply_to` and pairs it with its parent in the
   local store.
4. **Reconcile pending reply pairs** — resolves orphan pairs whose
   parent has since arrived.
5. **Backfill business-hours latency** — fills the working-hours latency
   column on every reply pair (M-F 09-17 UTC).
6. **Refresh contacts** — rebuilds the materialized `contacts` table.

Each step emits an `OperationProgress` event the CLI prints as
`[N/6] message` while a Unicode spinner ticks between steps. The
final summary reports `already healthy` (when nothing needed
backfilling) or `backfilled N items`, with per-row markers: `✓` for
"already correct", `← fixed` for real work, `↻` for the contacts
table size (a *total*, not a delta). JSON output adds
`status: "healthy" | "rebuilt"` and `duration_ms` so scripts can
branch on the outcome.

### Self-healing

The daemon now runs the four cheap delta steps (1, 2, 4, 5) after
each sync — they're no-ops on healthy data so the cost is near-zero,
but any drift fixes itself within one sync cycle. The heavier reply-
pair scan (step 3) runs once per daemon process to cover post-upgrade
rescans where a release adds a derived column.

Result: you almost never need to run `mxr doctor --rebuild-analytics`
manually. It stays as the sledgehammer for "I don't trust the local
state, redo everything from scratch" — useful after restoring from
backup, importing a foreign mailbox, or debugging a suspect query.

After bootstrap, mxr also keeps the materialized analytics current
via two background loops: a 60-second reply-pair reconciler and a
5-minute contacts refresher. Both shut down cleanly with the daemon.

## Commands

### Storage

Where disk is going.

```bash
mxr storage --by sender             # bytes per sender, descending
mxr storage --by mimetype           # bytes per attachment type
mxr storage --by label              # bytes per label
mxr storage --by message            # individual messages by size
mxr storage --by sender --limit 20 --format json | jq '.'
```

Four views: three rollups (sender / mimetype / label) and one per-message
ranking. The first three give you the bucket totals; `--by message` gives
you the single fattest emails — the 250 MB attachment from a courier
service, the giant zip a colleague sent in 2017 — with their message IDs
so you can act on them directly.

```bash
# Inspect the top 20 single biggest emails.
mxr storage --by message --limit 20

# Trash all messages from the heaviest sender.
mxr search "from:noreply@example.com" --format ids | xargs -n1 mxr trash

# Or trash the biggest individual messages directly.
mxr storage --by message --limit 10 --format ids | xargs -n1 mxr trash
```

The pattern: any storage view + `--format ids` pipes into `mxr search`
or directly into a mutation (`mxr trash`, `mxr archive`).

### Subscriptions

The list-sender ROI table. Aliased as `mxr unsub`.

```bash
mxr subscriptions                   # plain list, latest message per sender
mxr subscriptions --rank            # ranked: lowest open-rate first,
                                    # ties broken by archived-unread DESC
mxr unsub --rank --format json      # same; pipeable
```

Each row carries `opened_count`, `replied_count`, `archived_unread_count`,
and `message_count`. The rank is **open-rate ASC, archived-unread DESC** —
the noisiest lists float to the top. Action: pick a row, hit
`mxr unsubscribe` (the actual unsubscribe command).

### Stale

Threads where someone owes a reply.

```bash
mxr stale --mine                    # latest message inbound; you owe
mxr stale --theirs                  # latest message outbound; they owe
mxr stale --mine --older-than-days 7
mxr stale --mine --within-days 90   # active in the last 90 days only
```

Default window: between 14 days old (lower bound) and 365 days old
(upper bound). The upper bound exists deliberately — it keeps the
result actionable. Without it, decade-old archived threads dominate.
Widen `--within-days` for a deep audit.

The query also filters out messages with implausible `Date:` headers
(epoch 0 / pre-2000 garbage) so spam with corrupt dates doesn't surface.

### Contacts

Relationship analytics over the materialized contacts table.

```bash
# Reply imbalance — surfaces the people you're letting down (or vice versa).
mxr contacts asymmetry --min-inbound 3
mxr contacts asymmetry --format json | jq '.[] | select(.asymmetry > 0.7)'

# Going-cold detection. Last inbound much more recent than last outbound.
mxr contacts decay --threshold-days 30
mxr contacts decay --threshold-days 90 --max-lookback-days 1095

# Force a full materialized refresh (the daemon does this every 5 minutes).
mxr contacts refresh
```

`asymmetry` is `|inbound - outbound| / max(inbound, outbound)` in `[0, 1]`.
0 = balanced; 1 = one-sided. `--min-inbound` filters out one-off senders.

`decay` shows contacts whose last inbound is more recent than their last
outbound by **more than `--threshold-days`** (boundary excluded). The
default `--max-lookback-days 1095` (3 years) drops contacts so dormant
they're effectively past the relationship-rebuild horizon.

### Response time

Reply-latency percentiles, both clock and business-hours.

```bash
mxr response-time                                    # mine, all-time
mxr response-time --theirs                           # their reply time to me
mxr response-time --counterparty alice@example.com   # scoped
mxr response-time --since-days 90                    # last 90 days only
```

Output:

```
Reply-latency summary: I replied to
Sample size: 247

                              P50          P90
----------------------------------------------
clock                          4h 12m       2d 8h
business-hours                 1h 30m       6h 45m
```

Business-hours percentiles use M-F 09:00–17:00 UTC by default. Useful
when "I take 4h to reply to my boss" should not include the 11pm
arrival window.

If `Sample size: 0`, run `mxr doctor --rebuild-analytics` — reply pairs
need at least one rebuild to populate from existing data.

### Wrapped

Year-in-review for your inbox. Like Spotify Wrapped, except it runs
locally and never phones home.

```bash
mxr wrapped                          # year-to-date (default)
mxr wrapped --ytd                    # explicit; same as above
mxr wrapped --year 2025              # full 2025 calendar year
mxr wrapped --since-days 90          # last 90 days (quarterly review)
mxr wrapped --year 2025 --format json | jq '.'
```

The narrative output covers seven slices:

- **Volume** — sent, received, distinct threads.
- **When you do email** — busiest weekday, busiest hour-of-day,
  single busiest calendar day.
- **Top contacts** — most-emailed-to-me, most-emailed-by-me, most
  one-sided counterparty.
- **Reply discipline** — p50/p90 clock-time + business-hours,
  fastest reply (with target), slowest reply (capped at 30 days so
  the "8 years later" pathological pairs don't claim the title).
- **Storage** — total bytes in window, top mimetype, single
  heaviest message.
- **Newsletters** — unique lists, top list with open rate, share of
  inbound that came from a list.
- **Superlatives** — longest thread, most-ghosted counterparty
  (≥10 inbound, 0 outbound).

If a slice is missing data (e.g. no reply pairs in the window), the
section either reports the zero or hints at running
`mxr doctor --rebuild-analytics`. The JSON output is the full
`WrappedSummary` for piping into wrappers — your own end-of-year blog
post, a personal dashboard, a quick agent that mails you the summary on
Jan 1.

### Account addresses

Direction inference depends on this set; CRUD commands match the rest of
`mxr accounts`.

```bash
mxr accounts addresses list
mxr accounts addresses add alias@example.com           # alias
mxr accounts addresses add primary@example.com --primary
mxr accounts addresses set-primary alias@example.com   # demote previous primary
mxr accounts addresses remove old@example.com
```

Exactly one address per account is `is_primary=true` (enforced by a
partial unique index in SQLite). The address set is cached in memory by
the daemon and refreshed after every mutation.

### Doctor

Maintenance entry points. Both are idempotent.

```bash
mxr doctor --rebuild-analytics   # full 6-step rebuild with live progress
mxr doctor --refresh-contacts    # only the contacts table
```

The daemon self-heals analytics after each sync, so manual
`--rebuild-analytics` is rarely needed. Run it as the sledgehammer
when you want to be sure:

- After restoring from backup or importing from another tool
- When debugging a suspect analytics result
- After a release that warns about a one-time rescan

Adding/removing account addresses, schema changes, and long offline
periods are all covered by the post-sync hook automatically.

## Output formats

Every analytics command supports `--format`:

- `table` (default on a TTY) — human-readable column-aligned
- `json` (default on a pipe) — pretty single object/array
- `jsonl` — one JSON object per line, ideal for `jq -c`
- `csv` — comma-separated, RFC 4180 quoted
- `ids` — bare keys/emails/thread-ids, one per line

```bash
mxr stale --mine --format ids | xargs -I{} mxr cat {}
mxr unsub --rank --format json | jq '.[] | select(.opened_count == 0)'
mxr contacts decay --format csv > decay.csv
```

## Workflows

Knowing the commands is half of it. Here are the situations these
analytics actually solve — described as the feeling first, the query
second, the action third.

### Friday cleanup — clear the week's reply backlog

*The feeling*: It's Friday afternoon and you suspect you've ghosted
someone this week. You don't want to scroll the inbox; you want the
list.

```bash
mxr stale --mine --older-than-days 5 --within-days 30
```

Pick three to five rows. Reply, or send "I'll get to this Monday" —
whichever is honest. The narrow window stops it from surfacing
genuinely-old threads you've already mentally filed.

### Newsletter prune — kill what doesn't earn its keep

*The feeling*: You're getting too much mail and "I should clean up my
subscriptions" has been on your todo list for six months.

```bash
mxr unsub --rank --format json \
  | jq -r '.[] | select(.opened_count == 0 and .message_count >= 5) | .sender_email'
```

Senders with five or more messages and zero opens are noise, full stop.
Run `mxr unsubscribe` on each, or pipe into a bulk-archive rule. If a
sender survives this filter, you genuinely engage with it.

### Disk reclamation — find the gigabyte hog

*The feeling*: macOS just told you you're out of disk and Mail.app's
1.5 GB cache isn't the actual problem.

```bash
mxr storage --by sender --limit 20
mxr storage --by mimetype --limit 20
```

The top sender by bytes is almost always one big-attachment newsletter
or a former coworker who shared video files. The top mimetype tells
you what kind of bulk action helps (`application/pdf` → archive old
contracts; `video/*` → just delete). Combine with `mxr search
"from:<sender> has:attachment"` to action a target list.

### Cold-friend audit — the "I should text them" list, from data

*The feeling*: There's someone you used to talk to weekly and you
haven't replied to their last email in two months. You can't remember
who.

```bash
mxr contacts decay --threshold-days 60 --max-lookback-days 730
```

Last inbound 60+ days ago, you owe an outbound, and they were active
in the last two years. Top of the list is usually a person, not a
service. Send a real reply this week.

### Am I getting slower? — month-over-year response time

*The feeling*: Someone said "you used to be more responsive" and you
want to know if it's true.

```bash
mxr response-time --since-days 30
mxr response-time --since-days 365
```

`response-time` always emits both clock-time and business-hours
percentiles per direction; pick the row that matches what you care
about. If `business_hours_p90` for the last 30 days is meaningfully
higher than the year-long baseline, you're a bottleneck on something
specific. Either set expectations explicitly (auto-replies, "I batch
email at noon") or set up a `mxr rules` filter to demote low-priority
inbound.

### Asymmetric relationships — fix the one-sided ones

*The feeling*: There's a vague sense you're letting people down, but no
specific list.

```bash
mxr contacts asymmetry --min-inbound 5 --format json \
  | jq '.[] | select(.asymmetry > 0.7 and .total_inbound >= 10)'
```

People who emailed you ten or more times and got a reply less than 30%
of the time. Three options per row: reply now (with an apology if
appropriate), reset expectations explicitly ("I'll only reply when X"),
or stop pretending you'll engage and move them to a folder.

### Per-counterparty SLA — boss vs. client

*The feeling*: You want to know how fast you reply to your manager
specifically, or how fast a slow client tends to reply to your
proposals.

```bash
# How long do I take to reply to my manager?
mxr response-time --counterparty manager@company.com --since-days 90

# How fast does this client respond to my outbound?
mxr response-time --counterparty client@example.com --theirs --since-days 90
```

Useful before a 1:1 ("I'm averaging two hours; let me explain why
Wednesdays are different") or before chasing a stalled deal ("their
median response is four days, the proposal went out three days ago,
chill").

### Pre-vacation closeout — three artifacts in 30 seconds

*The feeling*: You're going OOO on Monday and the "I might be
forgetting something" feeling won't quit.

```bash
mxr stale --mine --older-than-days 7 --format ids > /tmp/oo-loose-ends.txt
mxr contacts asymmetry --min-inbound 10 --format json > /tmp/oo-asymmetric.json
mxr response-time --since-days 30 --format json > /tmp/oo-baseline.json
```

The first file is your "reply or hand off before Friday EOD" list. The
second is for the OOO message ("if you've been waiting on me, please
hold until next Wednesday"). The third sets the bar your replacement
or your future self will be measured against.

### Disk reclamation, drill-down edition

*The feeling*: `mxr storage --by sender` showed someone is eating 4 GB
and you want to delete the actual culprits, not just see a number.

```bash
# What individual emails are the biggest offenders?
mxr storage --by message --limit 30

# Same thing but as IDs — pipe into trash for bulk delete.
mxr storage --by message --limit 30 --format ids | xargs -n1 mxr trash

# Drill into one heavy sender specifically.
mxr search "from:noreply@example.com has:attachment" --limit 20
mxr search "from:noreply@example.com has:attachment" --format ids | xargs -n1 mxr trash
```

Pick the format that matches the action:
`mxr storage --by sender` for "who's responsible",
`mxr storage --by mimetype` for "what kind of file",
`mxr storage --by message` for "which specific emails to delete."

### Wrapped — your year, summarised

*The feeling*: Spotify just sent you Wrapped. You'd like the same thing
for the inbox you actually live in, without sending your email to a
third party.

```bash
mxr wrapped                          # year-to-date
mxr wrapped --year 2025              # full prior year
mxr wrapped --since-days 90          # quarterly version
```

Read the slowest-reply line first; it's almost always the most
informative one. Pair with `mxr response-time --since-days 365` if you
want the full distribution and not just the extremes.

### Year-in-review — without the year-in-review email

*The feeling*: It's December and you want a real picture of the year,
not the one your inbox app's marketing team made.

```bash
mxr response-time --since-days 365
mxr response-time --since-days 365 --theirs
mxr contacts asymmetry --min-inbound 20 --format json | jq '.[0:20]'
mxr storage --by sender --limit 20 --since-days 365
mxr wrapped --year 2025                       # human-readable year-in-review
```

Four numbers and two lists. Compare against last year by running the
same commands with `--since-days 730 | tail -365` style windows. The
question to answer: am I better at this than I was twelve months ago?

## Power tools

### Pipe ids into bulk mutations

`--format ids` is intentional. It's the input format for every mxr
mutation:

```bash
# Archive everything stale older than a year but newer than two.
mxr stale --mine --older-than-days 365 --within-days 730 --format ids \
  | xargs -n1 mxr archive

# Trash all zero-open list senders' mail in one shot.
mxr unsub --rank --format json \
  | jq -r '.[] | select(.opened_count == 0) | .sender_email' \
  | while read sender; do
      mxr search "from:$sender" --format ids | xargs -n1 mxr trash
    done
```

### Saved searches as durable lenses

If a `mxr stale` invocation becomes part of your weekly rhythm, save
it. The CLI is the definitive surface, but saved searches live in the
TUI sidebar and stay one keypress away:

```bash
mxr saved add owe-replies "is:unread label:inbox older_than:3d"
mxr saved add hot-clients "from:client@bigcorp.com" --mode hybrid
```

### Run analytics in scripts

Every command exits zero on success and writes machine-readable output
to stdout when `--format json` or `--jsonl` is set. They compose into
shell pipelines, scripts, agents, and editor integrations the same way
`grep` does. There is no separate "automation API" — the CLI *is* the
API.

```bash
# A weekly cron that emails you your inbox health report.
0 9 * * 1 mxr stale --mine --format json | mailx -s "weekly inbox audit" you@example.com
```

## Behavior on existing data

| Field | Filled by |
|---|---|
| `direction` | `--rebuild-analytics` (uses your address set), or sync going forward |
| `list_id` | `--rebuild-analytics` (from cached body metadata), or sync going forward |
| `reply_pairs` | `--rebuild-analytics` (one-time scan + the 60s reconciler going forward) |
| `business_hours_latency_seconds` | `--rebuild-analytics` (and the reconciler) |
| `contacts` table | `--rebuild-analytics` and the 5-min refresher loop |
| `message_events` | Forward-only — historical state-transition timestamps are unrecoverable |

Pre-existing read/star/archive timestamps are not reconstructable —
they were never recorded. The events table starts capturing transitions
from the first daemon boot after the analytics ship.

## Implausible-date filter

mxr's analytics ignore messages with `date < 2000-01-01 UTC`. In
practice these are spam with corrupt `Date:` headers that fall back to
epoch 0 at parse time. Without the filter, a 1970-stamped phishing
message ranks as "the most stale thread of all time" forever.

If you need the unfiltered view (e.g. for archaeology), run a raw SQL
query against `~/Library/Application Support/mxr/mxr.db` directly.

## What it deliberately doesn't do

- **No tracking pixels, no open-rate analytics on inbound mail.** Apple
  Mail Privacy Protection killed this category in 2021; mxr never had it.
- **No body sentiment scoring.** Low signal, easy to misread sarcasm.
- **No vanity counters.** "You sent 12,847 emails this year" is one-shot
  novelty, not a decision driver.
- **No cross-device telemetry.** Everything stays in your local SQLite.

## Agent prompts that work

```text
"Help me cut storage. Run `mxr storage --by sender --format json`,
identify the top 5 noise senders, show me how many messages each has,
and propose an `mxr search ... --format ids | mxr archive --dry-run`
pipeline for each. Show the dry-run first."
```

```text
"Who am I going cold on? `mxr contacts decay --threshold-days 60
--format json` then for each result show me the latest 3 inbound
subjects so I can decide whether to follow up."
```

## See also

- [Recipes — bulk operations](/guides/recipes/#with-xargs--bulk-operations)
- [Sender view (CLI)](/reference/cli/#sender-view)
- [Triage flow](/guides/triage-flow/)
