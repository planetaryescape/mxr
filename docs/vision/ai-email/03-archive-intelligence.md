# Archive Intelligence

Track 3 and Track 6. These features make old email useful without pretending
the model "remembers" anything. Retrieval and citations are mandatory.

## Shared Rules

- Lexical/hybrid search remains the retrieval base.
- LLM answers must cite message ids or thread ids.
- No citation means no answer.
- Semantic retrieval broadens recall; structured filters remain authoritative.
- No OCR. Use only indexed message text and extractable attachment text already
  supported by semantic indexing.

## Conversational Archive Query

### Problem

The user wants to ask, "what did Alice and I decide about pricing in Q2?" and
get a grounded answer over their local mail corpus.

### Non-Goals

- No hosted email search.
- No agentic browsing of remote providers.
- No answer without sources.
- No replacing `mxr search`; this is synthesis above search.

### User Journey

```bash
mxr ask "what did Alice and I decide about pricing in Q2?"
mxr ask "what did Alice and I decide about pricing in Q2?" --format json
mxr ask "pricing decisions" --from alice@example.com --after 2026-04-01 --before 2026-06-30
```

TUI:

- Command palette action "Ask archive".
- Results modal shows answer, citations, and open-thread actions.

### IPC and Types

Add:

- `Request::ArchiveAsk { question, filters, limit }`
- `ResponseData::ArchiveAnswer { answer }`
- `ArchiveAskFiltersData { account_id, from, to, after, before, mode }`
- `ArchiveAnswerData { text, citations, retrieval }`
- `ArchiveCitationData { message_id, thread_id, subject, date, quote }`
- `ArchiveRetrievalData { requested_mode, executed_mode, candidate_count }`

### Store Shape

No new canonical table in v1. Optional cache:

```sql
CREATE TABLE archive_answer_cache (
  id TEXT PRIMARY KEY,
  account_id TEXT,
  question_hash TEXT NOT NULL,
  filters_json TEXT NOT NULL,
  answer_json TEXT NOT NULL,
  content_fingerprint TEXT NOT NULL,
  generated_at INTEGER NOT NULL
);
```

Cache is convenience only. It must be invalidated by content fingerprint change.

### Algorithm

1. Parse explicit filters from CLI flags. Do not ask the LLM to enforce dates or
   senders.
2. Query lexical + semantic hybrid with existing search stack.
3. Fetch top candidate messages and thread context snippets.
4. Build an LLM prompt:
   - answer only from provided excerpts
   - cite every claim
   - say "not enough evidence" when citations do not support an answer
5. Validate citation ids are from retrieved candidates.
6. Return Markdown/table and JSON.

### Failure Modes

- Semantic unavailable: fallback to lexical and report executed mode.
- LLM disabled: return top search results plus "synthesis unavailable".
- Citations invalid: reject answer and surface error.
- Too many candidates: truncate by thread diversity before token budget.

### Tests

- Answer cites only retrieved message ids.
- Invalid citation id from mock LLM is rejected.
- Semantic disabled still returns lexical-backed degradation.
- Date filter is enforced before LLM prompt.
- "Not enough evidence" path returns no invented answer.

### Acceptance

- `mxr ask --format json` includes answer text, citations, and retrieval mode.

## Decision Log

### Problem

Important decisions are buried inside long threads. Users need a queryable log:
"agreed on Postgres", "pricing stays manual this quarter", "launch slipped to
June".

### Non-Goals

- No full project-management system.
- No decisions without evidence.
- No automatic mutation of rules/tasks.

### User Journey

```bash
mxr decisions
mxr decisions --topic pricing
mxr decisions rebuild --since 180d
mxr decisions show <decision-id>
```

TUI:

- Sender/profile modal can show recent decisions involving that contact.
- Thread view can show "decisions in this thread".

### IPC and Types

Add:

