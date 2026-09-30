---
title: Briefings and loop-in
description: Re-enter dormant threads, suggest who to Cc, find local experts, and look people up.
---

Five related "context recovery" surfaces. Each one starts from local mail and
the existing profile/index tables that apply to that question, then produces a
small answer instead of a dashboard. Surfaces that synthesize from message
evidence carry citations; deterministic profile-only fallbacks may return an
empty citation list.

| Surface | Question it answers |
|---|---|
| `mxr briefing context` | What matters before I read this thread? |
| `mxr briefing thread` | What was this dormant thread about when it went quiet? |
| `mxr briefing recipient` | What's the state of my relationship with this person right now? |
| `mxr suggest-recipients` | Who normally gets Cc'd on this kind of thread? |
| `mxr expert` | Who's answered something like this question before? |
| `mxr whois` | Who/what is this name? |

:::tip[The one-line mental model]
These features **suggest**, never **mutate**. None of them auto-Cc, auto-summarize a thread into your draft, or invent details the corpus doesn't support.
:::

## Thread context: `mxr briefing context`

The context the web and TUI readers show above a conversation, on the command
line. Facts come from the local store with no model: who the thread is mainly
with, how many emails you have exchanged, your and their usual reply times,
when you last spoke outside this thread, whether you owe a reply, and open
promises in the thread both ways. When a model is configured it adds a
one-sentence gist and the ask, with the sentence that makes the ask quoted.

```bash
mxr briefing context THREAD_ID
mxr briefing context THREAD_ID --no-ai          # facts only, no model
mxr briefing context THREAD_ID --format json    # {context, gist}
```

```text
Gist      The canary stays at 5% until the dashboard is quiet.
Asks you  to confirm who owns the rollout check
          "Can you confirm who owns the rollout check before Monday?"
With      Maya (maya@example.com): 41 emails · you usually reply within 4h · last spoke 12 Sep
You owe   a reply since Thu
Promises  You promised: send the runbook link, due 2 Oct
AI        local model qwen2.5:7b · from this thread only
```

Rules the gist follows:

- **The quote is checked.** mxr keeps the ask's quote only when it appears
  in the thread's text (line breaks and quote-mark style may differ, the
  words may not), and drops quotes under 8 characters. A paraphrase is dropped and the ask stays without it, so
  clients can highlight the quote without trusting the model.
- **Plain text within limits.** The gist is one sentence of at most 240
  characters and the ask at most 160, with no markdown.
- **Cached per newest message.** The answer is kept until a new message
  arrives, the model changes, or the privacy setting changes what the prompt
  may carry. `--refresh` asks again.
- **Privacy.** The thread's own messages go to the model under the same rule
  as `mxr summarize`. Your history with the person (counts and reply times)
  only goes to a cloud model with `llm.allow_cloud_relationship_data = true`.
  The `AI` line says which model answered, local or cloud, and whether it
  read that history.
- **No model is not an error.** Without one, `gist.status` is `disabled` and
  the human output shows the facts only.

In JSON, `context` has `counterparty`, `owed_reply` and `promises` (each
`{owner, commitment}`), and `gist` has `status` (`ready`, `disabled`,
`blocked` or `failed`), `gist`, `ask` (`summary` and an optional verified
`quote` of `{message_id, text}`), `provenance` (`model`, `locality`,
`sources`), `reason`, `generated_at` and `from_cache`. The bridge serves
the same data at `GET /api/v1/mail/threads/{id}/context` and
`GET /api/v1/mail/threads/{id}/context/gist`, and MCP clients get it from
`mxr_thread_context`.

### Many conversations at once: `mxr briefing gists`

The one line the desk and inbox rows show, for several conversations.
Answers from the cache without calling a model; `--generate` queues the
missing ones from people (in the order given) for a background writer, and
`mxr events --type gist` prints each as it lands.

```bash
mxr briefing gists THREAD_ID THREAD_ID --format json
mxr briefing gists THREAD_ID --generate
```

JSON has `model` (`available`, `disabled` or `blocked`), `gists` (each with
`newest_message_id`, the message it answers for), `queued`, `in_flight`, and
`skipped` with a reason: `not_people`, `not_found`, `recently_failed`,
`not_generated` or `queue_full`. A gist that goes stale while the model is
writing (someone replied) is never announced; it is written again. The same
data is at `POST /api/v1/mail/gists` (`{thread_ids, generate}`, at most 100)
and in the `mxr_thread_gists` MCP tool.

## Thread briefing: `mxr briefing thread`

When a thread has been dormant for a month or more, opening it cold is
expensive. `mxr briefing thread` synthesizes a Markdown recap from the local
thread transcript. It does not require a cached thread summary first.

```bash
mxr briefing thread THREAD_ID
mxr briefing thread THREAD_ID --format json
```

What you get: `{ thread_id, body_markdown, citations, generated_at, from_cache }`.
Each citation points at a message in the thread. If the model cites an unknown
message id, mxr ignores that citation instead of passing through unsupported
evidence.

### Cached, hash-invalidated

The briefing is cached in `context_briefings` keyed by thread id and a content
hash of the thread's message ids and dates. New mail invalidates the cache; a
duplicate request returns instantly.

```bash
# Force a regenerate (e.g. after changing the briefing model):
mxr briefing thread THREAD_ID --refresh
```

### When the LLM is off

With `[llm] enabled = false`, `mxr briefing thread` returns a deterministic
thread snapshot: message count, participant count, and latest message. No
invented summary.

```bash
mxr briefing thread THREAD_ID --format json \
  | jq '{thread_id, body_markdown, citations}'
```

