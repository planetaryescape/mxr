---
title: Forgotten work
description: Catch the commitments you make in sent mail and the replies you owe, ranked by your own cadence.
---

Two things make email feel like dropped balls: promises you typed into a draft and forgot about, and inbound threads that quietly aged past the speed at which you usually reply. mxr surfaces both, using only local data — extracted commitments from sent mail, and an "owed reply" lens that ranks threads against the recipient's typical cadence.

:::tip[The one-line mental model]
Commitments come **out** of your sent mail; the owed-reply lens looks at threads where you owe **in**. Both are deterministic ledgers — no LLM is required for the owed lens; the commitments extractor uses an LLM only to confirm a deterministic prefilter match.
:::

## Commitments — promises you made

When `mxr send DRAFT_ID --check` (or any real send) runs the [safety pipeline](/guides/pre-send-safety/), it scans the draft body for explicit outgoing commitments: "I'll send the deck Friday", "I'll get back to you next week", "I can review by EOD Monday". Each match becomes a **candidate** scoped to the draft. On successful send, candidates promote into the canonical `contact_commitments` ledger and show up in `mxr commitments`.

```bash
# Before sending: see the candidates the pipeline extracted.
mxr send DRAFT_ID --check --format json \
  | jq '.issues[] | select(.code == "commitment_candidate")'

# After a real send: candidate is now in the ledger.
mxr commitments --status open --format json
```

What you get: a JSON array of rows like `{ id, contact_email, direction, what, by_when, evidence_msg_id, status, created_at }`. `direction` is `yours` (you owe work) or `theirs` (they owe you). Every row has a message id you can `mxr cat` to read the original.

### List, filter, and resolve

```bash
# List everything still open across all contacts.
mxr commitments --status open --format json

# Just one person:
mxr commitments --contact alice@example.com --format json

# Mark done after you've shipped it.
mxr commitments resolve COMMITMENT_ID
```

```bash
# Daily standup: what did I commit to this week that's still open?
mxr commitments --status open --format json \
  | jq -r '.[] | select(.created_at > (now - 7*24*3600))
           | "\(.by_when // "unscheduled")\t\(.contact_email)\t\(.what)"' \
  | sort
```

What you get: one row per open commitment from the last 7 days, tab-separated, sorted by due date — pipe into `column -t` or paste straight into a standup doc.

### When the LLM is off

The extractor's prefilter is pure regex on first-person commitment markers (`I'll`, `I will`, `I can`, `I'll send`, …) and due-phrases (weekday, date, `tomorrow`, `next week`, `by EOD`). When the LLM is disabled or unreachable, deterministic prefilter matches still surface as low-confidence candidates in the safety report — they just don't get promoted into the ledger automatically. Run with `--check` to see them and decide.

```bash
mxr send DRAFT_ID --check --no-llm --format json \
  | jq '.issues[] | select(.code == "commitment_candidate")'
```

## Owed-reply lens — threads where you're the bottleneck

`mxr owed` ranks threads where the **latest** message is inbound and you haven't replied. It scores each thread by `waiting_days / expected_days`, where `expected_days` is the recipient's typical cadence from `reply_pairs` (falling back to a global p50, then to 7 days). The same set powers the [`is:owed-reply`](/guides/search/) search operator.

```bash
# Top 20 threads you owe, ranked overdue-first.
mxr owed --format json | jq -r '.[0:20]
  | sort_by(-.overdue_score)
  | .[]
  | "\(.overdue_score | tostring | .[0:4])\t\(.counterparty_email)\t\(.subject)"'
```

What you get: tab-separated rows `score \t sender \t subject`. Score 1.0 = exactly at typical cadence; 3.0 = three times longer than usual.

```bash
# Persistent sidebar lens (TUI and web).
mxr saved add owed 'is:owed-reply'

# Same set, scriptable.
mxr search 'is:owed-reply' --format ids
```

:::note[Two equivalent forms]
`mxr owed --format json` returns the structured row (with `overdue_score`, `waiting_days`, `cadence_days_p50`). `mxr search 'is:owed-reply'` returns the underlying message envelopes through the search stack. Use `owed` when you want the ranking metadata; use `search` when you want to compose with other operators (`is:owed-reply from:acme.com`).
:::

### Exclude noise

The lens already excludes list senders, screener-denied senders, trash, and spam — so newsletters never pollute it. To narrow further:

```bash
# Only threads waiting >= 14 days.
mxr owed --since 14 --format json

# Only threads whose latest inbound landed in the last 60 days
# (skip ancient unanswered relics).
mxr owed --within 60 --format json

# Combine: aged in, but not too old.
mxr owed --since 7 --within 60 --format json
```

## In real life

- **Friday standup prep:** `mxr commitments --status open --format json | jq '.[] | select(.direction == "yours") | {who: .contact_email, what, due: .by_when}'` — list every open promise across every contact, grouped by who's waiting.
- **Inbox-zero focus session:** `mxr owed --since 3 --format ids | head -10 | xargs -I{} mxr thread {} --format json` — pull the top 10 overdue threads, read them in batch, reply down the list.
- **Quarter-end review:** `mxr commitments --status open --format json | jq '[.[] | select(.direction == "theirs")] | length'` — count how many things people owe you that have been sitting open.
- **Pre-send sanity:** before queuing 8 replies in a row, check `mxr commitments --contact alice@example.com` so you don't promise the same deck twice.

## Agent prompts that work

```text
"For every open commitment in `mxr commitments --status open --format
json` where `direction == "yours"` and `by_when` is in the past, draft a
short status-update reply with `mxr draft-assist <evidence_msg_id>`.
Don't send — show me the drafts."
```

```text
"List the top 10 owed-reply threads from `mxr owed --since 5 --format
json`. For each, summarize the latest inbound with `mxr summarize
<thread_id>`. End with a single recommended verb per thread: REPLY,
ARCHIVE, DELEGATE."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/) — where the commitment candidates are extracted
- [Automated follow-ups](/guides/automated-followups/) — reminders and send-later for the *outbound* side
- [Search workflow](/guides/search/) — the `is:owed-reply` operator
- [LLM features](/guides/llm-features/) — configure the model used for commitment extraction
- [CLI — `mxr commitments`](/reference/cli/commitments/), [`mxr owed`](/reference/cli/owed/), [`mxr send`](/reference/cli/send/)