- `Request::ListDecisionLog { account_id, topic, since_days, limit }`
- `Request::RebuildDecisionLog { account_id, since_days }`
- `ResponseData::DecisionLog { decisions }`
- `DecisionLogEntryData { id, account_id, thread_id, decided_at, decision, topic_tags, participants, evidence }`

### Store Shape

```sql
CREATE TABLE decision_log (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  thread_id TEXT NOT NULL,
  decision TEXT NOT NULL,
  topic_tags_json TEXT NOT NULL DEFAULT '[]',
  participants_json TEXT NOT NULL DEFAULT '[]',
  evidence_msg_ids_json TEXT NOT NULL,
  decided_at INTEGER NOT NULL,
  extracted_at INTEGER NOT NULL,
  source_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'active'
);

CREATE INDEX idx_decision_log_account_time
  ON decision_log(account_id, decided_at DESC);
```

### Algorithm

Extraction is batchable and incremental:

1. Candidate threads: recent multi-message threads, threads with "decided",
   "agreed", "we'll use", "settled", "approved", "go with".
2. Prompt LLM with thread transcript and ask for strict JSON decisions.
3. Keep only entries with message evidence.
4. Stable id = hash(account, thread, normalized decision, evidence ids).
5. Upsert by stable id and source hash.

### Failure Modes

- LLM disabled: command says rebuild requires LLM; list still works.
- Changed thread: source hash refresh updates or marks stale entries.
- Duplicate phrasing: stable id and normalized text prevent duplicate rows.

### Tests

- Extracts explicit "we agreed on Postgres" with evidence.
- Ignores brainstorming without final decision.
- Rebuild idempotent.
- Changed source hash updates entry.
- JSON output round-trips.

### Acceptance

- `mxr decisions --topic pricing --format json` returns cited decision rows.

## Personal Knowledge Graph

### Problem

Users need lightweight explanations for people, projects, and jargon terms:
"Sam = VP Eng at Acme; first appeared 2024-05; topics: hiring, infra."

### Non-Goals

- No heavy graph database.
- No claim without mail evidence.
- No always-on extraction for every token in v1.

### User Journey

```bash
mxr whois sam
mxr whois "Project Apollo" --format json
mxr whois alice@example.com
```

TUI:

- Hover is not relevant in terminal.
- Keybind on focused sender/entity opens a compact explanation modal.

### IPC and Types

Add:

- `Request::ExplainEntity { account_id, query, limit }`
- `ResponseData::EntityExplanation { entity }`
- `EntityExplanationData { canonical_name, kind, summary, first_seen_at, last_seen_at, topics, citations }`

### Store Shape

Add only in Track 6 after archive query and decision log prove useful:

```sql
CREATE TABLE entities (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL,
  canonical_name TEXT NOT NULL,
  kind TEXT NOT NULL,
  aliases_json TEXT NOT NULL DEFAULT '[]',
  summary TEXT,
  first_seen_at INTEGER,
  last_seen_at INTEGER,
  source_hash TEXT NOT NULL DEFAULT ''
);

CREATE TABLE entity_mentions (
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  message_id TEXT NOT NULL,
  thread_id TEXT NOT NULL,
  mention_text TEXT NOT NULL,
  confidence REAL NOT NULL,
  PRIMARY KEY (entity_id, message_id, mention_text)
);
```

### Algorithm

V1 can be query-time without persistence:

1. If query is an email, load sender profile and relationship profile.
2. Else hybrid search for exact phrase and aliases.
3. Summarize top cited mentions.
4. Persist entities only when query-time version becomes slow or repeated.

### Failure Modes

- Ambiguous entity: return candidates, not a synthesized answer.
- No evidence: "No local evidence found."
- LLM disabled: return mentions/search results only.

### Tests

- Email query uses sender/relationship profile.
- Non-email query cites source messages.
- Ambiguous aliases return candidates.
- No evidence does not invent summary.

### Acceptance

- `mxr whois <term> --format json` is machine-readable and citation-backed.

