# Context Briefings

Track 5. Briefings help the user re-enter old context. They should feel like
"oh right" moments, not a second inbox.

## Shared Rules

- Briefings are cached.
- Briefings cite source messages and commitments.
- Briefings are user-invoked or subtle. No modal spam.
- Existing thread summary, relationship profile, commitments, stale/owed data,
  and semantic search are the source material.

## Lost-Context Briefing

### Problem

Opening a thread dormant for a month or more is expensive. The user needs the
state as it was when the thread went quiet: active people, decisions, pending
asks, commitments, and the next likely action.

### Non-Goals

- No full meeting minutes.
- No recap for every thread.
- No synthetic certainty.

### User Journey

```bash
mxr briefing thread <thread-id>
mxr briefing thread <thread-id> --refresh
```

TUI:

- When opening a dormant thread, show a one-line hint:
  "Dormant 47d. Press B for briefing."
- `B` opens a modal.

### IPC and Types

Add:

- `Request::GetThreadBriefing { thread_id, refresh }`
- `ResponseData::ThreadBriefing { briefing }`
- `ThreadBriefingData { thread_id, generated_at, dormant_days, summary, active_people, decisions, pending_commitments, open_questions, citations }`

### Store Shape

```sql
CREATE TABLE context_briefings (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('thread', 'recipient')),
  subject_key TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  briefing_json TEXT NOT NULL,
  generated_at INTEGER NOT NULL,
  expires_at INTEGER
);

CREATE INDEX idx_context_briefings_lookup
  ON context_briefings(account_id, kind, subject_key);
```

### Algorithm

Eligibility:

- Thread latest message older than configured threshold, default 30 days.
- Thread has at least 3 messages or any open commitment/decision.

Source pack:

- Existing thread summary.
- Last N messages, reader-cleaned.
- Relationship profile for top participants.
- Open commitments for participants.
- Decision log entries for thread.

Prompt:

- "Summarize context at time the thread went quiet."
- Preserve uncertainty.
- Return strict JSON with citations.

Cache hash:

- thread content hash
- relationship summary hashes
- open commitment ids/status
- decision ids/source hashes

### Failure Modes

- LLM disabled: return existing thread summary plus structured commitments.
- Invalid citations: reject and show fallback.
- Cache stale: regenerate only on explicit refresh or content hash change.

### Tests

- Dormant threshold gates briefing hint.
- Cached briefing reused when hash unchanged.
- New message invalidates cache.
- Open commitments are included.
- Invalid LLM citation rejected.
- LLM disabled fallback still returns deterministic context.

### Acceptance

- `mxr briefing thread <id> --format json` includes source citations and
  generated timestamp.

## New-Recipient Briefing

### Problem

When composing to someone after a long gap, the user needs recent relationship
context: last interaction, unresolved commitments, and tone/cadence.

### Non-Goals

- No creepy familiarity.
- No social profile scraping.
- No automatic body insertion.

### User Journey

```bash
mxr briefing recipient alice@example.com
mxr compose --to alice@example.com
```

TUI:

- Compose to a recipient with no recent interaction shows a quiet note:
  "Last contact 14mo ago. Press B for context."

### IPC and Types

Add:

- `Request::GetRecipientBriefing { account_id, email, refresh }`
- `ResponseData::RecipientBriefing { briefing }`
- `RecipientBriefingData { email, last_interaction_at, last_thread_id, relationship_summary, open_commitments, tone_note, cadence_note, citations }`

### Store Shape

Use `context_briefings` with:

- `kind = 'recipient'`
- `subject_key = lower(email)`

### Algorithm

Eligibility:

- No prior contact: briefing says "no local history".
- Long gap threshold default: 180 days.
- Always available manually via CLI.

Source pack:

- `contacts` row.
- `sender_profile`.
- relationship summary/style.
- open commitments.
- last 3 shared threads.

Output:

- Last interaction.
- "They were waiting on..." only if a cited open commitment exists.
- Known topics with citations.
- Tone note from deterministic style data.

### Failure Modes

- Unknown contact: no error; return empty history.
- LLM disabled: deterministic profile-only briefing.
- Cloud LLM with relationship privacy disabled: block LLM context and return
  deterministic fallback.

### Tests

- Unknown contact returns empty briefing.
- Long-gap contact includes last interaction.
- Open commitment appears with evidence id.
- Relationship privacy policy blocks cloud synthesis.
- TUI hint appears only past threshold.

### Acceptance

- Compose flow never auto-inserts briefing text into the draft.

