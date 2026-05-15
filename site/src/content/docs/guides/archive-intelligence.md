---
title: Archive intelligence
description: Ask grounded questions over your local mail, with citations, and keep a queryable decision log.
---

Years of mail accumulate facts: prices agreed, deadlines slipped, vendors picked. `mxr ask` and `mxr decisions` make that corpus answerable without pretending the model "remembers" anything. Retrieval runs on the local lexical and semantic indexes; every claim cites a message id; uncited LLM output is rejected before it reaches you.

:::tip[The one-line mental model]
`mxr ask` is **synthesis above search** — every answer cites retrieved messages, or returns "not enough evidence". `mxr decisions` is **a queryable ledger** built from threads that contain explicit decisions ("we agreed on Postgres"), keyed by stable id and source hash so rebuilds are idempotent.
:::

## Conversational archive query — `mxr ask`

```bash
mxr ask "what did Alice and I decide about pricing in Q2?"
```

What you get: a Markdown answer that names the decision, followed by `Citations:` with the cited message ids and short quotes. If the retrieved candidates don't support an answer, the response is literally "not enough evidence" — no synthesized confidence, no invented dates.

### Filters run before the prompt

Date and sender filters are enforced by the daemon, not the LLM. The model only ever sees messages the filter already accepted.

```bash
# Narrow to a person and a date range:
mxr ask "pricing decisions" \
  --from alice@example.com \
  --after 2026-04-01 --before 2026-06-30 \
  --format json
```

What you get: a JSON object `{ text, citations, retrieval }`. `citations` lists `{ message_id, thread_id, subject, date, quote }`. `retrieval` shows the requested vs. executed mode (`hybrid`, `lexical`, `semantic`) and `candidate_count` — so degraded queries are visible, not silent.

### Retrieval modes

```bash
# Hybrid (default): lexical BM25 + semantic, reciprocal-rank fused.
mxr ask "infra postmortems" --mode hybrid

# Lexical-only — useful when the semantic index is rebuilding,
# or when you want exact-term matches.
mxr ask "INC-1234 root cause" --mode lexical

# Semantic-only — broader recall on paraphrase-heavy questions.
mxr ask "what did we say about latency budgets" --mode semantic
```

When `--mode semantic` is requested but the semantic profile isn't ready, the daemon falls back to lexical and reports the executed mode in the JSON. Scripts can branch on `.retrieval.executed_mode`.

### Composing with `mxr cat`

The cited message ids are scriptable:

```bash
mxr ask "who owns the legal review for v2" --format json \
  | jq -r '.citations[].message_id' \
  | xargs -I{} mxr cat {} --view reader
```

What you get: every cited message rendered with reader mode, in order, so you can verify the synthesis yourself.

### When the LLM is off

`mxr ask` requires the LLM for synthesis. With `[llm] enabled = false`, the command returns the top retrieved candidates and a `synthesis unavailable` notice — so you still get the search hits, you just don't get an answer paragraph. Combine with `mxr search` if you need a pure retrieval surface.

```bash
# Equivalent retrieval surface without synthesis:
mxr search 'from:alice "pricing" newer_than:90d' --format json
```

## The decision log — `mxr decisions`

Important decisions land in long threads. `mxr decisions` extracts them into a stable ledger you can query by topic, time, or stable id. Each row carries the threads and message ids that justified the extraction.

```bash
# List the most recent decisions.
mxr decisions --format json

# Filter by topic tag (extracted by the LLM during rebuild):
mxr decisions --topic pricing --since 180 --format json

# Show one row in full (every evidence message + participants):
mxr decisions show DECISION_ID --format json
```

What you get: rows `{ id, thread_id, decision, topic_tags, participants, evidence, decided_at, status }`. `id` is `hash(account, thread, normalized_decision, evidence_ids)` — stable across rebuilds.

### Rebuilding

The log is populated incrementally on thread ingest. To re-extract over a window (e.g. after upgrading a model or after tuning the prompt):

```bash
# Dry-run-friendly: rebuild is idempotent on unchanged content.
mxr decisions rebuild --since 180

# Different window, JSON for scripting:
mxr decisions rebuild --since 365 --format json
```

What you get: a JSON envelope with `processed_thread_count`, `extracted_count`, and `kept_count`. Unchanged threads are skipped via source-hash comparison.

:::caution[The LLM is required for rebuild]
`mxr decisions` *list* works without the LLM — it reads the existing table. `mxr decisions rebuild` does not: extraction needs the model. Without it, rebuild exits with an explicit "LLM is disabled" error rather than silently producing nothing.
:::

### Composing decisions with threads

```bash
# For every pricing decision in the last quarter, open the thread it came from.
mxr decisions --topic pricing --since 90 --format json \
  | jq -r '.[] | .thread_id' \
  | xargs -I{} mxr thread {} --format json \
  | jq -r '.subject'
```

What you get: subjects of every thread that produced a pricing decision in the window. Use `--format ids` instead of the jq pipe if you only want thread ids.

## In real life

- **"What did we ship?"** at quarter end: `mxr decisions --since 90 --format json | jq -r '.[] | "\(.decided_at)\t\(.topic_tags | join(","))\t\(.decision)"' | sort` — chronological list of decisions tagged by topic, every one cite-backed.
- **Onboarding a new teammate:** `mxr ask "how do we run production deploys?" --mode hybrid --limit 20` — synthesized answer plus the exact messages they should read.
- **Pre-meeting prep:** `mxr ask "open items with Bob" --from bob@example.com --after 2026-01-01` — one-paragraph state of the relationship before walking into a 1:1.
- **Audit after a postmortem:** `mxr decisions rebuild --since 30 && mxr decisions --topic incident --format json` — re-extract after the postmortem thread closes, then list every decision the team made.

## Agent prompts that work

```text
"Use `mxr ask "<question>" --format json` to answer the question below.
Only quote the model's `.text` field. Append every entry from
`.citations[]` verbatim. If `.retrieval.executed_mode` differs from
"hybrid", flag that to me. Question: {{question}}"
```

```text
"For each topic in [hiring, pricing, infra], run `mxr decisions --topic
<t> --since 180 --format json` and produce a 5-bullet summary. Cite
the `evidence_msg_ids` for every bullet. Don't synthesize anything not
covered by an evidence id."
```

## See also

- [Search workflow](/guides/search/) — the retrieval primitives that back `mxr ask`
- [Semantic search](/guides/semantic-search/) — the index `--mode semantic` and `--mode hybrid` use
- [Briefings and loop-in](/guides/briefings-and-loop-in/) — `mxr briefing` reuses the same citation discipline for dormant threads
- [LLM features](/guides/llm-features/) — configure the model used for synthesis and extraction
- [CLI — `mxr ask`](/reference/cli/ask/), [`mxr decisions`](/reference/cli/decisions/)
