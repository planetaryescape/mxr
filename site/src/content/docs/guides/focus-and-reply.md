---
title: Focus & reply and promises on send
description: Work through everyone you owe a reply in one sitting, keep the promises you make in mail you send, and undo a send while its countdown runs.
---

## Focus & reply

Focus & reply puts everyone you owe a reply in one queue and shows them one
conversation at a time. The conversation and what it asks of you sit on the
left, your reply on the right. Send it and the next one comes up.

The queue is the desk's **You owe** lane: people in your inbox you have
written to before and who wrote last, furthest past your usual reply time
first. Newsletters, notifications, screened-out senders and snoozed
conversations never appear. Your reply-later queue follows, one entry per
conversation. It follows the account picker; **All accounts** covers every
enabled account. The order is fixed when you start, so nothing reshuffles
while you work; mail that arrives meanwhile joins the end. If the lane is
ever longer than one fetch, the end says how many more are waiting instead
of "That's everyone".

### Opening it

- `g F` from anywhere in the web app. From the desk it works through the
  **You owe** lane alone (no reply-later), and the lane's header offers the
  same: "Reply to all 14 in focus mode".
- The **Focus & reply** entry in the command palette (`⌘K`).
- In the TUI, open the reply queue (`Ctrl-p`, then **Reply Queue**) and press
  `F`.

`Esc` goes back to where you were.

### Working through the queue

| Key | What it does |
|-----|--------------|
| `⌘Enter` / `Ctrl+Enter` | Send the reply and move to the next conversation |
| `s` | Skip it for now: it moves to the end of the queue |
| `Z` | Snooze it, with a time in words ("tomorrow 9am", "fri 3") |
| `w` | Send, and remind you if nobody replies by a time you type |
| `d` | Draft the reply in your voice (needs a language model) |
| `r` | Back into the reply |
| `Esc` | Leave focus mode |

In the reply, `⌘Enter` sends. To use the single keys, step out of the reply
first: press `Tab`, or `Esc` in the rich-text editor (in the Markdown editor
with vim keys, `Esc` belongs to vim). The same actions are buttons under the
reply.

The header shows where you are ("2 of 5") and a thin bar fills as you go. A
reply counts as done once its send countdown starts. Nothing is drafted for
you unless you ask with `d` or **Draft for me**.

When the queue is empty, focus mode says so and names the next thing you
promised that is due, for example "Next thing due: Mon, send the notes to
nora@example.com." Skipping the last conversation sets it aside instead:
the end says how many you skipped, with **Come back to it**. If who you owe
couldn't be loaded, you get the error and **Try again**, never "Nobody is
waiting".

### From the command line

There is no separate focus command: the queue is what these already list.

```bash
mxr desk --format json          # the You owe lane, with the other lanes
mxr replies --format json       # your reply-later queue
mxr reply MESSAGE_ID --body "..." --yes
```

## Undo send

Every send from the web app waits a few seconds before it goes (set the
length under **Settings → Compose**). The toast counts the seconds down with a
thin bar that drains over the window. **Undo**, or `z` outside a text field,
stops the send and keeps the draft. In focus mode, undo also brings the
conversation back to the front of the queue with your reply as you left it.
With reduced motion on, the bar is hidden and the seconds still count.

## Promises on send

When you send something like "I'll send the deck by Friday", mxr notices the
promise and offers to remind you, with the date already worked out:

> You promised: send the deck (by Friday). Remind me Friday 2 October, 09:00?

The send never waits for this. The check runs during the send's countdown,
and the offer stays until you answer. **Remind me** keeps the promise as an
open commitment due at that time: it shows in the conversation's context and
in what's due. **Change time** takes the time in words. **Not now** keeps
nothing. If you undo the send, the offer goes with it.

How it works:

- A language model reads the message you are sending and points at each
  thing you promised and the words that say when. mxr keeps a promise only
  if both are in your message, and shows them in your own words. It reads
  the date with the same time parser every time field uses, in your time
  zone. The date you see is the parser's, not the model's.
- Only mail filed as sent (in your Sent folder, or sent by mxr) and from one
  of your addresses holds your promises; a From header alone doesn't.
- Mail without "I'll", "I will" or "I can" is never sent to the model.
- The model sees only the message you are sending and its recipients, under
  the privacy settings for commitments. Each offer says whether a local or
  cloud model found it.
- No model, a blocked one, or one that takes more than eight seconds: no
  offer, and the send is unaffected.

In the TUI, after a send the promise appears as a prompt: `y` to be
reminded, `n` or `Esc` to let it go. It disappears on its own after 30
seconds.

From the command line, `mxr send`, and `compose`, `reply` and `forward` with
`--yes`, print each promise after the send with the command that keeps it:

```text
You promised: send the deck, due Friday 2 October, 09:00 ("by Friday").
  Remind me: mxr commitments add MESSAGE_ID --what 'send the deck' --due 2026-10-02T08:00:00+00:00
```

The note goes to stdout only for table output on a terminal. Piped, or with
`--format json`, it goes to stderr so stdout stays exactly what scripts read.
You can also check a sent message and keep a promise yourself:

```bash
mxr commitments detect MESSAGE_ID --format json
mxr commitments add MESSAGE_ID --what "send the deck" --due "fri 9am" --dry-run
mxr commitments add MESSAGE_ID --what "send the deck" --due "fri 9am"
```

Adding the same promise again moves its date. `mxr commitments resolve ID`
marks it done.

## Remind me if nobody replies

`w` in focus mode, or **More send options → Send and remind me if no reply
in** in any reply, sends and sets a reminder for a time you type. If nobody
has written in the conversation by then, it comes back to your reply-later
queue. A reply cancels the reminder, whether it answers your message or an
earlier one in the same conversation, including one in the same second as
your send. See
[Automated follow-ups](/guides/automated-followups/) for the command line.
