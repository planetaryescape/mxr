---
title: Forgotten work
description: Track promises, review requests and resolve work without losing its source.
---

List open commitments, inspect their source messages, and resolve the work you have finished:

```bash
mxr commitments --status open --format json
mxr cat MESSAGE_ID
```

Stored asks track promises and requests in received and sent mail without a model. The owed-reply lens separately ranks unanswered directed requests against your usual reply cadence. Both use synced local mail.

:::tip[The one-line mental model]
Sent-mail asks use stored deterministic rules for owner promises and requests. Reply-only asks feed Your turn. Optional draft safety proposals are a separate feature; they do not decide automatic To do rows.
:::

<span id="commitments-promises-you-made"></span>

## Track promises and requests

After sync or an accepted send, stored asks project confirmed promises and requests into the commitment ledger. An owner promise is `yours`; a correspondent promise or an outgoing request is `theirs`. Waiting on uses these stored answers. Reply-only asks feed Your turn, while actionable instructions belong in [To do](/guides/todo/).

Draft safety hints are separate. Preview them before sending:

```bash
# Inspect informational commitment hints in the safety report.
mxr send DRAFT_ID --check --format json \
  | jq '.issues[] | select(.code == "commitment_candidate")'

# After sending: inspect the stored commitments for this account.
mxr commitments --status open --format json
```

What you get: a JSON array with `id`, `account_id`, `email`, `thread_id`, `direction`, `status`, `who_owes`, `what`, `date_unverified`, `evidence_msg_id` and `extracted_at`. A dated row also has `by_when`; an undated row omits it. Use `evidence_msg_id` with `mxr cat` to read the source. `extracted_at` is an RFC3339 timestamp.

