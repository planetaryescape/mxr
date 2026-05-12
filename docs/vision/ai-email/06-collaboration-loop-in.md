# Collaboration And Loop-In

Track 5. These features help choose people. They never add people.

## Shared Rules

- Suggestions only.
- Cite prior threads or messages.
- Exclude self addresses.
- Respect Bcc: never cite or suggest from Bcc unless the current user was the
  sender and local data has the Bcc.
- No provider-specific contact APIs in core.

## Maybe Include

### Problem

When composing about a topic, users often forget the colleague who is normally
included on that subject.

### Non-Goals

- No automatic CC.
- No org chart.
- No suggestion from a single weak coincidence.

### User Journey

```bash
mxr suggest-recipients --draft <draft-id>
mxr suggest-recipients --subject "pricing rollout" --body-stdin
```

TUI:

- Compose confirm shows "Maybe include Bob" as an info-level suggestion.
- User can press a key to add to Cc, or ignore.

### IPC and Types

Add:

- `Request::SuggestCollaborators { draft, limit }`
- `ResponseData::SuggestedCollaborators { suggestions }`
- `SuggestedRecipientData { email, display_name, reason, confidence, evidence }`

### Store Shape

V1 computes on demand from existing messages.

Optional later cache:

```sql
CREATE TABLE collaborator_patterns (
  account_id TEXT NOT NULL,
  topic_hash TEXT NOT NULL,
  email TEXT NOT NULL COLLATE NOCASE,
  support_count INTEGER NOT NULL,
  evidence_thread_ids_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, topic_hash, email)
);
```

### Algorithm

1. Build topic query from subject + first body paragraph.
2. Hybrid search prior sent and received threads.
3. Filter to threads with at least one current recipient or same topic.
4. Count participants who appear frequently in similar threads but are absent
   from current To/Cc.
5. Score:
   - topic similarity
   - co-participation count
   - recency
   - existing relationship strength
6. Require minimum support, default 3 distinct threads.
7. Return suggestions with evidence thread ids.

### Failure Modes

- Semantic unavailable: lexical topic search only.
- Low support: return no suggestions.
- Contact is already Bcc: do not suggest or reveal.
- Distribution lists: suppress by default unless repeatedly used by user.

### Tests

- Repeated similar threads with Bob suggest Bob.
- One-off thread does not suggest.
- Existing To/Cc recipients excluded.
- Self addresses excluded.
- Bcc evidence is not leaked.
- Suggestion cites at least two source threads when confidence medium/high.

### Acceptance

- `mxr suggest-recipients --format json` returns stable suggestions and evidence.

## Who's The Expert

### Problem

For inbound questions the user would forward, mxr can identify people in the
local corpus who have answered similar questions before.

### Non-Goals

- No ranking by job title.
- No scraping Slack/Calendar.
- No auto-forward.

### User Journey

```bash
mxr expert <message-id>
mxr expert --query "Who knows about DKIM setup?"
```

TUI:

- On a message, command palette action "Find expert".
- Result modal shows people, why, and cited threads.

### IPC and Types

Add:

- `Request::FindExpert { account_id, message_id, query, limit }`
- `ResponseData::ExpertSuggestions { experts }`
- `ExpertSuggestionData { email, display_name, score, reason, answered_threads, citations }`

### Store Shape

V1 computes from messages and semantic search.

Optional materialized expertise table:

```sql
CREATE TABLE expertise_index (
  account_id TEXT NOT NULL,
  email TEXT NOT NULL COLLATE NOCASE,
  topic TEXT NOT NULL,
  evidence_thread_ids_json TEXT NOT NULL,
  score REAL NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (account_id, email, topic)
);
```

Do not add this until query-time implementation proves slow.

### Algorithm

1. Build query from selected message body or explicit query.
2. Hybrid search similar historical threads.
3. Identify participants whose messages appear to answer the question:
   - their message follows a question
   - their message contains explanatory content
   - thread later has thanks/confirmation or no further unresolved ask
4. Aggregate by email.
5. Exclude current sender unless useful and explicit.
6. Return top candidates with cited answer messages.

LLM can improve reason text, but ranking should work without it.

### Failure Modes

- No similar threads: return no expert.
- LLM disabled: deterministic evidence snippets only.
- Ambiguous broad query: ask user to narrow via CLI error? No. Return low
  confidence rows with explanation.

### Tests

- Similar prior thread with Bob's answer ranks Bob.
- Person who asked similar questions but did not answer is not ranked.
- Current thread participants handled correctly.
- Citations point to answer messages, not only matching questions.
- LLM disabled path still returns deterministic suggestions.

### Acceptance

- `mxr expert <message-id> --format json` is enough for scripts/agents to use.

