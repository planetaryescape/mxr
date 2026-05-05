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
mxr doctor --rebuild-analytics
```

`--rebuild-analytics` performs five steps in order:

1. **Reclassify directions** — every `direction='unknown'` row gets
   inbound/outbound based on the address set.
2. **Backfill `list_id`** — promotes `List-Id` from cached body metadata
   into an indexed column.
3. **Backfill reply pairs** — scans every classified message with
   `in_reply_to` and pairs it with its parent in the local store.
4. **Backfill business-hours latency** — fills the working-hours latency
   column on every reply pair (M-F 09-17 UTC).
5. **Refresh contacts** — rebuilds the materialized `contacts` table.

The output reports per-step row counts.

After bootstrap, mxr keeps the materialized analytics current via two
background loops: a 60-second reply-pair reconciler and a 5-minute
contacts refresher. Both shut down cleanly with the daemon.

## Commands

### Storage

Where disk is going.

```bash
mxr storage --by sender             # bytes per sender, descending
mxr storage --by mimetype           # bytes per attachment type
mxr storage --by label              # bytes per label
mxr storage --by sender --limit 20 --format json | jq '.'
```

Use it to find heavyweight senders (newsletters with attachments, an old
work inbox), then archive or trash with `mxr search` plus a mutation.

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

Maintenance entry points.

```bash
mxr doctor --rebuild-analytics   # full rebuild, idempotent
mxr doctor --refresh-contacts    # only the contacts table
```

Run `--rebuild-analytics` after:

- Adding/removing account addresses (so direction reclassifies)
- A schema migration (so list_ids backfill into the indexed column)
- A long offline period (so reply pairs catch up across out-of-order arrivals)

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
