---
title: Raycast
description: Now and the five modes as Raycast commands, with each mode's actions, reply and Open in mxr. A client of the local daemon.
---

The mxr Raycast extension puts Now and the five modes behind Raycast commands:
Now, Messages, To Do, Updates, Reading and Archive. Each command lists its
mode the way the web app does, in the same sections, and acts with that mode's
own verbs. Every row also opens in the web app.

It is a client of the daemon, like the web app and the TUI. It reads and acts
through the daemon's local HTTP bridge; nothing is stored in Raycast except
its last copy of each list.

## What you need

- macOS, Raycast, Node.js 22 or later and npm.
- The mxr daemon running, with its HTTP bridge on (the default). Check with
  `mxr status`.

The extension is private and not in the Raycast Store. Load it from a
checkout:

```bash
cd apps/raycast
npm ci
npm run dev
```

`npm run dev` imports the commands into Raycast and rebuilds as the code
changes. They stay in Raycast after you stop it.

## What each command shows

| Command | Sections | Actions |
|---|---|---|
| Now | People, Due Soon, Updates, Reading, Coming Up | Reply, Done Here, Tick Off, Open in mxr |
| Messages | Your Turn, Pinned, Recent, Quiet | Reply, Reply All (groups), Done Here, Open in mxr |
| To Do | Now, each coming week, Whenever, Later | Tick Off, Open in mxr, Open Source Email in mxr |
| Updates | Needs a Look, Changed, Routine | Done Here, Let Go of This Digest, Open Link, Open in mxr |
| Reading | The edition's bands, then Later | Read in mxr, Open Original, Save for Later, Let Go |
| Archive | Records | Open in mxr, Copy Reference |

A list opens with its last copy at once and refreshes underneath. The daemon
keeps ready-made views of what you read recently, so the refresh is usually
instant too; after a few idle minutes the first read builds again, which takes
about a second on a large mailbox. Reads never count as a Reading visit or
mark Updates seen.

## Every change is previewed

Done Here, Tick Off, Let Go and Save for Later each ask the daemon for a dry
run first and show what it says will change, for example "Would change 1
to-do (ticked off). Nothing changed yet." Nothing happens until you confirm.
Changes the daemon can undo get an Undo button (⌘Z) on the toast that
follows. Anything the daemon could not change is listed, in the dialog and
on the toast, instead of being reported as done.

## Reply

Reply opens a form with the recipients and subject filled in and an empty
body, as the web composer does. Draft in My Voice (⌘D) adds a suggested
reply written in your style above what you have typed; nothing is sent until
you send it.

Send Reply runs the same pre-send safety check the daemon enforces on every
send:

- A clean reply is handed to the daemon to send ten seconds later, and the
  toast offers Undo Send. The daemon owns that wait, so closing Raycast does
  not lose the reply. Its scheduled-send loop runs every minute, so the reply
  goes out within about a minute. Undo stops it and keeps it in mxr as a
  draft; once the daemon has started sending, Undo says it is too late.
- Warnings are listed before the reply is scheduled.
- A reply the check would block is only sent if you choose Send Anyway. That
  send goes immediately, because a scheduled send is checked again when it
  fires and cannot carry the override, so there is no undo for it.

If you edit the reply while it is being checked, nothing is sent; send again
to check what is on screen. Leaving the form without sending throws the draft
away. Open in mxr (⌘O) on the form saves what you have written as an mxr draft
and opens it in the web composer.

## Open in mxr

Open in mxr opens the web app the daemon serves on its bridge port, at the
same place: a conversation, a person, a to-do, a Reading item or a mode. The
page signs itself in from the daemon on this machine, so the link carries no
token.

## Settings

The extension reads the bridge port and token from the files the daemon
writes in `~/Library/Application Support/mxr`. If your daemon uses another
data directory, set it in the extension's preferences. If the token file is
missing or out of date, the extension asks the daemon for it the way the web
app does.
