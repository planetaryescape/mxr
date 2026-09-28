---
title: Reading and Paper trail
description: Mail that isn't from people gets its own places. Newsletters and lists read as a feed, receipts and notifications bundle by sender, every placement says why, one key moves a sender, and a sweep clears a bundle or a whole place with a preview and undo.
---

The [desk](/guides/desk/) holds mail from people. Everything else that reaches
your inbox goes to one of two places, and none of it is counted as work:

| Place | What is in it | How it reads |
|---|---|---|
| **Reading** | Newsletters and mailing lists. | A feed, newest first, with every issue already open. Nothing is bold and nothing counts as unread. |
| **Paper trail** | Receipts, notifications, alerts and other automated mail. | One line per sender: how many, the newest subject, how long ago. Open a line to see its messages. |

Deliveries and calendar invites have their own pages (an invite may still
need an answer), so they stay out of both places. Both places are views over
the inbox: archiving a message takes it out, and undo puts it back.

## Why a message is where it is

Every bundle, every issue and every such message in the reader says why it
is there, for example "Here because: automated sender, has List-Unsubscribe."
The rules are fixed and local; no model decides. The first rule that matches
wins:

1. **Your choice.** You moved this sender (see below): "you moved this sender
   to Reading".
2. **A delivery update or a calendar invite.**
3. **A notifying address or domain**: `notifications@`, `alerts@`,
   `receipts@`, `billing@` and the like, or a host such as
   `alerts.example.com`. Paper trail, even with list headers (GitHub's
   notifications carry List-Unsubscribe).
4. **A newsletter address or domain**: `newsletter@`, `digest@`, or a host
   such as `news.example.com`. Reading.
5. **A `List-Id` header**, then **a `List-Unsubscribe` header**. Reading.
6. **A `no-reply@` address with no list headers.** Transactional mail, so
   Paper trail. (A `no-reply@` sender that does carry List-Unsubscribe is
   marketing, so it reads as Reading.)
7. **A sender known to write to lists.** Reading.
8. Anything else is a person, and belongs on the desk.

The rules look at each message, so a shop that sends both receipts and offers
can show up in both places, each message with its own reason.

## Move a sender, for good

Press `K` in Reading or Paper trail (or use **Move sender…** under a message
in the reader, or the palette's **Move sender to…**) and choose:

| Key | Kind | Where their mail goes |
|---|---|---|
| `p` | People | The desk, like anyone you write to. |
| `r` | Reading | The feed. |
| `t` | Paper trail | The bundles. |
| `x` | Screened out | Off the desk and every place. New mail from them is trashed and marked read as it arrives. |
| `a` | Automatic | The rules above decide again. |

The choice is remembered for that sender, so their future mail follows it.
It moves their mail at once: move a newsletter to People and it leaves
Reading and appears on the desk. The toast's Undo (or `u`) puts back the kind
they had before. Choices are stored as the sender's
[screener decision](/guides/triage-flow/) (People is allow, Reading is feed,
Paper trail is paper-trail, Screened out is deny), so the Screener's
**Decisions** tab lists them too.

A sender you move to Reading or Paper trail keeps their mail in the inbox:
those places are views over the inbox. Earlier releases dropped the inbox
label from feed and paper-trail senders as mail arrived, which hid it from
these places; that no longer happens. Mail those senders sent before this
release stays archived, and their new mail appears in the place.

## Pin the exceptions, sweep the rest

A sweep archives everything unpinned in one sender's bundle, or in the whole
place. Pin the few messages you want to keep in view first: `p` pins or
unpins the message under the cursor (or use the pin at the end of its line).
Pins are local to this machine and are not provider stars.

1. `S` sweeps the bundle under the cursor; `A` (or **Sweep all**) sweeps the
   whole place. The keys are the same in the web app and the TUI.
2. The preview comes from the daemon's dry run of the same request: how many
   messages, from which senders, a few subjects, and how many pinned messages
   stay. When the sweep reaches past what is on screen, it says so ("Archives
   143 messages, 20 shown here").
3. Confirm, and the daemon archives only the messages that preview listed,
   as a background job. Mail that arrived after the preview is left for next
   time, and a message pinned or moved away in the meantime stays. A preview
   works once and expires after a few minutes; if it has expired, nothing is
   archived and you can preview again.
4. The toast's Undo (or `u`, for about a minute) puts every swept message
   back.

## Keys in the web app

| Key | In Reading and Paper trail |
|---|---|
| `g r` / `g p` | Go to Reading / Paper trail |
| `j` / `k` | Next / previous issue or line |
| `Enter` | Open a bundle, or open the conversation |
| `p` | Pin or unpin |
| `S` | Sweep this sender's bundle |
| `A` | Sweep the whole place |
| `K` | Move sender to… |
| `D` | Unsubscribe |
| `u` | Undo the last change |

Once a conversation is open, the reader's own keys apply (`K` there is the
previous message).

Before 0.6.41 the web app used `g R` / `g P` to get here and `s` / `S` to
sweep. `g R` and `g P` still work for one more release. `s` no longer
sweeps, because it stars everywhere else.

The desk's **Everything else** line links to both places with how much
arrived this week ("Reading 7 this week", "Paper trail 14 this week").
Those are not unread counts, and the sidebar shows Reading and Paper trail
with no count at all.

## In the CLI

```bash
mxr reading                      # bundles, newest first, with the reason for each
mxr paper-trail --messages 5     # up to five messages listed per sender
mxr paper-trail --sender receipts@shop.example --format json
```

`--format json` returns `bundles` (each with `sender_email`, `kind` with
`kind`, `rule`, `reason` and `corrected`, `message_count`, `pinned_count`,
`newest_at` and its `messages`), `total_bundles` and `total_messages`.
`--format jsonl` prints one bundle per line and `--format ids` prints the
listed message ids. `--account` limits a place to one account.

```bash
# Why is this message where it is?
mxr why MESSAGE_ID

# Move a sender, or hand them back to the rules.
mxr sender kind digest@news.example.com reading
mxr sender kind digest@news.example.com auto

# Pin the ones to keep, then preview and sweep.
mxr pin MESSAGE_ID
mxr sweep paper-trail --sender receipts@shop.example --dry-run
mxr sweep paper-trail --yes
mxr undo MUTATION_ID        # once for each id the sweep printed
mxr unpin MESSAGE_ID
```

`mxr sweep` without `--yes` shows the preview and asks before it archives;
outside a terminal it refuses and asks for `--yes` or `--dry-run`. The real
sweep commits its own preview's token, so it archives only what the preview
listed. JSON output includes the preview, the archive job and its
`undo_ids`.

## In the TUI

Reading and Paper trail are lenses in the sidebar (`g r` and `g p` from the
mail list), bundled by sender, each bundle with its "here because" line and
no unread counts.

| Key | Does |
| --- | --- |
| `Enter` | Open the message beside the list |
| `p` | Pin or unpin the message under the cursor |
| `K` | Move the sender: `p` people, `r` Reading, `t` Paper trail, `x` screened out, `a` automatic |
| `S` | Preview sweeping this sender's bundle, then `Enter` to archive |
| `A` | Preview sweeping the whole place (also in the command palette) |
| `e`, `s`, `#` | Archive, star or trash the message under the cursor |
| `u` | Undo the last sweep, every chunk of it |
