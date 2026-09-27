---
title: The desk
description: What needs you, not what arrived. Replies you owe, promises coming due, threads waiting on someone and new mail from people, in the CLI, the TUI and the web app.
---

An inbox sorted by arrival answers "what came in?". The desk answers "what
needs me?". It has four lanes, and every row says why it is there and how
long it has been:

| Lane | What is in it | Its clock |
|---|---|---|
| **You owe** | Someone you are in conversation with wrote last and you have not replied. | Since their message, next to how fast you usually reply to them. |
| **Due** | Promises you made in sent mail that are due within a week, or overdue. | The due day. |
| **Waiting on** | You wrote last, at least 12 hours ago, and they have not answered. Watched contacts who have gone quiet longer than usual join this lane too. | Since your message, next to how fast they usually reply. |
| **New from people** | Recent mail (the last 7 days) from a person you have not written to before. | When it arrived. |

Everything that is not work is summarised in one line of counts: **Reading**
(unread newsletters and lists), **Paper trail** (receipts and notifications),
**Deliveries**, **Invites** and **Screener** (new senders with no decision
yet). None of those counts are badges; the only badges count work.

The desk is built from what mxr already knows locally: who wrote last in each
conversation, your contacts, screener decisions, past reply times, open
commitments and cadence watches. It makes no model calls, so it is as fast
as any other local read.

## What counts as a person

A message counts as mail from a person unless the sender looks like one of
these:

- a list or newsletter (a `List-Id` header, an unsubscribe method, or a
  sender marked as a list), or a sender you screened into the feed;
- a machine: a `noreply`, `notifications` or `alerts` style address or
  sending domain, a delivery update, a calendar invite, or a sender you
  screened into the paper trail;
- a sender you denied in the screener, which never shows on the desk.

A conversation shows on the desk only while it is in the inbox. Archiving or
snoozing it takes it off; undo puts it back. A conversation you started has
nothing in the inbox to archive, so on a **Waiting on** row archive means
**done waiting**: the thread leaves the lane until a new message arrives in
it, from them or from you. A promise under **Due** stays
until you mark it done, whatever happens to its conversation. When a
conversation qualifies for two lanes, it shows once, in the first of: You
owe, Due, Waiting on, New from people.

"Usually" is the median of your past replies with that person (or theirs to
you), and needs at least two of them. A row turns a soft yellow only when it
has been longer than that.

## In the CLI

```bash
mxr desk
```

```text
3 replies owed, waiting on 2 threads, 4 new messages from people.

You owe 3
  Maya Ortiz              Launch checklist · replied to your message        2d · usually 4h
  Theo Nash               Pricing copy · wrote to you                       1d
  ...

Everything else: Reading 7 · Paper trail 14 · Deliveries 1 · Screener 2
```

`--format json` returns the whole desk: each lane's `rows` and `total`, the
`elsewhere` counts, and `generated_at`. `--format jsonl` prints one row per
line with its `lane`, and `--format ids` prints thread ids. Every row has the
`thread_id` and `message_id` to open, the counterparty, a `reason`,
`age_seconds`, and `usual_seconds` when the pace is known.

```bash
# Open the most overdue reply you owe.
mxr desk --format json | jq -r '.owed.rows[0].message_id' | xargs mxr cat
```

`--account` limits the desk to one account (the default covers them all), and
`--limit` sets how many rows each lane returns.

```bash
# Done waiting on a thread; preview first when you pass several.
mxr desk dismiss THREAD_ID --dry-run
mxr desk dismiss THREAD_ID
# Changed your mind: it waits again.
mxr desk restore THREAD_ID
```

## In the web app

The web app opens on the desk. The greeting counts the work ("Saturday
morning. 3 replies, 2 promises.") and each count links to that lane in full.
Lanes show their first five rows; **Show all** opens the rest.

The keys are the ones every list uses: `j`/`k` move one cursor across all
the lanes, `Enter` opens the conversation and `Esc` comes back to the same
row, `e` archives, `Z` snoozes, `#` trashes and `u` undoes. On a row under
**Waiting on**, `e` (or `w`) is done waiting. On a promise under **Due**, `w`
marks it done.

`g d` goes to the desk and `g i` to the inbox in arrival order; `g w` opens
**Waiting on**. To open on the inbox instead, set **Settings, Appearance,
Home** to Inbox.

## In the TUI

**Desk** is the first entry in the sidebar's lens list, and `g h` opens it
(`g d` stays Drafts in the TUI). `j`/`k` move across the lanes and `Enter`
opens the conversation beside the desk, and `e` on a row under Waiting on is
done waiting.
