---
title: The desk
description: What needs you, not what arrived. Replies you owe, promises coming due, threads waiting on someone and new mail from people, each with one Done, in the CLI, the TUI and the web app.
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

Everything that is not work is summarised in one line of links:
[**Reading**](/guides/reading-and-paper-trail/) (newsletters and lists that
arrived this week, read or not), [**Paper trail**](/guides/reading-and-paper-trail/)
(receipts and notifications that arrived this week), **Deliveries**,
**Invites** and **Screener** (new senders with no decision yet). None of
those counts are badges; the only badges count work.

The desk is built from what mxr already knows locally: who wrote last in each
conversation, your contacts, screener decisions, past reply times, open
commitments and cadence watches. It makes no model calls, so it is as fast
as any other local read.

## What counts as a person

A message counts as mail from a person unless the sender looks like one of
these:

- a list or newsletter (a `List-Id` header, an unsubscribe method, a
  `newsletter@` style address, or a sender marked as a list), or a sender
  you moved to Reading;
- a machine: a `no-reply`, `notifications` or `alerts` style address or
  sending domain, a delivery update, a calendar invite, or a sender you
  moved to Paper trail;
- a sender you screened out, which never shows on the desk.

A sender you mark as a person (`K` in Reading or Paper trail, then `p`)
counts as one whatever their headers say. The full rules, with the reason
each placement shows, are in
[Reading and Paper trail](/guides/reading-and-paper-trail/).

A conversation shows on the desk only while it is in the inbox. Archiving or
snoozing it takes it off; undo puts it back. A promise under **Due** stays
until you mark it done, whatever happens to its conversation. When a
conversation qualifies for two lanes, it shows once, in the first of: You
owe, Due, Waiting on, New from people.

## Done: put it away

Every row has one way to say "there is nothing for me to do here": **Done**.
It is the check at the end of the row, `e` in the web app and the TUI, and
`mxr desk done` on the command line. The row leaves at once, and what Done
does depends on the lane:

| Lane | Done |
|---|---|
| **You owe**, **New from people** | Archives the conversation, marks it read, and keeps it off the desk. |
| **Waiting on** | Marks it read and stops waiting. Nothing is archived: a conversation you started has nothing in the inbox to archive. |
| **Due** | Marks the promise kept and the conversation read, and keeps it off the desk. Nothing is archived: the promise was the work. Other open promises in the same conversation stay under Due. |

"Off the desk" holds for every lane, a watched contact's row included, until
someone writes in the conversation again: them, or you. Moving it back to the
inbox by hand does not bring it back; a new message does. A kept promise does
not return. Done also takes the conversation out of your reply-later queue.

Undo (the toast's **Undo**, `u`, or `mxr undo ID`) puts it back exactly as
it was: the messages return to the inbox, each one read or unread as it was
before, the conversation is back on the desk and in reply-later if it was
there, and a promise is open again.

If a conversation cannot be put away (its account is offline, say), its row
stays and the rest are still done. Press Done again to retry it; its undo
still returns each message to how it was before the first try.

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
# Done: preview, then put it away. The lane defaults to Waiting on when
# you wrote last, otherwise You owe.
mxr desk done THREAD_ID --dry-run
mxr desk done THREAD_ID
# A promise under Due: pass its commitment id.
mxr desk done THREAD_ID --lane due --promise COMMITMENT_ID
# Changed your mind: undo with the id Done printed.
mxr undo MUTATION_ID
```

```text
Done: 1 conversation.
  0192f0c4-...  owed        archived 2, marked 1 read, off the desk until someone writes
Undo with: mxr undo 0192f0c5-...
```

`--format json` returns `mutation_id` and one outcome per conversation:
`lane`, `archived` and `marked_read` counts, `dismissed`,
`resolved_commitment_id`, `reply_later_cleared`, and `error` for one that
could not be put away (the command then exits non-zero). It takes the `thread_id`, `lane` and
`commitment_id` that `mxr desk --format json` prints for each row.

`mxr desk dismiss THREAD_ID` is the older "done waiting" on its own, without
marking anything read, and `mxr desk restore THREAD_ID` undoes it.

## In the web app

The web app opens on the desk. The greeting counts the work ("Saturday
morning. 3 replies, 2 promises.") and each count links to that lane in full.
Lanes show their first five rows; **Show all** opens the rest.

The keys are the ones every list uses: `j`/`k` move one cursor across all
the lanes, `Enter` opens the conversation and `Esc` comes back to the same
row, `Z` snoozes, `#` trashes and `u` undoes. On the desk, `e` is **Done**,
in every lane: archive and Done mean the same thing here, so the key you
already use to put mail away does it. `w`, the list's row key, and `m` do
the same. Done works on a selection, and from the reader when you opened
the conversation from the desk. Holding the key down does not put away row
after row.

With a mouse, the check shows when you point at a row or move the cursor to
it. On a touch screen it is always there.

`g d` goes to the desk and `g i` to the inbox in arrival order; `g w` opens
**Waiting on**. To open on the inbox instead, set **Settings, Appearance,
Home** to Inbox.

## In the TUI

**Desk** is the first entry in the sidebar's lens list, and `g h` opens it
(`g d` stays Drafts in the TUI). `j`/`k` move across the lanes and `Enter`
opens the conversation beside the desk. `e` (or `m`) is **Done** for the
row under the cursor, from the list or from that conversation opened beside
it, and `u` undoes it. Holding a key down in the TUI repeats its action, so
tap `e` once per row.
