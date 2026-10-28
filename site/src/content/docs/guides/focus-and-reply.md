---
title: Reply to everyone you owe
description: Answer your owed replies in one sitting and keep the promises you send.
---

Work through everyone waiting on a reply from you, one conversation at a
time, and keep track of what you promise while you do it. Focus & reply puts
the desk's **You owe** lane and your reply-later queue in one queue. The
conversation and what it asks of you sit on the left, your reply on the
right. Send it and the next one comes up.

## Start a focus session

Press `g F` in the web app or the TUI. In the web app you can also open
**Focus & reply** from the command palette (`⌘K`), or use **Reply to all 14
in focus mode** in the header of the desk's You owe lane. The keys inside it
are in the [keybindings reference](/reference/keybindings/#focus--reply).

- From anywhere else, the queue is the You owe lane, furthest past your
  usual reply time first, then your reply-later queue, one entry per
  conversation.
- From the desk, the queue is the You owe lane alone.
- The queue follows the account picker. **All accounts** covers every
  enabled account.
- The order is fixed when you start, so nothing reshuffles while you work.
  Mail that arrives meanwhile joins the end.

Newsletters, notifications, screened-out senders and snoozed conversations
never appear. `Esc` or `q` leaves focus mode and takes you back to where you
were.

## Work through the queue

For each conversation, do one of these. Each is a button under the reply,
and all but **Send, remind me if nobody replies** have a key (it is in the
palette):

| Action | What happens |
|---|---|
| **Send** | Sends the reply and moves to the next conversation. |
| **Skip for now** | Moves it to the end of the queue. |
| **Done, no reply needed** | Puts it away, as [Done on the desk](/guides/now/#put-a-row-away-with-done) does, and moves on. |
| **Snooze** | Takes a time in words, such as `tomorrow 9am` or `fri 3`. |
| **Send, remind me if nobody replies** | Sends, and brings the conversation back if nobody writes by a time you type. |
| **Draft for me** | Drafts the reply in your voice. Needs a [language model](/guides/llm-features/). |

In the reply, `⌘Enter` (`Ctrl+Enter` off macOS) sends. To use the single-key
actions, step out of the reply first: press `Tab`, or `Esc` in the rich-text
editor. In the Markdown editor with vim keys, `Esc` belongs to vim. Nothing
is drafted for you unless you ask.

The header shows where you are ("2 of 5") and a bar fills as you go. A reply
counts once its send countdown starts.

About **Done, no reply needed**:

- It marks the conversation read, takes it out of reply-later, and keeps it
  off the desk until someone writes in it again.
- It archives a conversation from the You owe lane. A reply-later entry
  where you wrote last is treated as Waiting on: marked read and set aside,
  not archived.
- Undo from its toast brings the conversation back to the front of the
  queue.

## Finish

- **Everyone answered:** focus mode shows low tide ("Low tide. That's
  everyone you write with.") and the next promise you made that is due, for
  example "Next thing due: Mon, send the notes to nora@example.com." A promise
  whose date a model wrote is not offered here; see [dates mxr does not act
  on](/guides/forgotten-work/#dates-mxr-shows-but-does-not-act-on). People
  who wrote to you and you have not written to are in Messages, not in the
  queue, and the end says how many: "3 others who wrote to you are in
  Messages."
- **You skipped the last ones:** the end says how many you skipped, with
  **Come back to it**.
- **More than one batch:** the queue loads up to 5,000 rows. Past that, the
  end reads "That's this batch." with **Continue with N more**.
- **Who you owe could not be loaded:** you get the error and **Try again**,
  never an empty queue.

Closing the tab while a reply is still in its undo window asks you first.

## Undo a send

Every send from the web app waits before it goes: 10 seconds by default.
Change it under **Settings > Compose > Undo send window** to 5, 10 or 30
seconds, or **Send immediately**, which sends at once with no undo.

The toast counts the seconds down. **Undo** stops the send and keeps the
draft. In focus mode, undo also brings the conversation back to the front of
the queue with your reply as you left it. With reduced motion on, the bar is
hidden and the seconds still count.

## Keep the promises you make

When you send something like "I'll send the deck by Friday", mxr notices the
promise and offers to remind you, with the date worked out:

> You promised: send the deck (by Friday). Remind me Friday 2 October, 09:00?

The send never waits for this. The check runs during the send's countdown,
and the offer stays until you answer:

- **Remind me** keeps the promise as an open commitment due at that time. It
  shows under **Due** on the [desk](/guides/now/) and in the conversation's
  context. The toast that confirms it has its own Undo.
- **Change time** takes a time in words.
- **Not now** keeps nothing.

If you undo the send, the offer goes with it. At most five promises are
offered per message.

How mxr finds a promise:

- Mail without "I'll", "I will" or "I can" is never sent to the model.
- A language model reads only the message you are sending and its
  recipients, under the privacy settings for commitments. Each offer says
  whether a local or cloud model found it.
- mxr keeps a promise only when what you promised appears in your own
  words. It offers a reminder only when the words that say when also appear
  and parse. The date comes from the same
  [time parser](/reference/time-phrases/) every time field uses, in your time
  zone, not from the model.
- Only mail filed as sent (in your Sent folder, or sent by mxr) and from one
  of your addresses holds your promises. A From header alone does not.
- With no model, a blocked one, or one that takes more than eight seconds,
  there is no offer and the send is unaffected.

In the TUI, the promise appears as a yes/no prompt after the send ("Remind
me ...?"). Answer yes to be reminded, or no or `Esc` to let it go. It
disappears after 30 seconds.

### From the command line

`mxr send`, and `compose`, `reply` and `forward` with `--yes`, print each
promise after the send with the command that keeps it:

```text
You promised: send the deck, due Friday 2 October, 09:00 ("by Friday").
  Remind me: mxr commitments add MESSAGE_ID --what 'send the deck' --due 2026-10-02T08:00:00+00:00
```

The note goes to stdout only for table output on a terminal. Piped, or with
`--format json`, it goes to stderr, so stdout stays what scripts read. A
promise without a date is printed with `--due "<when>"` for you to fill in.
The CLI waits at most 10 seconds for the check.

To check a sent message and keep a promise yourself, preview first:

```bash
mxr commitments detect 7294ff6c-2134-5909-8ea2-e2a1f0694ca9
mxr commitments add 7294ff6c-2134-5909-8ea2-e2a1f0694ca9 --what "send the deck" --due "fri 9am" --dry-run
```

```text
Would remind you on Friday 2 October, 09:00: send the deck (to theo@northstar.example). Nothing was stored.
```

```bash
mxr commitments add 7294ff6c-2134-5909-8ea2-e2a1f0694ca9 --what "send the deck" --due "fri 9am"
```

```text
Reminder set for Friday 2 October, 09:00: send the deck (to theo@northstar.example). Commitment 7294ff6c-2134-5909-8ea2-e2a1f0694ca9::promise::01a0e735-2f6b-7dd2-b454-c1b273a5df21
```

Adding the same promise again moves its date, and reopens it if it was
resolved. `mxr commitments resolve ID`, or Done on its Due row, marks it
kept.

## Remind me if nobody replies

Use **Send, remind me if nobody replies** in focus mode, or **More send
options > Send and remind me if no reply in** in any reply, and type a time.
Until then the conversation stays off the desk's Waiting on. If nobody has
written in it by then, it comes back to Waiting on and your reply-later
queue. A reply from a person cancels the reminder, whether it
answers your message or an earlier one in the same conversation; an
auto-responder does not. For the command line (`mxr send --remind-after`,
`mxr remind`), see [Automated follow-ups](/guides/automated-followups/).

## In the TUI

`g F` opens the same queue, with the same keys and the same requests to the
daemon. `F` in the reply queue (`g q`) opens it on the selected message.
Opened from the desk, it is the You owe lane alone, as in the web app. The
TUI has no account picker, so the queue covers every account.

The conversation shows why it is here and its newest message from them. The
reply is written in `$EDITOR`, so a few things work differently:

- `r` opens the reply. When you save and quit, the send review opens, and
  **Send** there sends and brings up the next conversation. Another send, a
  failed send or a discarded reply leaves the queue where it is. A terminal
  can't deliver `Ctrl+Enter`, so the review's Send is the TUI's Send and
  next.
- **Send, remind me if nobody replies** is in the palette (`:`). It opens the
  reply, and the review then asks when to remind you. In any send review,
  `n` does the same.
- `d` opens the reply. When you save and quit, you pick a writing model and
  mxr drafts the reply in your voice, as **Refine with the assistant** in
  the review does.
- `j`, `e` and `b` work as in the web app. After `e`, `u` or `z` puts the
  conversation back at the front of the queue.
- At the end, the TUI says what you handled, how many more are waiting and
  who is in Messages. `j` brings back the ones you skipped, and `g F` starts
  a new session, which also retries a queue that failed to load. There is no
  next-promise line.

## From the command line

There is no separate focus command. The queue is what these list:

```bash
mxr desk --format json | jq -r '.owed.rows[] | "\(.message_id) \(.subject)"'
mxr replies --format json
mxr reply MESSAGE_ID --body "On it." --yes
```

Next: [clear Reading and Paper trail](/guides/reading-and-paper-trail/).
