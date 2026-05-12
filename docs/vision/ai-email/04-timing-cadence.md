# Timing And Cadence

Track 4. These features are statistical, local, and cheap. No LLM required.

## Send-Time Optimizer

### Problem

Users often send at a time when the recipient historically replies slowly. mxr
has local reply-pair data and can make this visible.

### Non-Goals

- No automatic scheduling.
- No global productivity advice.
- No server-side tracking pixels or open tracking.

### User Journey

```bash
mxr send-time alice@example.com
mxr send-time alice@example.com --at "fri 19:00"
mxr send <draft-id> --check
```

TUI:

- Compose confirm may show a low-priority timing note:
  "Alice usually replies fastest Tue 09:00-11:00. Current slot averages 36h."

### IPC and Types

Add:

- `Request::SendTimeRecommendation { account_id, recipients, proposed_at }`
- `ResponseData::SendTimeRecommendation { recommendation }`
- `SendTimeRecommendationData { proposed_at, recipient_rows, best_windows, confidence }`
- `RecipientSendTimeRowData { email, sample_count, proposed_expected_reply_seconds, best_expected_reply_seconds, best_windows }`
- `SendWindowData { weekday, hour_start, hour_end, expected_reply_seconds }`

### Store Shape

V1 computes on demand from `reply_pairs`.

Optional cache:

```sql
CREATE TABLE recipient_reply_latency_buckets (
  account_id TEXT NOT NULL,
  email TEXT NOT NULL COLLATE NOCASE,
  weekday INTEGER NOT NULL,
  hour INTEGER NOT NULL,
  sample_count INTEGER NOT NULL,
  p50_seconds INTEGER NOT NULL,
  p90_seconds INTEGER NOT NULL,
  refreshed_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, email, weekday, hour)
);
```

### Algorithm

Data:

- `reply_pairs.direction = they_replied`
- `sent_at` bucketed by local weekday/hour
- latency = their reply delay

Steps:

1. For each recipient, collect reply pairs where account owner sent first and
   recipient replied.
2. Bucket by local weekday/hour of the outbound message.
3. Require minimum sample count:
   - high confidence: >= 20 pairs and >= 3 buckets with data
   - medium: >= 8
   - low: otherwise
4. Smooth sparse buckets using adjacent hours and global recipient median.
5. Compare proposed slot against best windows.
6. Return note only when difference is meaningful, for example proposed p50 is
   at least 2x best p50 and confidence is medium/high.

### Failure Modes

- Too little data: return low confidence and no warning.
- Multiple recipients: compute each, display worst meaningful delta.
- Time zone unavailable: use local machine timezone and label as local.

### Tests

- Known bucket data chooses expected fastest window.
- Low sample count suppresses warning.
- Proposed bad slot generates recommendation.
- Multiple recipients preserve per-recipient rows.
- JSON output has stable weekday/hour fields.

### Acceptance

- `mxr send-time <email>` returns table and JSON.
- `mxr send --check` includes timing info only when confidence is useful.

## Cadence Drift Alert

### Problem

Some relationships matter and go cold. mxr should surface this only for
relationships the user chose to maintain.

### Non-Goals

- No nagging for every contact.
- No LLM.
- No calendar/social CRM.

### User Journey

```bash
mxr cadence watch alice@example.com --every 14d
mxr cadence list
mxr cadence drift
mxr cadence unwatch alice@example.com
```

TUI:

- Analytics page shows watched contacts with drift.
- No modal on startup.

### IPC and Types

Add:

- `Request::WatchCadence { account_id, email, expected_days }`
- `Request::UnwatchCadence { account_id, email }`
- `Request::ListCadenceDrift { account_id, limit }`
- `ResponseData::CadenceDrift { rows }`
- `CadenceWatchData { account_id, email, expected_days, created_at }`
- `CadenceDriftRowData { email, expected_days, days_since_contact, last_contact_at, drift_days }`

### Store Shape

```sql
CREATE TABLE relationship_watchlist (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  email TEXT NOT NULL COLLATE NOCASE,
  expected_days INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, email)
);
```

### Algorithm

1. Watchlist is explicit.
2. Last contact = max(last inbound, last outbound) from `contacts`.
3. Drift = `now - last_contact_at - expected_days`.
4. List only positive drift.
5. Rank by drift days, then relationship volume.

Optional later:

- Suggest watch candidates from frequent contacts, but never auto-watch.

### Failure Modes

- Contact unknown: allow watch row, drift starts after first observed contact.
- List sender: reject by default unless `--allow-list-sender`.
- Multiple accounts: account-specific watch rows.

### Tests

- Watch row round-trips.
- Contact past expected interval appears.
- Contact within expected interval does not appear.
- Unwatch removes row.
- List sender rejected by default.

### Acceptance

- `mxr cadence drift --format json` returns only explicit watched contacts.

