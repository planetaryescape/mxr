---
title: Timing and cadence
description: Pick a send slot that respects the recipient's reply pattern, and watch a small list of relationships for drift.
---

Two cheap, statistical, fully local features that read your existing `reply_pairs` and `contacts` data. Neither calls an LLM. Neither does any server-side tracking — no pixels, no open tracking, no remote calls. They surface patterns mxr already has.

:::tip[The one-line mental model]
`mxr send-time` answers "when does this recipient reply fastest?" `mxr cadence` answers "which relationships I chose to maintain have gone cold?" Both run on data the sync loop already gathers.
:::

## Send-time optimizer — `mxr send-time`

```bash
mxr send-time alice@example.com
```

What you get: a table of weekday × hour buckets ranked by typical reply latency, plus a confidence label (`low`, `medium`, `high`) driven by sample count. `low` means too little data to draw a conclusion — the command returns the buckets but suppresses the recommendation.

### Compare a proposed slot

```bash
# "If I send Friday at 7pm, how does that compare to her fastest slot?"
mxr send-time alice@example.com --at "fri 19:00" --format json
```

What you get: JSON with `proposed_at`, `recipient_rows[]` (each row has `proposed_expected_reply_seconds` and `best_expected_reply_seconds`), `best_windows[]`, and a `confidence` enum. A useful note fires only when the proposed slot is at least 2× slower than the best window AND confidence is medium or high.

### Multiple recipients

```bash
# Pick the slot that's least bad for everyone.
mxr send-time alice@example.com bob@example.com carol@example.com --format json
```

What you get: per-recipient rows so you can see which person dominates the recommendation. The worst meaningful delta wins; recipients with low sample count are reported but excluded from the worst-case calculation.

### Inside the safety pipeline

When you `mxr send DRAFT_ID --check`, the safety pipeline asks the same send-time path for a timing hint. The hint is only attached when confidence is medium/high AND the proposed slot is meaningfully worse than the best — so it doesn't nag on every send.

```bash
# See the timing info attached to a real safety report:
mxr send DRAFT_ID --check --format json \
  | jq '.issues[] | select(.code == "send_time_hint")'
```

### Time syntax

The `--at` flag accepts the same forms as `mxr snooze --until`:

| Form | Example |
|---|---|
| Named day | `friday`, `mon`, `tue` |
| Day + time | `fri 19:00`, `tomorrow 9am`, `monday 17:00` |
| Relative | `in 2h`, `in 3d`, `in 2w` |
| RFC3339 | `2026-06-01T15:00:00Z` |

Times are interpreted in the machine's local timezone and labeled as such.

## Cadence drift — `mxr cadence`

Relationships you actually maintain are a small set. mxr does not auto-watch them — you watch each one explicitly with an expected interval, and the daemon surfaces only the ones that have drifted past it.

```bash
# Watch Alice with a 14-day expectation:
mxr cadence watch alice@example.com --every 14d

# See the list:
mxr cadence list --format json

# See drift (positive drift_days only):
mxr cadence drift --format json
```

What you get from `drift`: rows `{ email, expected_days, days_since_contact, last_contact_at, drift_days }` ranked drift-descending. `drift_days = days_since_contact - expected_days`. No rows = nothing has drifted; that's a valid empty success state.

### Watch / unwatch

```bash
# Add a contact (interval is required — no implicit defaults).
mxr cadence watch alice@example.com --every 14d
mxr cadence watch mentor@example.com --every 30d

# Remove a row.
mxr cadence unwatch alice@example.com
```

The watchlist lives in `relationship_watchlist`, keyed by `(account_id, email)`. Watch entries are non-destructive on unwatch — they're removed cleanly, not soft-deleted.

### List senders are rejected by default

`mxr cadence watch` refuses mailing-list addresses (anything with a List-Id history) without an explicit override:

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

What you get: each drifted contact's profile (volume, recent threads, open commitments) so you can decide whether the gap actually matters.

```bash
# Save it as a sidebar lens (TUI and web).
mxr saved add cadence-drift 'cadence:drift'
```

## In real life

- **Monday morning planning:** `mxr cadence drift --format json | jq '.[0:5]'` — the five most-drifted relationships you said you'd maintain. Skim, decide whether to write.
- **Choosing a send slot:** before scheduling a sensitive ask, `mxr send-time alice@example.com --at "thu 16:00"` — switch slots if the proposed window is much slower than her best.
- **Audit of "I'll keep in touch":** `mxr cadence list --format json | jq 'length'` — count how many relationships you've actually committed to keeping warm.
- **Newsletter denial:** `mxr cadence watch news@example.com --every 7d` fails — confirms the screener's list-sender classification is working.

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

- [Pre-send safety](/guides/pre-send-safety/) — where `send-time` attaches as a non-blocking hint
- [Analytics](/guides/analytics/) — reply-latency, response-time, and the underlying `reply_pairs` data
- [Forgotten work](/guides/forgotten-work/) — the inbound side (owed replies) of the same data
- [Search workflow](/guides/search/) — operators that compose with watchlist output
- [CLI — `mxr send-time`](/reference/cli/send-time/), [`mxr cadence`](/reference/cli/cadence/)
