---
title: Clear the desk
description: Work through replies you owe, promises due, and threads waiting on others.
---

Open the desk, deal with each row, and put away the ones that need nothing
from you. The desk shows what needs you instead of what arrived, in four
lanes:

- **You owe**: someone you have written to wrote last.
- **Due**: a promise you made is coming due.
- **Waiting on**: you wrote last and they have not answered.
- **New from people**: a person you have not written to before.

Every row says why it is there and how long it has been. Newsletters,
receipts and notifications are not on the desk; they wait in
[Reading and Paper trail](/guides/reading-and-paper-trail/). The exact lane
rules are in the [desk reference](/reference/desk-and-places/#lanes).

## Open the desk

The web app opens on the desk (`mxr web`). In the TUI, **Desk** is the first
lens in the sidebar. From the command line:

```bash
mxr desk
```

```text
14 replies owed, waiting on 4 threads, 7 new messages from people.

You owe 14
  Jon Bell                Research notes: terminal workflows · wrote to you           22h · usually 47m
  Samir Patel             Contract renewal details · 2 messages since you last wrote  16h · usually 47m
  Samir Patel             Action required: unusual sign-in attempt · wrote to you     10h · usually 47m
  and 11 more (--limit to see them)

Waiting on 4
  Samir Patel             Launch checklist for Project Aurora · no reply to your la…  1d
  Ari Stone               Urgent password reset notice · no reply to your last mess…  1d
  Leo Park                Weekly local-first reading list · no reply to your last m…  19h
  and 1 more (--limit to see them)

New from people 7
  Cal Brooks              Contract renewal details · wrote to you                     4h
  Account Verification    Build failed on release branch · wrote to you               8h
  Cal Brooks              Spring upgrade offer for your workspace · wrote to you      11h
  and 4 more (--limit to see them)

Everything else: Reading 6 · Paper trail 3 · Deliveries 2 · Screener 27
```

That output is from `mxr desk --limit 3` against the demo mailbox. Each row
shows the counterparty, the subject, the reason, how long it has been, and
"usually" when mxr knows how fast replies go between you (the median of at
least two past replies). A row that is past its usual time is flagged
overdue; the web app colours its age.

The **Everything else** line counts what arrived this week in Reading and
Paper trail, active deliveries, open invites and new senders in the
screener. None of those are unread counts.

In the web app, the greeting counts the work ("Saturday morning. 3 replies,
2 promises.") and each count links to its lane. A lane shows five rows;
**Show all** opens the rest. `mxr desk --account NAME` limits the CLI to one
account.

## See what each row asks

With a language model configured, each row from a person says in one line
what the conversation is about, and on You owe and New from people the
reason becomes what they ask of you ("Asks: confirm who owns the rollout
check"). The inbox, label queues, the reply queue and owed replies show the
same line in place of the snippet, and so does the TUI.

Lines come from the cache at once. Missing ones are written in the
background for the rows on screen first and appear as each is ready; the
desk keeps a line's space under every row, so nothing moves. Pointing at a
line names the model that wrote it. Newsletters and automated mail never go
to the model, and with no model the rows look as they always did.

A local model writes a line every few seconds (about 3 s each with gemma4 on
Ollama), so a screen of rows fills in over a minute or so the first time,
then comes from the cache until someone writes again.
`llm.gist_concurrency` lets a hosted model write several at once.

```bash
mxr desk --gists                          # cached lines only, never waits
mxr briefing gists THREAD_ID --generate   # queue the missing ones
```

## Deal with a row

Open the conversation, then do what it asks:

- **Reply.** The reply field sits at the bottom of the conversation. To
  answer everyone you owe in one sitting, use
  [Focus & reply](/guides/focus-and-reply/).
- **Snooze.** Snoozing takes the conversation off the desk until it wakes.
  Type the time in words, such as `fri 3` or `in 2d`
  ([time phrases](/reference/time-phrases/)).
- **Keep a promise.** Due rows are promises from mail you sent: ones mxr
  found, and ones you asked to be reminded about. Do the thing, then use
  Done. See [promises on send](/guides/focus-and-reply/#keep-the-promises-you-make).

When you reply, the row leaves You owe on its own. When they reply, a
Waiting on row leaves on its own.

## Come back to it later

Reply later takes a time. Press `b` on a row, type when you want it back,
such as `tue 9`, `tomorrow` or `in 2d`, and check the exact time shown under
the field before you press Enter. The toast names that time. What happens
depends on who wrote last:

- **They did (You owe, New from people):** reply later. The conversation
  leaves the desk and your reply queue, and comes back to both at that time
  under You owe, with the reason "back from reply later". Your inbox is not
  touched.
- **You did (Waiting on):** bring it back if nobody replies. The conversation
  leaves Waiting on. If nobody has replied by then, it comes back with the
  reason "no reply by the time you set" and joins your reply queue so you
  can follow up. If they reply first, it lands in You owe as usual and the
  time is dropped.

It comes back on its own, once, even if mxr was not running at the time.
Press Enter with no time for the plain reply later: the reply queue, now.
On Waiting on a time is required. `u`, or the toast's **Undo**, puts back
whatever was set before, for about a minute.

From the command line, preview with `--dry-run`, then set it. `--at` takes
the same [time phrases](/reference/time-phrases/):

```bash
mxr desk later 7f5a428a-90b3-5f5f-a276-bfc82dd82874 309ae832-4d84-5d78-a3ef-76a7eda21496 --at "tue 9" --dry-run
```

```text
Would set 2 conversations for Tuesday 6 October, 09:00 (in 6 days).
  7f5a428a-90b3-5f5f-a276-bfc82dd82874  reply later: back in You owe and the reply queue then
  309ae832-4d84-5d78-a3ef-76a7eda21496  waiting: back then if nobody has replied
```

```bash
mxr desk later 309ae832-4d84-5d78-a3ef-76a7eda21496 --at "in 3d"
```

```text
Set 1 conversation for Saturday 3 October, 15:58 (in 3 days).
  309ae832-4d84-5d78-a3ef-76a7eda21496  waiting: back then if nobody has replied
Undo with: mxr undo 01a0f2d3-5a6c-7db2-8456-4781c4303b08
```

Snooze is different: it takes the conversation out of the inbox until it
wakes. Reply later leaves the inbox alone and only changes the desk and the
reply queue.

## Put a row away with Done

Use **Done** when a row needs nothing from you: a thank-you, a thread that
resolved itself, a promise you already kept. What Done does depends on the
lane:

| Lane | Done |
|---|---|
| You owe, New from people | Archives the conversation, marks it read, and keeps it off the desk. |
| Waiting on | Marks it read and stops waiting. Nothing is archived. |
| Due | Marks the promise kept and the conversation read. Other promises in the conversation stay. |

Every lane also takes the conversation out of your reply-later queue and
drops any time set to bring it back. It stays off the desk until someone
writes in it again, you or them.

- **Web app:** the check at the end of the row. On the desk the archive key
  is Done, in every lane and on a selection, and from the reader when you
  opened the conversation from the desk. With a mouse the check shows when
  you point at the row; on a touch screen it is always there, and a short
  swipe right is Done too.
- **TUI:** the archive key on a desk row, or in the conversation opened
  beside the desk.
- **CLI:** preview first, then put it away:

```bash
mxr desk done 080a03cf-08ab-5aca-a5e6-73c9f269800d --dry-run
```

```text
Would put away 1 conversation.
  080a03cf-08ab-5aca-a5e6-73c9f269800d  owed        would archive 5, mark 1 read, off the desk until someone writes
```

```bash
mxr desk done 080a03cf-08ab-5aca-a5e6-73c9f269800d
```

```text
Done: 1 conversation.
  080a03cf-08ab-5aca-a5e6-73c9f269800d  owed        archived 5, marked 1 read, off the desk until someone writes
Undo with: mxr undo 01a0e733-f0fb-7510-9fb5-0462837a28ae
```

The thread id is the row's `thread_id` in `mxr desk --format json`, or a
line of `mxr desk --format ids`. Without `--lane`, Done picks Waiting on when
you wrote last and You owe otherwise. For a Due row, pass the promise:

```bash
mxr desk done THREAD_ID --promise COMMITMENT_ID --dry-run
```

```text
Would put away 1 conversation.
  93d1de1e-0556-5bcb-b5d9-9effb317b6b6  due         promise kept, off the desk until someone writes
```

Keys for every surface are in the
[keybindings reference](/reference/keybindings/). Holding a key down in the
web app does not put away row after row. In the TUI a held key repeats, so
tap once per row.

## Undo

Undo puts everything back as it was: the messages return to the inbox, each
read or unread as before, the row returns to the desk and to reply-later, and
a promise is open again. Use the toast's **Undo** in the web app or TUI, or
the id Done printed:

```bash
mxr undo 01a0e733-f0fb-7510-9fb5-0462837a28ae
```

```text
Undone
```

The undo window is about a minute.

If a conversation cannot be put away (its account is offline, say), its row
stays, the rest are done, and `mxr desk done` exits non-zero. The first
Done's undo id still restores everything it changed. Run Done again to retry;
the retry gets its own undo id.

## Stop waiting without marking read

`mxr desk dismiss` takes a Waiting on row off the desk until a new message
arrives, and changes nothing else:

```bash
mxr desk dismiss 309ae832-4d84-5d78-a3ef-76a7eda21496 --dry-run
```

```text
Would stop waiting on 1 conversation.
  309ae832-4d84-5d78-a3ef-76a7eda21496
```

`mxr desk restore THREAD_ID` puts it back in Waiting on.

## Script the desk

`--format json` returns every lane with its rows and totals. Row fields are
listed in the [desk reference](/reference/desk-and-places/#output).

```bash
# Subjects of the replies you owe, most overdue first.
mxr desk --format json | jq -r '.owed.rows[] | .subject'

# Read the most overdue one.
mxr cat "$(mxr desk --format json | jq -r '.owed.rows[0].message_id')"
```

## Make the inbox your home instead

In the web app, set **Settings > Appearance > Home** to **Inbox**. The
arrival-order inbox shows everything, Reading and Paper trail mail included,
and the desk stays one step away. The TUI opens on the inbox.

## When a conversation is not where you expect

- **Not on the desk at all:** run `mxr why MESSAGE_ID`. A newsletter or
  notification goes to Reading or Paper trail, and a screened-out sender
  never shows. To count a sender as a person, move them to People
  ([move a sender](/guides/reading-and-paper-trail/#move-a-sender-for-good)).
- **Gone after you archived it:** You owe and New from people show only
  while the conversation is in the inbox. Undo, or move it back and wait for
  the next message.
- **Came back after Done:** someone wrote in the conversation again. Done
  holds only until the next message, on purpose: new mail is new
  information.
- **Gone after `b`:** you set a time. It comes back then; until then it is
  still in the inbox. `u` right away, or `b` again with a new time, changes
  it.
- **In You owe instead of New from people:** you have written to them
  before, or replied in that conversation.
- **In New from people after you moved them to People:** moving a sender
  to People makes their mail count as from a person. It lands in You owe
  once you have written to them.

Next: [answer everyone you owe in one sitting](/guides/focus-and-reply/).
