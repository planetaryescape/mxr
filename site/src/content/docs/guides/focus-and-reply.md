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

In the web app, open **Focus & reply** from the command palette (`⌘K`), or
use **Reply to all 14 in focus mode** in the header of the desk's You owe lane.
The key for each is in the
[keybindings reference](/reference/keybindings/#focus--reply).

- From anywhere else, the queue is the You owe lane, furthest past your
  usual reply time first, then your reply-later queue, one entry per
  conversation.
- From the desk, the queue is the You owe lane alone.
- The queue follows the account picker. **All accounts** covers every
  enabled account.
- The order is fixed when you start, so nothing reshuffles while you work.
  Mail that arrives meanwhile joins the end.

Newsletters, notifications, screened-out senders and snoozed conversations
never appear. Leaving focus mode takes you back to where you were.

## Work through the queue

For each conversation, do one of these. Each is a button under the reply and
has a key:

| Action | What happens |
|---|---|
| **Send** | Sends the reply and moves to the next conversation. |
| **Skip for now** | Moves it to the end of the queue. |
| **Done, no reply needed** | Puts it away, as [Done on the desk](/guides/desk/#put-a-row-away-with-done) does, and moves on. |
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
  everyone.") and the next promise you made that is due, for example "Next
  thing due: Mon, send the notes to nora@example.com."
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
  shows under **Due** on the [desk](/guides/desk/) and in the conversation's
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

In the TUI, the promise appears as a prompt after the send: press `y` to be
reminded, `n` or `Esc` to let it go. It disappears after 30 seconds.

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
If nobody has written in the conversation by then, it comes back to your
reply-later queue. A reply cancels the reminder, whether it answers your
message or an earlier one in the same conversation, including one sent in the
same second. For the command line (`mxr send --remind-after`,
`mxr remind`), see [Automated follow-ups](/guides/automated-followups/).

## In the TUI

Open the reply queue (`Ctrl-p`, then **Reply Queue**) and press `F`. The TUI
run differs from the web app's:

- It works through the reply-later queue only, not the You owe lane.
- It starts at the selected row and wraps round. The status bar reads
  "Focus 2 of 5: replying to ...".
- Sending one reply opens the next. Discarding a reply pauses the run.
- There is no skip, Done or snooze inside the run.

## From the command line

There is no separate focus command. The queue is what these list:

```bash
mxr desk --format json | jq -r '.owed.rows[] | "\(.message_id) \(.subject)"'
mxr replies --format json
mxr reply MESSAGE_ID --body "On it." --yes
```

Next: [clear Reading and Paper trail](/guides/reading-and-paper-trail/).
