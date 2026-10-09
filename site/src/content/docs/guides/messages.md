---
title: Messages
description: People you talk with, one row each, with your conversations inside as topics. Reply, say got it, or mark done here.
---

Messages shows people, not emails. Each row is a person you're in
conversation with, merged across their addresses, and your conversations
with them sit inside the row as topics. A group conversation, where two or
more other people took part, is its own row. The quoted line under a name
is what they asked you.

```bash
mxr messages            # the bands
mxr messages --turn mine
mxr messages person samir@launchpad.example
mxr messages ack THREAD_ID --dry-run
```

In the web app and the TUI, `g m` opens Messages.

The web app starts with the conversation list and no conversation open.
Choose a person or group to read it. A link to a specific conversation opens
that conversation directly. Opening Messages from navigation returns to the list.

## Four bands, closest people first

| Band | Who is in it | Order |
|---|---|---|
| Your turn | Their latest message to you is unanswered | Closest first, then whoever has waited longest past your usual pace with them |
| Pinned | Up to nine people you pinned with `s` | Most recent first, with a dot when it's your turn |
| Recent | Everyone else you've talked with in the last 30 days | Most recent first; "You: ..." when you wrote last |
| Quiet | Done here, or a turn nobody took | Collapsed; a new message brings them back |

Closeness comes from your own history with them, counted on your machine:
how often you've written to each other, how recently, and for how long.
Your turn uses the same rule as Now's People, so the two always agree.

A turn nobody took goes quiet after three times your usual interval with
that person, or a week when there's no history, and never stays longer
than 30 days. Nothing in Messages expires; it only goes quiet.

## Which threads land here

mxr judges each thread's shape from who took part:

- One other person wrote in it or you wrote to them: a topic in their row.
- Two or more other people took part: its own row, keyed by the thread,
  so it stays one row however the CC list changes.
- You were only copied and never wrote in it, or it went to more than 10
  people: it goes to Updates, not Messages, and Updates lists it with the
  reason "copied". A crowd thread stays in Messages while someone has
  replied to you in it, with you in To, and you haven't answered yet.

The 10-recipient line is `messages.large_thread_recipients` in
[config](/reference/config/). Automated mail and lists never land here.

## What a conversation shows

Opening a person shows how you know them ("You've written 48 times since
2023. Last: Tuesday."), every topic with them, group threads as "with
Ruth: Pricing copy", and the selected topic as a conversation.

Each message shows what it says that the thread didn't already: the
quoted history and signature are removed. mxr finds the quote by matching
the message against the earlier messages in the thread, as well as by
Gmail, Apple Mail and Outlook's quote markers, so it also catches quotes
no client marked. A message that lost anything says "trimmed: quote, sig"
(or "footer" for a disclaimer or unsubscribe footer), and `o` (or `v`)
shows it as sent. A reply that is nothing but quoted text reads "(only
quoted text)", so nobody's earlier words pass as the sender's. The CLI, TUI, web app and MCP all show
the same text.

Length decides the layout. A message of three lines or fewer reads like
chat, theirs on the left and yours on the right. A longer one is a
full-width letter with "Read all N paragraphs". Letters are never put in
bubbles.

The reply box sits at the bottom, already addressed ("Reply to Samir ·
Contract renewal"), with reply all on by default in a group.

## Got it

`.` replies with a short acknowledgement in your own words for that
person, such as "Hi Samir,\n\nGot it, thanks.\n\nCheers,\nAlex". The
greeting and sign-off are the ones you actually use with them, read from
your sent mail; no model writes it. It shows the exact text and counts
down for five seconds before sending, and `u` cancels it.

```bash
mxr messages ack THREAD_ID --dry-run   # the exact text, nothing sent
mxr messages ack THREAD_ID             # shows it, counts down, then sends
```

Got it is a real reply, so it reads fine in every mail client, unlike an
emoji reaction. Because it sends mail on one key, mxr guards it:

- It goes only to one person, never to all. It refuses mailing lists,
  automated and no-reply addresses, and a Reply-To on another domain you
  have never written to; reply by hand in those cases.
- What goes out is exactly what you saw. If someone writes again, or
  anything about the reply changes during the countdown, nothing is sent
  and you are asked to look again. A preview is good for one minute.
- It never sends twice for the same message, even when a send fails half
  way: if the provider might have taken it, mxr says so and won't try
  again, so check Sent. Once you've written after their message, there is
  nothing left to acknowledge.
- It works only on a conversation in Messages. A thread you were only
  copied on, or one sent to a crowd, is in Updates, and Got it refuses it.

## Keys

| Key | Does |
|---|---|
| `Enter` | Open the person |
| `r` / `a` | Reply / reply all |
| `.` | Got it |
| `e` | Done here, until they write again |
| `t` | Make a to-do from this topic |
| `b` | Reply later |
| `s` | Pin or unpin the person |
| `c` | New topic with this person |
| `[` / `]` | Previous / next topic |
| `p` | The person's page |
| `o` or `v` | The message as sent |
| `u` | Undo |
| `?` | What this mode is for, and its keys |

## One person, several addresses

People write from work and personal addresses. mxr never merges them on
its own, because a wrong merge shows one person's mail under another.
When two addresses you've written to share a name, it suggests the merge
on the person's page and in `mxr messages merge --suggestions`. You merge
by hand, with a preview first:

```bash
mxr messages merge samir@launchpad.example samir.p@home.example --dry-run
mxr messages merge samir@launchpad.example samir.p@home.example
mxr messages split samir.p@home.example
```

## For scripts and agents

`mxr messages --format json` returns every band with each row's topics,
preview, closeness and why line. `mxr messages person ID --format json`
returns the page with each message's new text and its `trimmed` flags.
The bridge serves the same data under `/api/v1/mail/people`, and the MCP
server has `mxr_messages`, `mxr_person` and `mxr_got_it` (which previews
unless confirmed with the previewed text).
