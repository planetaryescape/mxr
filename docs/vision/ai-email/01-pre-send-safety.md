# Pre-Send Safety

Track 1. Highest daily utility. This is one composable pre-send pipeline, not
six unrelated prompts.

## Shared Pipeline

### Problem

Sending is the highest-risk email action. The mistakes are mundane: wrong
recipient, missing attachment, reply-all blast, secret pasted into body,
unexpected tone, or failing to answer the actual ask.

### Non-Goals

- No grammar checker.
- No generic "make this better" rewrite.
- No automatic recipient edits.
- No cloud DLP.
- No irreversible send without explicit user confirmation when a blocker exists.

### User Journey

CLI:

```bash
mxr send <draft-id> --check
mxr send <draft-id>
mxr send <draft-id> --override-safety <token>
mxr compose --to alice@example.com --body "see attached" --check
```

TUI:

- Compose confirm runs safety automatically.
- Warnings show in a compact modal before send.
- `enter` accepts non-blocking warnings.
- Blockers require copying/confirming the override token or editing the draft.

### IPC and Types

Add:

- `Request::CheckDraftSafety { draft, context }`
- `ResponseData::DraftSafetyReport { report }`
- `DraftSafetyContextData { mode, reply_all, original_message_id, thread_id, allow_llm }`
- `DraftSafetyReportData { verdict, issues, checked_at }`
- `DraftSafetyVerdictData = Safe | Warn | Blocked`
- `DraftSafetyIssueData { kind, severity, title, detail, citations, override_token }`

Use the same `check_draft_safety` function from:

- CLI `mxr send --check`
- TUI compose confirm
- daemon `SendDraft`
- daemon `SendStoredDraft`
- scheduled-send flusher, before firing due sends

### Store Shape

Track 1 can be mostly stateless. Persist only audit and override data:

```sql
CREATE TABLE draft_safety_runs (
  id TEXT PRIMARY KEY,
  draft_id TEXT,
  account_id TEXT NOT NULL,
  verdict TEXT NOT NULL,
  issues_json TEXT NOT NULL,
  checked_at INTEGER NOT NULL
);

CREATE TABLE draft_safety_overrides (
  token TEXT PRIMARY KEY,
  draft_id TEXT,
  issue_kinds_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  used_at INTEGER
);
```

Do not store PII snippets longer than needed for the warning. Prefer issue kind,
field, and redacted preview.

### Failure Modes

- Store unavailable: fail closed for `SendStoredDraft`; report error for
  `--check`.
- LLM disabled/unreachable: deterministic checks still run; LLM-backed checks
  become `Info` with degradation reason.
- Safety pipeline panic/error: send is blocked, because a broken safety gate must
  not silently pass.
- Scheduled send hits a blocker: keep draft, clear schedule, emit event.

## Wrong Recipient Detector

### Problem

Autocomplete mistakes are costly, especially when two contacts share names or a
company-internal message goes to an external/competitor domain.

### Algorithm

Inputs:

- Draft To/Cc/Bcc.
- `contacts` table.
- `account_addresses`.
- Recent sender/profile data.
- Optional user config:

```toml
[safety.recipients]
internal_domains = ["company.com"]
sensitive_domains = ["competitor.com"]
warn_on_first_time_external = true
```

Checks:

1. Normalize all recipient emails.
2. Compare recipient local-parts and display names against known contacts using
   a small edit-distance helper. Add a dependency only if a battle-tested crate
   is chosen; otherwise keep the local implementation tiny and documented.
3. Flag if a recipient is within typo distance of a more frequent contact and
   the chosen recipient has weak history.
4. Flag if body/subject contains internal markers and a recipient domain is not
   internal.
5. Flag configured sensitive domains.
6. Flag first-time external recipients when enabled.

Severity:

- Blocker: configured sensitive domain or likely internal leak.
- Warning: close typo candidate or first-time external.

### Tests

- Typo-distance warns for `alcie@example.com` when `alice@example.com` is common.
- Does not warn when both addresses have strong prior relationship.
- Internal body plus external domain blocks.
- Configured sensitive domain blocks.
- Account's own addresses are ignored.

### Acceptance

- `mxr send --check` reports a cited recipient issue before provider send.
- `SendDraft` refuses blocker without override.
- JSON output includes candidate intended recipient and reason.

## Missing Attachment Detector

### Problem

Users write "attached" and forget the file.

### Algorithm

Pure deterministic regex over subject + body:

