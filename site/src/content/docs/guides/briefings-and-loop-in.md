---
title: Briefings and loop-in
description: Re-enter dormant threads, suggest who to Cc, find local experts, and look people up — all citation-backed.
---

Four related "context recovery" surfaces. Each one starts from the same source pack — your local mail, the relationship engine's profiles, and the lexical/semantic indexes — and produces a small, cited answer instead of a dashboard.

| Surface | Question it answers |
|---|---|
| `mxr briefing thread` | What was this dormant thread about when it went quiet? |
| `mxr briefing recipient` | What's the state of my relationship with this person right now? |
| `mxr suggest-recipients` | Who normally gets Cc'd on this kind of thread? |
| `mxr expert` | Who's answered something like this question before? |
| `mxr whois` | Who/what is this name? |

:::tip[The one-line mental model]
These features **suggest**, never **mutate**. None of them auto-Cc, auto-summarize a thread into your draft, or invent details the corpus doesn't support. Every output cites a message id.
:::

## Thread briefing — `mxr briefing thread`

When a thread has been dormant for a month or more, opening it cold is expensive. `mxr briefing thread` synthesizes a structured recap from the existing thread summary, the relationship profile, open commitments, and decision-log entries.

```bash
mxr briefing thread THREAD_ID
mxr briefing thread THREAD_ID --format json
```

What you get: `{ thread_id, generated_at, dormant_days, summary, active_people, decisions, pending_commitments, open_questions, citations }`. Each section either has cited message ids or is omitted — empty fields are not papered over with prose.

### Cached, hash-invalidated

The briefing is cached in `context_briefings` keyed by thread id and a content hash that includes the thread itself, the relationship summary, open commitments, and decision-log entries. New mail or a refreshed relationship summary invalidates the cache; a duplicate request returns instantly.

```bash
# Force a regenerate (e.g. after a fresh decision-log rebuild):
mxr briefing thread THREAD_ID --refresh
```

### When the LLM is off

With `[llm] enabled = false`, `mxr briefing thread` returns the deterministic source pack — existing thread summary + structured commitments + decision entries — and labels the synthesized fields as "LLM disabled". No invented summaries.

```bash
mxr briefing thread THREAD_ID --format json \
  | jq '{commitments: .pending_commitments, decisions, citations}'
```

## Recipient briefing — `mxr briefing recipient`

Same primitives, different subject. Useful before writing to someone after a long gap.

```bash
mxr briefing recipient alice@example.com
mxr briefing recipient alice@example.com --format json
```

What you get: `{ email, last_interaction_at, last_thread_id, relationship_summary, open_commitments, tone_note, cadence_note, citations }`. `tone_note` and `cadence_note` come from deterministic stylometry — they exist even when the LLM is off.

The TUI compose flow shows a quiet `Last contact 14mo ago. Press B for context.` hint when you're composing to someone past the gap threshold (default 180 days). The hint is never auto-inserted into the draft body — only shown above it.

### Privacy

`mxr briefing recipient` consults the relationship profile, which can contain personal context (topics discussed, commitments owed, communication style). When `[llm.relationship_privacy] allow_cloud = false`, the synthesis path is blocked for cloud LLMs and a deterministic fallback is returned instead. Local LLMs see the full profile.

## Maybe include — `mxr suggest-recipients`

Suggests Cc candidates who frequently appear on similar prior threads but are absent from this draft. Suggestions, not actions.

```bash
mxr suggest-recipients --draft DRAFT_ID --format json
```

```bash
# Ephemeral draft (no stored row):
echo 'rollout plan attached' \
  | mxr suggest-recipients --subject "pricing rollout" --body-stdin --format json
```

What you get: `{ email, display_name, reason, confidence, evidence }` rows. `evidence` is a list of thread ids that justified the suggestion; `reason` is a short string like `"co-participated on 4 similar threads in the last 90 days"`.

### Hard rules

- **Minimum support is 3 distinct threads** by default. One-off coincidences don't fire.
- **Self addresses are excluded.** Account-owned addresses can't suggest themselves.
- **Bcc is never leaked.** Even when local data contains a Bcc'd recipient on a prior thread, that address never appears as a suggestion.
- **Existing recipients are excluded.** Anyone already on the draft's To/Cc/Bcc is filtered out before scoring.

### Composing with compose

```bash
# Pre-check before sending: who else usually gets this?
mxr suggest-recipients --draft DRAFT_ID --format json \
  | jq -r '.[] | "\(.email)\t\(.confidence)\t\(.reason)"'

# Then decide manually whether to add — mxr never edits To/Cc for you.
mxr drafts edit DRAFT_ID
```

