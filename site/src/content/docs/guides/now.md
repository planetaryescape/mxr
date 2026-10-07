---
title: Start from Now
description: The few things that need you now, from every mode, and Messages, built on the desk.
---

Now is the front page: the few things that need you now, from every mode.
It shows at most ten things, in four sections that never change order:

- **People**: whose turn it is with you, from Messages. Never the people you
  are waiting on.
- **Due soon**: To do's Now band, overdue first, then by act-by date.
- **Updates**: one card for the latest digest (cut at 08:00 and 16:30):
  its headline, up to three lines that need a look or changed, and how much
  routine waits behind them.
- **For tonight**: from 17:00, one issue to read, from the newsletter you read
  most.

Each section shows at most three items, then "and 11 more in Messages". A
long list of people becomes one line: "14 people waiting on you: 7 you've
written to, 7 new. Start with the three below." The headline, that line and
"and 11 more in Messages" count the same people, so the numbers add up. A section with nothing in it
disappears, and when they all have, Now says "Clear." and when the next to-do
surfaces. The caps live in the daemon, so the web app, the TUI and
`mxr now` show the same ten things.

Archive's records are not built yet, and say "early version"
where they show. [Email modes](/guides/email-modes/) has
the status of each mode.

## Open Now

The web app and the TUI open on Now. Press `g h` to come back to it from
anywhere. From the command line:

```bash
mxr now --format table
```

```text
Saturday morning. 14 people waiting on you, 1 thing to act on.

PEOPLE  (and 11 more in Messages)
  14 people waiting on you: 7 you've written to, 7 new. Start with the three below.
  Samir Patel             Contract renewal details
    From Messages: your turn with Samir Patel, 16h.
  Leo Park                Research notes: terminal workflows
    From Messages: your turn with Leo Park, 25h.
  Ari Stone               Flight options for the Portland demo day
    From Messages: your turn with Ari Stone, 18h.

DUE SOON
  Fix payment for Spotify  £11.99  act now

UPDATES  08:00 digest  (+4 more, 5 routine)
  3 need a look, 4 changed. 5 routine from 2 sources.
  Google                Security alert: New sign-in from Chrome on Windows  (already in To do)
  GitHub acme/api       Run failed: CI - main (9f8e7d6)
  Stripe                Payout of R 4,210.00 failed: bank declined  (already in To do)

Not now: Reading 5 this week
```

That is the demo mailbox on a Saturday morning. Every row says which mode it
came from and why. `mxr now` prints JSON by default, with the same sections,
caps and lines.

## Act on a row in its own mode

| Key | What it does |
|---|---|
| `j` / `k` | Next and previous row |
| `Enter` | Open the row in its own mode: a person in Messages, a to-do in To do, the card in Updates, the pick in Reading |
| `e` | Done here, in the row's mode |
| `r` | Reply to a person |
| `o` | Open the email |
| `A` | Let go of the Updates card, after a preview |
| `u` | Undo |
| `?` | What Now is for, then its keys |

Done here takes the conversation out of that one mode until a new message
arrives. Other modes keep it, and the email is archived in your mail
provider only when no mode holds it any more. The toast says which happened
every time:

```text
Done in Messages. Still in To do (due Mon).
Done. Archived in Gmail.
```

`u` or the toast's **Undo** puts it back. To keep mail in the inbox even when
the last mode lets go, set `modes.archive_on_last_done = false`. The desk's
own Done at `/desk` follows the same rule: it never archives an email To do
still holds.

In To do, `e` ticks off that one to-do. Another to-do on the same email stays
open, and the email stays in the inbox until the last one is ticked off. In
Updates, `e` lets go of one source in the digest.

```bash
mxr modes done THREAD_ID --mode todo --todo TODO_ID --dry-run
mxr modes done --mode updates --sender notifications@github.com --account ACCOUNT_ID --dry-run
```

`A` on the Updates card previews what letting go of the digest does ("Let go
of 11 updates from 4 sources; 1 also in To do stays there.") and lets go of
exactly that set when you confirm. See
[Updates](/guides/updates/#let-go-of-the-digest-in-one-key).

## See which other modes hold an email

One email can be in several modes: the landlord who asks "are you around
Thursday?" and sends a lease to sign by Monday is in Messages and To do. The
reader and the rows in Messages say so, in that mode's words:

```text
Also in To do: Sign the document from Sam Okafor, act by Mon 5 Oct  g x
```

The `g` key opens that mode. From the command line:

```bash
mxr modes why MESSAGE_ID          # or --thread THREAD_ID
```

## Answer a new sender's one question

When a person writes to you for the first time, their row asks once where
their mail belongs: "New sender. Keep in Messages?" with Messages, Updates,
Reading and Block as the answers. Answering moves the sender for their future
mail too, and undo puts it back. Automated senders are never asked: their
mail goes to Updates or Reading, and the row's why line names the rule. The
[Screener](/guides/triage-flow/) page, under More, lists every sender you
have decided on.

## The rail

The sidebar lists Now, the five modes and Inbox, with their keys:

| Entry | Key | Opens |
|---|---|---|
| Now | `g h` | The front page; its badge counts people whose turn it is and things to act on |
| Messages | `g m` | People, built on the desk's lanes (early version) |
| To do | `g x` | Things email asked you to do, by when to act |
| Updates | `g u` | Notifications as a briefing by source, twice a day. `g p` opens it too |
| Reading | `g r` | Newsletters as an [edition](/guides/reading/), with a Later shelf. No count: nothing in it is owed |
| Archive | `g e` | Records, which are coming; search finds receipts until then |
| Inbox | `g i` | Everything, newest first |

Screener, Reply queue, Waiting on, Snoozed and Subscriptions are under
**More**. Subscriptions no longer has a `g` key; `g u` opens Updates.
`mxr modes rail` prints the same list with counts.

On a phone the web app shows five tabs instead: Now, Messages, To do,
Reading and Find. Find holds search, Archive and Inbox. Updates opens from
the card on Now.

## The desk's lanes, and Messages

Messages is now its own mode: people as rows with their topics inside. See
[Messages](/guides/messages/). The desk still exists (`mxr desk`, `/desk`)
and shares Messages' rule for whose turn it is:

- **You owe**: someone you have written to wrote last.
- **Due**: a promise you made is coming due.
- **Waiting on**: you wrote last and they have not answered.
- **New from people**: a person you have not written to before.

A thread you were only copied on, or one sent to more than 10 people, that
you never wrote in is nobody's turn: it is in Updates. A turn nobody took
goes quiet after three times your usual interval with that person, at least
a week. The exact lane rules are in the
[reference](/reference/desk-and-places/#lanes).

Person mail still in your inbox that no lane holds stays in Messages as
quiet, so it never vanishes from every mode, and ticking off a to-do on it
never archives it.

## Open the desk

In the web app, `/desk` shows the desk's lanes. From the command line:

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
arrival-order inbox shows everything, Updates and Reading mail included,
and Now stays one key away (`g h`).

## When a conversation is not where you expect

- **Not on the desk at all:** run `mxr why MESSAGE_ID`. A newsletter or
  notification goes to Reading or Updates, and a screened-out sender
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
