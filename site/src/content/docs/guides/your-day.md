---
title: Work through your day
description: Start on Now, answer people, act on tasks, and clear the digest.
---

Open Now to choose what needs you, then work in the mode that holds it.
Use the same mailbox from the web app, terminal or CLI.

## 1. Start on Now

```bash
mxr now
mxr web
```

In the web app or TUI, press `g h` for Now. Open a person or task to continue
in its mode. Now caps each section instead of showing the whole inbox.
On the web, expand the arrivals line only when you want the sorting details.
[Start from Now](/guides/now/) explains the sections and counts.

## 2. Answer people in Messages

```bash
mxr messages list --turn mine
```

Press `g m` and choose a person or group. The web list opens without selecting
a conversation. Read the visible request; use `Read all N paragraphs` when
you want the rest of a longer letter. Reply, or mark the topic done here if
it needs no answer. Preview from the CLI before marking it done:

```bash
mxr modes done THREAD_ID --mode messages --dry-run
mxr modes done THREAD_ID --mode messages
```

Other modes can still hold work from the same conversation.
[Messages](/guides/messages/) covers replies, acknowledgements and retained
quoted context. [Focus & reply](/guides/focus-and-reply/) works through a reply
queue one conversation at a time.

## 3. Act on To dos

```bash
mxr todo
```

Press `g x`. Read the action title, amount and dates, then use the row's
primary action or open its source email. In the web app, tick the checkbox
on the left or press `Space` on the selected row when you have finished.
Schedule work for later without changing its due date, or correct what mxr
read. [Act on To dos](/guides/todo/) shows those steps.

A `Looks resolved` offer needs a separate review: inspect its quoted email
and proposed change before accepting. It never completes or retimes a task
on its own. Follow [Review later evidence](/guides/todo-resolution/).

## 4. Read Updates and let go

```bash
mxr updates
mxr updates let-go --dry-run
mxr updates let-go
```

Press `g u` to read the digest. Check `Needs a look`, then `Changed`; routine
mail is grouped behind them. Press `t` on an update to create a to-do when
you want to act on it. A warning in Updates is a suggestion, not proof that
a task was created.

For a package, read the latest state mxr found in its email and open that
email for context. Use the carrier's tracking service when you need live
status. Pin an email you need to retain before sweeping its sender.
[Updates](/guides/updates/) explains pinning, sweeping and digest expiry.

## 5. Read what you chose

```bash
mxr reading edition --peek
```

Press `g r`. Read an issue, save it to Later, or let go when you are done.
`--peek` lists the edition without recording a visit. Fetch a linked article
only when you want to open it; that request contacts its website.
[Reading](/guides/reading/) covers Later and unsubscribe choices.

## 6. Find a record or group related mail

```bash
mxr records ask "lisbon booking ref"
mxr categories --format json
```

Press `g e` to search Archive's filed records and inspect a field's source.
[Archive](/guides/archive/) also shows subscriptions and annual renewal tasks.
Use [personal categories](/guides/personal-categories/) for your own groups,
such as Travel, across the modes. Preview category assignments and rules
before applying them.

## See also

- [Compare the modes](/guides/email-modes/)
- [Keyboard reference](/reference/keybindings/)
- [Back up and restore your workspace](/guides/backup-recovery/)
