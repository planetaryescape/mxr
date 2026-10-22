---
title: Raycast
description: Now, the five modes and the inbox as Raycast commands, with each mode's actions, reading, search, reply and Open in mxr. A client of the local daemon.
---

The mxr Raycast extension puts Now, the five modes and the inbox behind
Raycast commands: Now, Inbox, Messages, To Do, Updates, Reading and Archive.
Each mode command lists its mode the way the web app does, in the same
sections, and acts with that mode's own verbs. Every row reads in Raycast and
also opens in the web app.

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
| Now | People, Due Soon, Updates, Reading, Coming Up | Read, Reply, Done Here, Tick Off, Open in mxr |
| Inbox | Every inbox conversation, newest first | Read, Reply, Open in mxr |
| Messages | Your Turn, Pinned, Recent, Quiet | Read, Reply, Reply All (groups), Done Here, Open in mxr |
| To Do | Now, each coming week, Whenever, Later | Tick Off, Read Source Email, Open in mxr, Open Source Email in mxr |
| Updates | Needs a Look, Changed, Routine | Read, Done Here, Let Go of This Digest, Open Link, Open in mxr |
| Reading | The edition's bands, then Later | Read, Read in mxr, Open Original, Save for Later, Let Go |
| Archive | Records | Read Source Email, Open in mxr, Copy Reference |

Rows show when mail arrived: the time today, "Yesterday 14:32", the weekday
within a week, then the date. The tooltip has the full date.

The Inbox loads a page at a time as you scroll.

A list opens with its last copy at once and asks the daemon again
underneath, then every 20 seconds while it is open, so mail the daemon has
synced shows without reopening the command. The Inbox pauses that once you
scroll past its first page, since reloading starts it from the top again.

The daemon keeps ready-made views of what you read recently, so those reads
are usually instant; after a few idle minutes the first read builds again,
which takes about a second on a large mailbox. Any change to stored mail,
your own actions included, makes the next read build afresh. A view
built while a sync was still writing is rebuilt two seconds after the writes
stop, so it is never more than that behind what the daemon has stored. Reads never count as a
Reading visit or mark Updates seen.

## Read a conversation

Read (Enter on most rows) opens the conversation in Raycast, newest message
first, with who it is from and to and when it arrived. Long quoted history and
signatures fold the way they do in the web reader. HTML-only mail is shown as
text, with images left out, so opening it never loads a sender's tracking
pixel. As in the web app, the conversation is marked read once it has been
open a moment; Mark as Unread (⌘⇧U) puts it back. Reply (⌘R), Reply All
(⌘⇧R) and Open in mxr (⌘O) work from the reader too.

## Search and filter by account

Every command has an account filter in the search bar; press ⌘P to open it.
Each command remembers its choice, and All Accounts is the default.

Typing in a mode command does two things. It narrows that mode's own rows,
and below them, under All Mail, it lists every conversation in the mailbox
that mxr's search finds, in the chosen account. In the Inbox command, typing
replaces the inbox with those results. Search runs through the daemon's
index with the same query language as `mxr search`, so `from:`, `to:`,
`subject:`, `is:unread`, `has:attachment`, `after:` and quoted phrases all
work; see [Search](/guides/search/). `from:` takes a whole address
(`from:support@corsair.zendesk.com`), not part of one; plain words match
sender names.

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
