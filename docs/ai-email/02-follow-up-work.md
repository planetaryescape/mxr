# Forgotten And Dropped Work

Track 2. This closes the loop between writing mail and remembering what the
mail created.

## Shared Defaults

- Reuse `contact_commitments` before adding a new ledger.
- Commitments are explicit only. No inferred work.
- Direction matters: `yours` means the account owner owes work; `theirs` means
  the contact owes work.
- Every commitment has evidence message id and thread id.
- CLI and JSON surfaces ship before TUI polish.

## Outgoing Commitment Extraction

### Problem

The user writes "I'll send the deck Friday" and mxr already has a relationship
commitment ledger. The send path should capture that promise before it becomes
forgotten work.

### Non-Goals

- No calendar/task-app integration in v1.
- No inferred commitments from vague politeness.
- No auto-resolve based on semantic guesses.

### User Journey

```bash
mxr send <draft-id> --check
mxr send <draft-id>
mxr commitments --status open
mxr commitments --contact alice@example.com
```

Flow:

1. User sends or checks a draft.
2. Safety pipeline extracts explicit outgoing commitments.
3. On `--check`, candidates are shown but not persisted as final commitments.
4. On successful send and local sent ingest, candidates are written to
   `contact_commitments`.
5. User can resolve later with `mxr commitments resolve <id>`.

### IPC and Types

Extend safety report with:

- `SafetyCheckKindData::CommitmentCandidate`
- `CommitmentCandidateData { who_owes, what, by_when, direction, evidence_draft_span }`

Add optional request:

- `Request::ExtractDraftCommitments { draft }`
- `ResponseData::DraftCommitments { candidates }`

This helper is useful for tests and CLI, but the send path still calls the same
internal function.

### Store Shape

Use existing `contact_commitments` for final rows.

Add a transient candidate table only if needed for stored drafts and scheduled
sends:

```sql
CREATE TABLE draft_commitment_candidates (
  id TEXT PRIMARY KEY,
  draft_id TEXT NOT NULL,
  account_id TEXT NOT NULL,
  email TEXT NOT NULL COLLATE NOCASE,
  direction TEXT NOT NULL CHECK (direction IN ('yours', 'theirs')),
  who_owes TEXT NOT NULL,
  what TEXT NOT NULL,
  by_when INTEGER,
  evidence_text TEXT NOT NULL,
  extracted_at INTEGER NOT NULL,
  promoted_at INTEGER
);
```

Promotion creates `contact_commitments` after send success. Candidate ids are
draft-scoped. Final commitment ids include account, contact, sent message id,
direction, and normalized `what`.

### Algorithm

Deterministic prefilter:

- Look for first-person commitments: `I'll`, `I will`, `I can`, `I'll send`,
  `I'll follow up`, `I'll check`, `I'll get back`.
- Look for due phrases: weekday, date, `tomorrow`, `next week`, `by EOD`.

LLM extraction:

- Uses `LlmFeature::Commitments`.
- Prompt includes draft body, recipients, and reply thread title.
- Strict JSON only.
- Validate non-empty `what`, direction, and recipient evidence.

Fallback:

- If LLM disabled, deterministic matches become low-confidence candidates in
  safety report, not final ledger rows unless user confirms.

### Failure Modes

- LLM disabled: report candidate hint only.
- Send succeeds but candidate promotion fails: send receipt still returns;
  event log warns and `mxr doctor` can expose "unpromoted candidates".
- Scheduled send: re-run extraction at fire time because draft may have changed.

### Tests

- Draft "I'll send the deck Friday" yields candidate.
- Candidate is not final after `--check`.
- Candidate promotes after successful `SendStoredDraft`.
- Failed provider send does not promote.
- Duplicate resend/idempotent send does not duplicate commitment.
- LLM JSON with empty `what` is ignored.

### Acceptance

- After send, `mxr commitments --status open --format json` includes the
  outgoing promise with sent message evidence.

## Owed Reply Inbox Lens

### Problem

Users need a first-class view of threads where they are the bottleneck, ranked
by urgency relative to their own behavior.

### Non-Goals

- No generic "unread older than N days" folder.
- No LLM required for v1.
- No nagging for newsletters or list senders.

### User Journey

```bash
mxr owed
mxr owed --since 90d --format json
mxr saved add owed "is:owed-reply"
```

TUI:

- Sidebar lens "Owed".
- Sort by overdue score.
- Press reply; successful send removes from lens.

### IPC and Types

Add:

- `Request::ListOwedReplies { account_id, limit, older_than_days, within_days }`
- `ResponseData::OwedReplies { rows }`
- `OwedReplyRowData { thread_id, latest_message_id, counterparty_email, subject, waiting_days, cadence_days_p50, overdue_score }`

Search operator:

- `is:owed-reply`

### Store Shape

Likely no new table for v1. Build from:

- `messages`
- `reply_pairs`
- `contacts.cadence_days_p50`
- `message_flags`
- `screener_decisions`

If performance needs it later, add a materialized `owed_reply_threads` table
refreshed by analytics rebuild.

### Algorithm

1. Start with stale threads where latest message is inbound and non-self.
2. Exclude list senders, screener-denied/feed senders, trash/spam, archived-only
   if user config says so.
3. Join contact cadence.
4. Compute expected reply window:
   - use contact `cadence_days_p50` if present
   - else use global p50 from `reply_pairs`
   - else default 7 days
5. `overdue_score = waiting_days / expected_days`, capped for display.
6. Rank by score, then waiting days.

### Failure Modes

- Missing contacts: use global/default cadence.
- Bad date headers: respect existing plausible date floor.
- Huge mailbox: cap candidate windows, then add materialized view if needed.

### Tests

- Latest inbound older than cadence appears.
- Latest outbound thread does not appear.
- List sender excluded.
- Contact-specific cadence changes ranking.
- Sending reply removes row.
- `is:owed-reply` matches daemon list.

### Acceptance

- `mxr owed --format ids` prints thread ids in the same order as table output.
- TUI owed lens and CLI use the same IPC request.