## Recipient briefing: `mxr briefing recipient`

Same primitives, different subject. Useful before writing to someone after a long gap.

```bash
mxr briefing recipient alice@example.com
mxr briefing recipient alice@example.com --format json
```

What you get: `{ thread_id, body_markdown, citations, generated_at, from_cache }`,
where `thread_id` is the recipient email. The deterministic baseline includes
message counts and last inbound/outbound dates when mxr has them; the LLM can
turn that baseline into prose when enabled.

The TUI compose flow shows a quiet `Last contact 14mo ago. Press B for context.` hint when you're composing to someone past the gap threshold (default 180 days). The hint is never auto-inserted into the draft body; it is only shown above it.

### Privacy

`mxr briefing recipient` consults the relationship profile, which can contain
personal context. When `llm.allow_cloud_relationship_data = false`, relationship
synthesis is blocked for non-local LLM endpoints and a deterministic fallback is
returned instead. Local LLMs see the full profile.

## Maybe include: `mxr suggest-recipients`

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

# Then decide manually whether to add; mxr never edits To/Cc for you.
mxr drafts edit DRAFT_ID
```

## Who's the expert: `mxr expert`

For an inbound question you'd otherwise forward, `mxr expert` ranks people in your local corpus who have answered similar questions before, ranked by their *answers*, not their *questions*.

```bash
# Start from a specific message:
mxr expert MESSAGE_ID --format json

# Start from a free-text query:
mxr expert --query "Who knows about DKIM setup?" --format json
```

What you get: `{ email, display_name, score, reason, answered_threads, citations }` rows. Every citation points at an *answer* message: a reply that follows a question, contains explanatory content, and was followed by thanks/confirmation or no further unresolved ask in the same thread.

### Ranking shape

| Signal | Weight |
|---|---|
| Their message follows a question on the topic | Required |
| Their message contains explanatory content (length, structure) | Strong |
| Thread later has "thanks"/"that worked" or no further ask | Strong |
| They are a current thread participant | Excluded by default (`--include-self` to keep) |

### LLM behavior

Ranking and `reason` text are deterministic today. `mxr expert` does not call the LLM, so disabling `[llm]` does not change the returned rows.

```bash
# Compose with --include-self when you're the expert and want to confirm:
mxr expert MESSAGE_ID --include-self --format json
```

## Personal knowledge graph: `mxr whois`

Lightweight, citation-required explanations for people and free-text terms. Query-time only: there is no persisted entity table in v1, by design.

```bash
mxr whois sam
mxr whois alice@example.com --format json
mxr whois "Project Apollo" --limit 20 --format json
```

What you get: `{ canonical_name, kind, summary, first_seen_at, last_seen_at, topics, citations, candidates }`. `kind` is `person`, `term`, `ambiguous`, or `unknown`. Email queries return `person`; project and jargon names use the free-text `term` / `ambiguous` path until a persisted entity table exists.

When evidence is weak, `summary` says so and `candidates` lists alternatives instead of inventing a definition.

### Email vs. free-text

When the query is an email, the path uses the sender/relationship profile directly. Otherwise the path is lexical search over messages plus a citation pass that filters mentions down to confident ones. The same `--limit` controls the citation budget either way.

```bash
# Disambiguate an ambiguous name:
mxr whois "Sam" --format json | jq '.candidates'
```

What you get: a list of candidate entities with first/last-seen timestamps. Pick the one you mean and re-query with the email or a more specific phrase.

## In real life

- **Reopening a 3-month-old thread:** `mxr briefing thread THREAD_ID --format json` first; read the `.body_markdown` recap and inspect `.citations`.
- **Composing to a contact after a year:** `mxr briefing recipient alice@example.com` before opening the draft; read the deterministic relationship baseline or LLM prose in `.body_markdown`.
- **Forwarding an inbound question:** `mxr expert MESSAGE_ID --format json | jq '.[0]'`; forward to the top answerer with a one-line context.
- **Reading a meeting note that mentions "Sam":** `mxr whois Sam --format json | jq '.candidates // .canonical_name'`; disambiguate without leaving the terminal.
- **Pre-send Cc check:** `mxr suggest-recipients --draft DRAFT_ID --format json` before hitting send on a topic you don't normally own; your normal collaborators may be missing.

## Agent prompts that work

```text
"Before I reply on dormant thread THREAD_ID, run `mxr briefing thread
THREAD_ID --format json`. Quote `.body_markdown`, then list each
`.citations[].quote` with its message id. End with a one-line recommended
next step."
```

```text
"For inbound MESSAGE_ID, run `mxr expert MESSAGE_ID --format json` and
propose the top expert with a one-sentence forward note grounded in
their cited answer message. Don't forward; show me first."
```

```text
"Before I send DRAFT_ID, run `mxr suggest-recipients --draft DRAFT_ID
--format json`. List suggestions with `confidence >= medium` and a
one-line reason. Ask me which to add. Never edit the draft yourself."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/): the gate the suggest-recipients hint appears inside
- [Archive intelligence](/guides/archive-intelligence/): same citation discipline for `mxr ask` / `mxr decisions`
- [LLM features](/guides/llm-features/): configure the model used for briefings
- [Sender view](/guides/sender-view/): the per-contact view that links into recipient briefings
- [CLI: `mxr briefing`](/reference/cli/briefing/), [`mxr suggest-recipients`](/reference/cli/suggest-recipients/), [`mxr expert`](/reference/cli/expert/), [`mxr whois`](/reference/cli/whois/)
