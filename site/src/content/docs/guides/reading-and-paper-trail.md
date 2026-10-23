---
title: Reading and Paper trail compatibility
description: Use the retained Paper trail sender-bundle commands and find current Reading, Updates and Archive workflows.
---

:::note
This page retains the older Paper trail sender-bundle workflow and its CLI
and TUI compatibility commands. In the web app, `/paper-trail` redirects to
[Updates](/guides/updates/). Sourced receipts, orders and bookings live in
[Archive](/guides/archive/), and newsletters have their own
[Reading](/guides/reading/) edition. The examples below cover the retained
sender moves, pins and sweeps.
:::

The older workflow grouped mail outside Messages into Reading and Paper
trail. Reading now has its own edition. Paper trail remains a sender-bundle
compatibility view; current notifications are handled in Updates.

| Place | What is in it | How it reads |
|---|---|---|
| **Reading** | Newsletters and mailing lists. | An [edition](/guides/reading/): readable items banded by your last visit, ranked by what you read. Nothing counts as unread. |
| **Paper trail** | Receipts, notifications, alerts and other automated mail. | One line per sender: how many, the newest subject, how long ago. Open a line to see its messages. |

Receipts, orders, bookings and bills that mxr recognises are also filed
as records in [Archive](/guides/archive/), which keeps them after you sweep
their emails and answers with the field you need, like an order number.

Notifications now have their own mode. [Updates](/guides/updates/) reads
automated mail as a briefing: one line per source, what changed first,
gathered at the configured cuts and let go after review. `mxr paper-trail`
and `mxr sweep` still address the older sender-bundle view.

Both places are views over the inbox. Archiving a message takes it out, and
undo puts it back. Deliveries and calendar invites have their own pages, so
they are in neither. Neither place carries a count in the sidebar; the
desk's **Everything else** line says how much arrived this week.

## Open a place

In the TUI, Reading and Paper trail remain sidebar lenses. The web app's old
`/paper-trail` route redirects to Updates. From the command line, inspect the
legacy sender bundles with:

```bash
mxr paper-trail --messages 2
```

```text
Paper trail · 3 messages from 3 senders

Uptime Robot                       1  2026-09-28
  here because: automated sending domain
    Interview panel for Staff Engineer candidate                      cc2cbc75-87a9-5c63-b941-aa63ec3607fb

Pager Relay                        1  2026-09-27
  here because: automated sending domain
    Launch checklist for Project Aurora                               732401d8-f0db-5459-ae9f-7e611d28226f

Build Watch                        1  2026-09-27
  here because: automated sending domain
    Pricing page copy review                                          e6de11f8-127a-58f6-8ce6-d828078395bd
```

`mxr reading senders` prints the same shape for Reading. `--sender ADDRESS`
shows one sender's bundle and `--account NAME` one account.

## Check why a message is here

Every bundle and every issue says why it is there ("here because: has
List-Unsubscribe"). The rules are fixed and local; no model decides. To ask
about one message:

```bash
mxr why cc2cbc75-87a9-5c63-b941-aa63ec3607fb
```

```text
uptime@alerts.demo.mxr.local: paper-trail (automated sending domain)
```

The full list of rules, in the order they apply, is in the
[placement rules reference](/reference/desk-and-places/#placement-rules).

## Move a sender for good

When a sender is in the wrong place, move them from the CLI or the TUI's
Reading and Paper trail lenses. Choose the legacy Paper trail destination
only when you want to keep using sender bundles:

| Kind | Where their mail goes |
|---|---|
| **People** | The desk, like anyone you write to. |
| **Reading** | The feed. |
| **Paper trail** | The bundles. |
| **Screened out** | Nowhere. New mail from them is trashed and marked read as it arrives. |
| **Automatic** | The rules decide again. |

From the command line:

```bash
mxr sender kind uptime@alerts.demo.mxr.local reading
```

```text
uptime@alerts.demo.mxr.local is now reading (was automatic).
```

```bash
mxr why cc2cbc75-87a9-5c63-b941-aa63ec3607fb
```

```text
uptime@alerts.demo.mxr.local: reading (you moved this sender to Reading)
```

`mxr sender kind ADDRESS auto` hands the sender back to the rules.

The move applies to their inbox mail at once and to their mail from then on:
move a newsletter to People and it leaves Reading and appears on the desk.
Undo in the web app or TUI puts back the kind they had before. Kinds are
stored as the sender's [screener decision](/guides/triage-flow/), so the
Screener's **Decisions** tab lists them too.

Mail that releases before v0.6.38 archived on arrival for feed and
paper-trail senders stays archived. Their new mail appears in the place.

## Pin what you want to keep

A sweep leaves pinned messages where they are. Pin the few you want to keep
in view before you sweep, from the TUI or command line:

```bash
mxr pin cc2cbc75-87a9-5c63-b941-aa63ec3607fb
```

```text
Pinned 1 message.
```

Pins are local to this machine. They are not provider stars.
`mxr unpin MESSAGE_ID` lets the next sweep take the message.

## Sweep the rest

A sweep archives everything unpinned in one sender's bundle, or in the whole
place. Always preview first:

```bash
mxr sweep paper-trail --dry-run
```

```text
Would archive 2 messages from Paper trail; 1 pinned message stays.
  Build Watch                        1
  Pager Relay                        1
    Launch checklist for Project Aurora
    Pricing page copy review
```

Then sweep:

```bash
mxr sweep paper-trail --yes
```

```text
Archived 2 messages from Paper trail; 1 pinned message stays.
  Build Watch                        1
  Pager Relay                        1
    Launch checklist for Project Aurora
    Pricing page copy review
Undo with:
  mxr undo 01a0e733-cc1f-71c0-b554-c03d6170d24c
```

Add `--sender ADDRESS` to sweep one bundle. Without `--yes` or `--dry-run`,
`mxr sweep` shows the preview and asks; outside a terminal it refuses.

In the TUI, preview a sender's bundle or the whole place before confirming
the sweep. The preview comes from the same daemon request as the apply.

What a sweep archives:

- Only the messages its preview listed. Mail that arrived after the preview
  is left for next time, and a message pinned or moved away in the meantime
  stays.
- Nothing, if the preview has expired (after 10 minutes). Preview again.

## Undo a sweep

Undo puts every swept message back. Use the TUI toast's **Undo** or an id
the sweep printed:

```bash
mxr undo 01a0e733-cc1f-71c0-b554-c03d6170d24c
```

```text
Undone
```

A large sweep runs in chunks and prints one undo id per chunk; run
`mxr undo` for each. The undo window is about a minute.

## Keys

Use the TUI's Reading and Paper trail keys from the
[keybindings reference](/reference/keybindings/#reading). For the current
web workflows, use [Reading](/guides/reading/) and [Updates](/guides/updates/).

Next: when the desk is clear, the web app shows
[low tide](/guides/sound-hints-and-touch/#low-tide). Back to
[your day in mxr](/guides/your-day/).