## Who's the expert — `mxr expert`

For an inbound question you'd otherwise forward, `mxr expert` ranks people in your local corpus who have answered similar questions before — ranked by their *answers*, not their *questions*.

```bash
# Start from a specific message:
mxr expert MESSAGE_ID --format json

# Start from a free-text query:
mxr expert --query "Who knows about DKIM setup?" --format json
```

What you get: `{ email, display_name, score, reason, answered_threads, citations }` rows. Every citation points at an *answer* message — a reply that follows a question, contains explanatory content, and was followed by thanks/confirmation or no further unresolved ask in the same thread.

### Ranking shape

| Signal | Weight |
|---|---|
| Their message follows a question on the topic | Required |
| Their message contains explanatory content (length, structure) | Strong |
| Thread later has "thanks"/"that worked" or no further ask | Strong |
| They are a current thread participant | Excluded by default (`--include-self` to keep) |

### When the LLM is off

Ranking is deterministic. The LLM is used only to improve the `reason` text into a short human-readable phrase. Without it, `reason` is a structured signal list and the rest of the row is unchanged.

```bash
# Compose with --include-self when you're the expert and want to confirm:
mxr expert MESSAGE_ID --include-self --format json
```

## Personal knowledge graph — `mxr whois`

Lightweight, citation-required explanations for people, projects, and jargon. Query-time only — there is no persisted entity table in v1, by design.

```bash
mxr whois sam
mxr whois alice@example.com --format json
mxr whois "Project Apollo" --limit 20 --format json
```

What you get: `{ canonical_name, kind, summary, first_seen_at, last_seen_at, topics, citations }`. `kind` is `person`, `email`, `project`, or `term`. When evidence doesn't support a confident answer, `summary` is omitted and `candidates` lists the alternatives instead — never a synthesized definition.

### Email vs. free-text

When the query is an email, the path uses the sender/relationship profile directly. Otherwise the path is a hybrid search over messages and a citation pass that filters mentions down to confident ones. The same `--limit` controls the citation budget either way.

```bash
# Disambiguate an ambiguous name:
mxr whois "Sam" --format json | jq '.candidates'
```

What you get: a list of candidate entities with first/last-seen timestamps — pick the one you mean and re-query with the email or a more specific phrase.

## In real life

- **Reopening a 3-month-old thread:** `mxr briefing thread THREAD_ID --format json` first; reply with confidence, having read the decisions and pending commitments.
- **Composing to a contact after a year:** `mxr briefing recipient alice@example.com` before opening the draft; check the `last_interaction_at` and any open commitment they're owed.
- **Forwarding an inbound question:** `mxr expert MESSAGE_ID --format json | jq '.[0]'`; forward to the top answerer with a one-line context.
- **Reading a meeting note that mentions "Sam":** `mxr whois Sam --format json | jq '.candidates // .canonical_name'`; disambiguate without leaving the terminal.
- **Pre-send Cc check:** `mxr suggest-recipients --draft DRAFT_ID --format json` before hitting send on a topic you don't normally own — your normal collaborators may be missing.

## Agent prompts that work

```text
"Before I reply on dormant thread THREAD_ID, run `mxr briefing thread
THREAD_ID --format json`. Quote the `.summary`, list each
`.pending_commitments[].what` with its `evidence_msg_id`, and end with
a one-line recommended next step."
```

```text
"For inbound MESSAGE_ID, run `mxr expert MESSAGE_ID --format json` and
propose the top expert with a one-sentence forward note grounded in
their cited answer message. Don't forward — show me first."
```

```text
"Before I send DRAFT_ID, run `mxr suggest-recipients --draft DRAFT_ID
--format json`. List suggestions with `confidence >= medium` and a
one-line reason. Ask me which to add. Never edit the draft yourself."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/) — the gate the suggest-recipients hint appears inside
- [Archive intelligence](/guides/archive-intelligence/) — same citation discipline for `mxr ask` / `mxr decisions`
- [LLM features](/guides/llm-features/) — configure the model used for briefings, expert reasons, and whois synthesis
- [Sender view](/guides/sender-view/) — the per-contact view that links into recipient briefings
- [CLI — `mxr briefing`](/reference/cli/briefing/), [`mxr suggest-recipients`](/reference/cli/suggest-recipients/), [`mxr expert`](/reference/cli/expert/), [`mxr whois`](/reference/cli/whois/)