- match: `attached`, `see attached`, `attachment`, `enclosed`, `I've attached`,
  `I have attached`, `attaching`
- avoid obvious false positives: `not attached`, `without attachment`, quoted
  context stripped by compose parser

If match and `draft.attachments.is_empty()`, issue warning.

### Tests

- "see attached" with zero files warns.
- Same body with one file passes.
- "not attached" does not warn.
- Context block in reply does not trigger.

### Acceptance

- Runs without LLM.
- Warning is visible in CLI table and JSON.

## Reply-All Sanity

### Problem

Reply-all is often accidental when the body clearly addresses one person.

### Algorithm

Inputs:

- `DraftSafetyContextData.reply_all`
- `draft.to + cc`
- body first 500 chars
- known display names from current thread

Checks:

1. If not reply-all, pass.
2. If total visible recipients <= 2, pass.
3. Extract direct address cues: `Hi Alice`, `Alice,`, `hey alice`, first-name
   vocative in first paragraph.
4. If exactly one non-self participant is named and group language is absent
   (`team`, `all`, `folks`, `everyone`), warn.

Severity: warning. Never blocker.

### Tests

- Reply-all to 6 people with "Alice," warns.
- Reply-all to 6 people with "Team," passes.
- Direct reply with one recipient passes.
- Names in quoted context are ignored.

### Acceptance

- Warning says which single person appears addressed.

## PII and Secrets Preview

### Problem

Users paste secrets or regulated identifiers into drafts.

### Non-Goal

This is not a compliance-grade DLP engine.

### Algorithm

Local-only detectors:

- SSN-shaped values: `###-##-####`
- Credit cards: digit groups passing Luhn, excluding short random numbers
- API keys/secrets: common prefixes and assignments:
  - `sk-...`
  - `ghp_...`
  - `xoxb-...`
  - `AWS_ACCESS_KEY_ID`
  - `AWS_SECRET_ACCESS_KEY`
  - `api_key=`
  - `client_secret=`
  - PEM private key header

Return redacted previews:

- `sk-...abcd`
- `**** **** **** 4242`
- `***-**-1234`

Severity:

- Blocker for private keys and obvious API secrets.
- Warning for SSN/card-like values.

### Tests

- Luhn-valid card warns.
- Luhn-invalid digit group passes.
- PEM private key blocks.
- Config docs mentioning `client_secret` outside a draft do not matter; only
  draft body/frontmatter is checked.

### Acceptance

- No raw secret appears in JSON, logs, or TUI modal.

## Tone-Mismatch Warning

### Problem

Users sometimes send mail that is much more casual or formal than their normal
relationship with the recipient.

### Algorithm

Use existing relationship style:

- `contact_style.formality_score`
- `contact_style.avg_sentence_len`
- `mxr_relationship::score_voice_match`
- user voice profile fallback when no contact baseline

For each primary recipient:

1. Load contact style.
2. Compute draft stylometry.
3. Score voice match.
4. Warn only when confidence is medium/high and score below threshold.

Copy should be specific:

- "Tone is more formal than usual with alice@example.com."
- "Tone is more casual than usual with alice@example.com."

Severity: warning.

### Tests

- No warning when fewer than 3 prior sent messages exist.
- Warns when formality delta exceeds threshold with high confidence.
- Does not call LLM.

### Acceptance

- Uses deterministic local metrics.
- Configurable threshold under `[safety.tone]`.

## Answer-Coverage Check

### Problem

Replies often answer only part of the thread. The user needs a quick "what did
I miss?" before sending.

### Algorithm

LLM-backed, citation required:

1. Load current thread messages through reader-cleaned text.
2. Extract explicit asks from latest inbound messages first.
3. Ask LLM for strict JSON:

```json
{
  "asks": [
    {
      "question": "pricing timeline",
      "evidence_msg_id": "...",
      "addressed": true,
      "draft_evidence": "..."
    }
  ]
}
```

4. Validate every ask has evidence message id from the loaded thread.
5. If no asks or LLM disabled, degrade with info.
6. Warn when unaddressed asks remain.

Severity: warning.

### Tests

- Three asks, draft covers two, warning names missing ask.
- LLM output with unknown message id is rejected.
- LLM disabled returns deterministic checks only.
- Prompt includes thread transcript and draft body.

### Acceptance

- Output says "Alice asked 3 questions; draft addresses 2."
- Missing ask cites message id and short quote.

