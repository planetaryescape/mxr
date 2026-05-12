# AI Email Roadmap

This directory specifies the useful AI layer for mxr. It is a roadmap, not a
code change. Implement it over multiple sessions, smallest vertical slice
first.

The product rule is strict: AI features must help with an actual email job.
They must reduce mistakes, recover forgotten work, find old context, or improve
timing. Do not ship features because they are easy to demo.

## Principles

- Local-first by default. SQLite remains source of truth. Embeddings and derived
  summaries are rebuildable.
- Deterministic first. Regex, reply-pair statistics, address history, and
  explicit config run before any LLM call.
- CLI first. Every capability has a CLI and JSON/JSONL surface before or with
  TUI support.
- TUI support uses daemon IPC. No TUI-only intelligence.
- Citations required. Synthesis features must cite source messages or threads.
- Warnings are reviewable. mxr may warn or block pending explicit override, but
  must never silently rewrite, add recipients, or send.
- Provider-agnostic. Provider quirks stay below the core mail model.
- Privacy explicit. Relationship data can leave the machine only when existing
  LLM privacy config allows it.

## Current Infra Map

Use these before adding new subsystems:

| Need | Existing base |
|---|---|
| Local retrieval | `mxr-search`, `mxr-semantic`, hybrid search, saved searches |
| LLM calls | `mxr-llm`, feature-specific runtime overrides |
| Thread synthesis | `crates/daemon/src/handler/summarize.rs`, `thread_summaries` |
| Draft generation | `draft-assist`, `draft-new`, `draft-refine` handlers |
| Voice and tone | `mxr-relationship`, `contact_style`, `user_voice_profile`, voice match |
| Commitments | `contact_commitments`, relationship service extraction |
| Response timing | `reply_pairs`, `mxr response-time` |
| Forgotten replies | `mxr stale`, `message_flags`, reply-later queue |
| Relationship cadence | `contacts`, `mxr contacts decay` |
| Deterministic writing checks | `mxr-humanizer`, compose validation |
| Searchable old content | semantic chunks, lexical Tantivy index, LLM export |

## Feature Matrix

| Job | Feature | Track | First surface |
|---|---|---:|---|
| Pre-send safety | Wrong recipient | 1 | `mxr send --check` |
| Pre-send safety | Missing attachment | 1 | `mxr send --check` |
| Pre-send safety | Reply-all sanity | 1 | `mxr send --check` |
| Pre-send safety | PII/secrets preview | 1 | `mxr send --check` |
| Pre-send safety | Tone mismatch | 1 | `mxr send --check` |
| Pre-send safety | Answer coverage | 1 | `mxr send --check` |
| Forgotten work | Outgoing commitments | 2 | `mxr commitments` |
| Forgotten work | Owed reply lens | 2 | `mxr owed` |
| Archive intelligence | Conversational archive query | 3 | `mxr ask` |
| Archive intelligence | Decision log | 3 | `mxr decisions` |
| Archive intelligence | Personal knowledge graph | 6 | `mxr whois` |
| Timing | Send-time optimizer | 4 | `mxr send-time` |
| Timing | Cadence drift alert | 4 | `mxr cadence` |
| Old-content onboarding | Lost-context briefing | 5 | `mxr briefing thread` |
| Old-content onboarding | New-recipient briefing | 5 | `mxr briefing recipient` |
| Collaboration | Maybe include | 5 | `mxr suggest-recipients` |
| Collaboration | Who's the expert | 5 | `mxr expert` |

Tracks 1-2 are the top-priority daily-utility work. Tracks 3-6 are staged so
they reuse stable safety, relationship, commitment, and citation primitives.

## Do Not Build

- 3-button Smart Reply. It does not fit keyboard-native email and creates
  throwaway text.
- Magic-wand "rewrite in style X" without relationship grounding. Voice match
  and draft refine can exist, but must be grounded.
- AI emoji insertion.
- Uncited archive answers.
- Always-on relationship nags. Cadence alerts require an explicit watchlist.
- Auto-CC or auto-forward. Suggestions only.

## Shared Public Interfaces

These are planned protocol shapes, not implemented yet:

```rust
Request::CheckDraftSafety {
    draft: Draft,
    context: DraftSafetyContextData,
}

ResponseData::DraftSafetyReport {
    report: DraftSafetyReportData,
}
```

Core data:

- `DraftSafetyReportData { draft_id, verdict, issues, generated_at }`
- `DraftSafetyIssueData { kind, severity, title, detail, citations, override_token }`
- `SafetySeverityData = Info | Warning | Blocker`
- `SafetyCheckKindData = WrongRecipient | MissingAttachment | ReplyAll | PiiSecret | ToneMismatch | AnswerCoverage | CommitmentCandidate`
- `CitationRefData { message_id, thread_id, quote, field }`
- `SuggestedRecipientData { email, display_name, reason, evidence }`

Rules:

- `SendDraft` and `SendStoredDraft` call the same safety pipeline before provider
  send.
- `--check` runs only the report and exits.
- `--yes` does not bypass blockers. A separate explicit override is required.
- `--format json|jsonl` returns stable structured reports.

## Docs

| File | Purpose |
|---|---|
| [01-pre-send-safety.md](./01-pre-send-safety.md) | Safety pipeline and six checks |
| [02-follow-up-work.md](./02-follow-up-work.md) | Commitments and owed-reply lens |
| [03-archive-intelligence.md](./03-archive-intelligence.md) | Archive query, decision log, knowledge graph |
| [04-timing-cadence.md](./04-timing-cadence.md) | Send-time and cadence features |
| [05-context-briefings.md](./05-context-briefings.md) | Lost-context and new-recipient briefings |
| [06-collaboration-loop-in.md](./06-collaboration-loop-in.md) | CC suggestions and expert finding |
| [07-build-sequence.md](./07-build-sequence.md) | Cross-session implementation order |

