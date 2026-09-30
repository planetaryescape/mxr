---
title: Timing and cadence
description: Pick a send slot that respects the recipient's reply pattern, and watch a small list of relationships for drift.
---

Two cheap, statistical, fully local features that read your existing `reply_pairs` and `contacts` data. Neither calls an LLM. Neither does any server-side tracking: no pixels, no open tracking, no remote calls. They surface patterns mxr already has.

:::tip[The one-line mental model]
`mxr send-time` answers "when does this recipient reply fastest?" `mxr cadence` answers "which relationships I chose to maintain have gone cold?" Both run on data the sync loop already gathers.
:::

## Send-time optimizer: `mxr send-time`

```bash
mxr send-time alice@example.com
```

What you get: a confidence label, then each recipient's fastest one-hour window with its median (p50) reply time. From the demo mailbox:

```text
Confidence: Low

Recipient: jon@papertrail.example (samples: 2)
  Fastest: Sun 14:00-15:00 (p50 = 47m)
```

Confidence is `medium` from 8 replies and `high` from 20 replies spread over at least 3 hour buckets; below that it is `low`. A `low` answer still names the fastest window, so weigh it accordingly.

### Compare a proposed slot

```bash
# "If I send Friday at 7pm, how does that compare to her fastest slot?"
mxr send-time alice@example.com --at "fri 19:00" --format json
```

What you get: JSON with `proposed_at`, `proposed_weekday` (0 is Monday) and `proposed_hour`, `recipient_rows[]` (each with `sample_count`, `best_expected_reply_seconds`, its own `best_windows[]`, and `proposed_expected_reply_seconds` when that slot has data), the merged `best_windows[]`, and `confidence`. Weekdays and hours are in UTC: `fri 19:00` in London comes back as `proposed_hour: 18`.

### Multiple recipients

```bash
# Pick the slot that's least bad for everyone.
mxr send-time alice@example.com bob@example.com carol@example.com --format json
```

What you get: per-recipient rows so you can see who dominates. Overall `confidence` is the lowest of any recipient, and `best_windows` merges every recipient's windows, fastest first.

### Inside the safety pipeline

When you `mxr send DRAFT_ID --check`, the safety pipeline asks the same send-time path about sending now. A note is attached only when confidence is medium or high and now is at least twice as slow as the best window, so it doesn't nag on every send.

```bash
# See the timing info attached to a real safety report:
mxr send DRAFT_ID --check --format json \
  | jq '.issues[] | select(.code == "send_time_note")'
```

### Time syntax

The `--at` flag accepts the same [time phrases](/reference/time-phrases/)
as `mxr snooze --until`:

| Form | Example |
|---|---|
| Named day | `friday`, `mon`, `tue` |
| Day + time | `fri 19:00`, `tomorrow 9am`, `monday 17:00` |
| Relative | `in 2h`, `in 3d`, `in 2w` |
| RFC3339 | `2026-06-01T15:00:00Z` |

The phrase is read in your local time zone. Reply-time buckets are compared in UTC.

## Cadence drift: `mxr cadence`

Relationships you actually maintain are a small set. mxr does not auto-watch them: you watch each one explicitly, and the daemon surfaces only the ones that have drifted past their expected interval.

```bash
# Watch Alice with a 14-day expectation:
mxr cadence watch alice@example.com --every 14d

# See the list:
mxr cadence list --format json

# See drift (positive drift_days only):
mxr cadence drift --format json
```

What you get from `drift`: rows `{ email, display_name, last_contact_at, expected_days, drift_days, total_volume }`, most drifted first. `drift_days` is the days since last contact minus `expected_days`, and only positive drift is listed. No rows means nothing has drifted; that's a valid empty success state.

### Watch / unwatch

```bash
# Add a contact with an expected interval (--every or --expected-days).
mxr cadence watch alice@example.com --every 14d
mxr cadence watch mentor@example.com --every 30d

# Remove a row.
mxr cadence unwatch alice@example.com
```

The watchlist lives in `relationship_watchlist`, keyed by `(account_id, email)`. Without an interval, drift uses the contact's usual cadence, then 30 days. Unwatch removes the row; nothing is soft-deleted.

### List senders are rejected by default

`mxr cadence watch` refuses addresses mxr has marked as list senders, unless you override it:

```bash
# Pass --allow-list-sender when you actually mean it:
mxr cadence watch news@indie.example --every 7d --allow-list-sender
```

This stops the watchlist from filling up with newsletter addresses that don't reply.

### Composition

```bash
# For every drifted contact, open their full profile.
mxr cadence drift --format json \
  | jq -r '.[].email' \
  | xargs -I{} mxr sender {}
```

What you get: each drifted contact's profile (volume, recent threads, open commitments) so you can decide whether the gap actually matters. Watched contacts who have drifted also show under **Waiting on** on the [desk](/guides/desk/).

## In real life

- **Monday morning planning:** `mxr cadence drift --format json | jq '.[0:5]'`: the five most-drifted relationships you said you'd maintain. Skim, decide whether to write.
- **Choosing a send slot:** before scheduling a sensitive ask, `mxr send-time alice@example.com --at "thu 16:00"`, and switch slots if the proposed window is much slower than her best.
- **Audit of "I'll keep in touch":** `mxr cadence list --format json | jq 'length'` counts how many relationships you've committed to keeping warm.
- **Newsletter denial:** `mxr cadence watch news@example.com --every 7d` fails, which confirms the screener's list-sender classification is working.

## Operational notes

- All metrics are computed on demand from `reply_pairs` and `contacts`. A future `recipient_reply_latency_buckets` cache is documented, but there is no current table to maintain; the on-demand path is the source of truth.
- No data leaves the machine for either feature. `mxr send-time` and `mxr cadence drift` are pure-Rust queries.
- Watchlist entries are account-scoped: switching accounts gives you a different watchlist.

## Agent prompts that work

```text
"Before scheduling any draft, check `mxr send-time <recipient> --at
<proposed_at> --format json`. If the proposed slot is at least 2x slower
than the best window AND confidence is medium or high, propose the
faster slot to me. Do not auto-reschedule."
```

```text
"List my top 5 drifted relationships from `mxr cadence drift --format
json`. For each, summarize the last shared thread via `mxr summarize
<thread_id>` and suggest a one-sentence opener. Don't send."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/): where `send-time` attaches as a non-blocking hint
- [Analytics](/guides/analytics/): reply-latency, response-time, and the underlying `reply_pairs` data
- [Forgotten work](/guides/forgotten-work/): the inbound side (owed replies) of the same data
- [Search workflow](/guides/search/): operators that compose with watchlist output
- [CLI: `mxr send-time`](/reference/cli/send-time/), [`mxr cadence`](/reference/cli/cadence/)
