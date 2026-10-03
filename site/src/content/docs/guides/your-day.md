---
title: Work through your day
description: Clear what needs you, answer who you owe, then sweep what's left.
---

Start on the desk, answer everyone you owe, keep your promises, and sweep the
mail that isn't from people. This page is the order; each step links to the
page with the detail. It works the same in the web app, the TUI and the CLI,
because all three read the same daemon.

:::note
mxr is moving to [five email modes](/guides/email-modes/). Now is the front
page, and Messages, the mode for people, is the desk's lanes for now
(`g m`). The steps below use the desk; they change as each mode ships.
:::

## 1. Open the desk

```bash
mxr web      # the web app opens on Now; g m opens the desk's lanes
mxr desk     # the same lanes in the terminal
```

The desk shows what needs you, not what arrived: **You owe**, **Due**,
**Waiting on** and **New from people**. Every row says why it is there and how
long it has been. Newsletters and notifications are not on it.

In the TUI, **Messages** in the sidebar opens the desk lens.
[Start from Now](/guides/now/#messages-built-on-the-desk) covers every lane.

## 2. Put away what needs nothing

A thank-you, a thread that resolved itself, a reply you no longer need:
use **Done** on the row. It archives and marks read, and the row stays away
until someone writes again. Preview it from the command line first:

```bash
mxr desk done THREAD_ID --dry-run
mxr desk done THREAD_ID
```

Undo brings it back exactly as it was
([Done](/guides/now/#put-a-row-away-with-done)).

## 3. Answer everyone you owe

Open **Focus & reply** from the command palette, or from the header of the
You owe lane. It shows one conversation at a time with your reply beside it.
Send, and the next one comes up; skip, snooze or Done the ones that can wait.
By default every send waits ten seconds, so you can undo it.

When a reply promises something with a date ("I'll send the deck by
Friday"), mxr offers to remind you then. Say yes, and the promise shows under
**Due** on the desk from a week before it is due until you mark it done.
[Reply to everyone you owe](/guides/focus-and-reply/) covers the queue,
undo send and promises.

## 4. Check what you are waiting on

**Waiting on** lists conversations where you wrote last and nobody has
answered. Nudge the ones that matter. Use Done on the ones you no longer
need an answer to. To stop watching one until a time, press `b` and type it
(`in 3d`): it leaves Waiting on and comes back then if nobody has replied
([come back to it later](/guides/now/#come-back-to-it-later)). Sending with
**Send and remind me if no reply in** does the same from the start
([automated follow-ups](/guides/automated-followups/)).

## 5. Clear Reading and Paper trail

Read what you want in **Reading**, pin what you want to keep in **Paper
trail**, then sweep the rest:

```bash
mxr sweep paper-trail --dry-run
mxr sweep paper-trail --yes
```

Move a sender who is in the wrong place once, and their mail follows from
then on. [Clear Reading and Paper trail](/guides/reading-and-paper-trail/)
covers the rules, pins and sweeps.

## 6. Low tide

When the desk is empty, the web app shows low tide: **Low tide. Nobody's
waiting on you.** with the next promise that is due. The TUI says the same in
its status bar. See [Sound, key hints and touch](/guides/sound-hints-and-touch/)
for the optional sounds that go with it.

## Times in words

Anywhere mxr asks for a time (snooze, send later, reminders, promises) you
can type it in words and see exactly when it resolves before you commit:

```bash
mxr time fri 3 --now 2026-09-28T09:00:00+01:00
```

```text
Friday 2 October, 15:00 (in 4 days)
  Assumed: am or pm
  Or: Friday 2 October, 03:00 (in 4 days)
```

The [time phrases reference](/reference/time-phrases/) lists every phrase.