Automatic deadlines come from unambiguous words in the message, interpreted relative to its date. A date you explicitly record also has authority. Older model-supplied dates can remain visible as unverified; they do not drive task timing. See [date provenance](#dates-mxr-shows-but-does-not-act-on).

Optional model-backed draft extraction can also persist proposals, which promote after a successful send. A safety hint alone does not create a commitment.

### List, filter, and resolve

```bash
# List open rows across contacts in the selected/default account.
mxr commitments --status open --format json

# Select another account explicitly:
mxr commitments --account personal --status open --format json

# Just one person:
mxr commitments --contact alice@example.com --format json

# Mark done after you've shipped it.
mxr commitments resolve COMMITMENT_ID
```

```bash
# Open rows recorded in the last seven days, sorted by due date.
mxr commitments --status open --format json \
  | jq -r '
    def epoch: sub("\\.[0-9]+Z$"; "Z") | fromdateiso8601;
    .[] | select((.extracted_at | epoch) > (now - 7*24*3600))
    | [(.by_when // "unscheduled"), .email, .what] | @tsv' \
  | sort
```

What you get: tab-separated due date, contact email and promise text. The filter uses the recorded `extracted_at`, not the deadline. Automatic asks use the source message date for this field; draft proposals use their extraction time. Pipe into `column -t` or paste into a standup doc.

### Dates mxr shows but does not act on

mxr acts on a commitment's date only when it knows where the date came from: words in the email ("by Friday"), or a date you typed. A date a model wrote on its own is kept on the row and shown as "date unverified", and nothing runs on it. The desk's Due lane, your to-dos and Now all ignore it, so a wrong date never makes a promise look overdue.

See which dates are unverified:

```bash
mxr commitments --status open --format json \
  | jq -r '.[] | select(.date_unverified) | "\(.id)\t\(.by_when)\t\(.what)"'
```

Text output marks the same rows, for example `due 2026-10-30 (date unverified)`. Thread context and the relationship profile show the same "(date unverified)" label.

Every commitment that already had a date when you upgraded to v0.6.141 counts as unverified, including some that were dated from the email's words in v0.6.120 to v0.6.140, because mxr did not record the source before then. Your own recorded promises keep their dates. New commitments are dated from the email's words, or left undated.

To make an unverified date count, record the promise again with the date you want. Preview it first, then keep it; the date is now yours:

```bash
mxr commitments add MESSAGE_ID --what "send the deck" --due "fri 9am" --dry-run
mxr commitments add MESSAGE_ID --what "send the deck" --due "fri 9am"
mxr commitments resolve COMMITMENT_ID
```

`resolve` closes a commitment that is done.

A phrase with more than one reading is saved without a date. That covers "fri 3", a weekday named on that same weekday, and a phrase mxr could only read by dropping some of its words.

### When the LLM is off

Automatic stored asks and explicit `mxr commitments add` work with the LLM off. The safety report can still show informational regex hints for possible promises. Optional model-backed draft extraction produces no stored proposals when the model is disabled; those hints are not automatic ledger entries. Inspect them with:

```bash
mxr send DRAFT_ID --check --no-llm --format json \
  | jq '.issues[] | select(.code == "commitment_candidate")'
```

## Owed-reply lens: threads where you're the bottleneck

`mxr owed` lists the [desk's](/guides/now/) **You owe** lane in full, for one account: unanswered directed requests from people you have written to, plus reply-later reminders you explicitly set. Informational mail does not create reply debt. New senders' directed requests appear in Messages' New from people instead. The exact rules are in the [desk reference](/reference/desk-and-places/#lanes). `mxr owed --all` retains the raw unanswered-inbound scope, including automated and archived mail.

Rows come most overdue first. `overdue_score` is `waiting_days / expected_days`, where `expected_days` is how long you usually take to reply to that person (`usual_seconds`, the median of your past replies, with at least two of them), or one day when there is no history.

```bash
# Top 20 replies you owe, most overdue first.
mxr owed --format json | jq -r '.[0:20][]
  | "\(.overdue_score | tostring | .[0:4])\t\(.from_email)\t\(.subject)"'
```

What you get: tab-separated rows `score \t sender \t subject`. Score 1.0 means exactly your usual reply time; 3.0 means three times longer.

:::note[Rows and the raw list]
`mxr owed --format json` returns one row per conversation (`thread_id`, `latest_inbound_msg_id`, `from_email`, `from_name`, `subject`, `latest_inbound_at`, `waiting_days`, `expected_days`, `overdue_score`, and `usual_seconds` when known).

`mxr owed --all` is the raw list the desk filters: every conversation whose latest inbound message has no later reply from you, archived or not, automated senders included (list senders and screener-denied senders are left out). It ranks by the sender's contact cadence (`cadence_days_p50`), falling back to the average of every contact's cadence, then 7 days, never below half a day. The [`is:owed-reply`](/guides/search/) search operator matches this raw set, so you can combine it with other operators (`is:owed-reply from:dana@acme.com`).
:::

### Narrow by age

The windows apply to the latest message from them:

```bash
# Only replies waiting >= 14 days.
mxr owed --since 14 --format json

# Only replies whose latest message landed in the last 10 days.
mxr owed --within 10 --format json

# The raw list, skipping ancient unanswered relics.
mxr owed --all --since 7 --within 60 --format json
```

The default list only looks at conversations active in the last 30 days, like the desk, apart from ones you set to reply later. Use `--all` to reach older threads.

## In real life

- **Friday standup prep:** `mxr commitments --status open --format json | jq '.[] | select(.direction == "yours") | {who: .email, what, due: .by_when}'` lists your open promises and the contact waiting for each, in the selected account.
- **Inbox-zero focus session:** `mxr owed --since 3 --format ids | head -10 | xargs -I{} mxr thread {} --format json` pulls the top 10 overdue threads so you can read them in batch and reply down the list.
- **Quarter-end review:** `mxr commitments --status open --format json | jq '[.[] | select(.direction == "theirs")] | length'` counts how many things people owe you that have been sitting open.
- **Pre-send sanity:** before queuing 8 replies in a row, check `mxr commitments --contact alice@example.com` so you don't promise the same deck twice.

## Agent prompts that work

```text
"For every open commitment in `mxr commitments --status open --format
json` where `direction == "yours"` and `by_when` is in the past, draft a
short status-update reply with `mxr draft-assist <evidence_msg_id>`.
Don't send; show me the drafts."
```

```text
"List the top 10 owed-reply threads from `mxr owed --since 5 --format
json`. For each, summarize the latest inbound with `mxr summarize
<thread_id>`. End with a single recommended verb per thread: REPLY,
ARCHIVE, DELEGATE."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/): where the commitment candidates are extracted
- [Automated follow-ups](/guides/automated-followups/): reminders and send-later for the *outbound* side
- [Search workflow](/guides/search/): the `is:owed-reply` operator
- [To do](/guides/todo/): inspect and correct tasks from stored asks
- [LLM features](/guides/llm-features/): configure optional model-backed draft proposals
- [Review later evidence](/guides/todo-resolution/): completion checks after new mail arrives
- [CLI: `mxr commitments`](/reference/cli/commitments/), [`mxr owed`](/reference/cli/owed/), [`mxr send`](/reference/cli/send/)
