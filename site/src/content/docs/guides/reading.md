---
title: Read newsletters as an edition
description: Reading turns your newsletters into an edition you visit, a reader set like a book page, and a Later shelf. Nothing in it is owed.
---

Reading is where newsletters, digests and posts you subscribed to land. It
is a front page you visit when you like, not a pile you owe: there is no
unread count, the sources you read most come first, and items fade on their
own unless you keep them.

The unit is a readable item, not an email:

| The email | What Reading shows |
|---|---|
| One essay | One item: its headline, source, minutes and first paragraph |
| A digest of links | The issue, with each link as its own item and blurb |
| A teaser ("New post... Read more") | The article it points at |
| A short notice | One line |

Headlines lose the newsletter's own boilerplate ("[AINews]", "Issue #42 |",
the source's name, emoji). The first paragraph skips the "View in browser"
line, the greeting and the sponsor. All of this is rules on your machine;
no model reads your newsletters.

Reading, arrivals and Inbox chips share one stored home assessment. Inspect its
rule, source and evidence with `mxr why MESSAGE_ID --format json`. New cached
body evidence, sender teaching and rule changes refresh that answer; your
Later and done state remain separate.
The stored-assessment details require changes made after
`v0.6.158`; that release does not include them.

## Open the edition

In the web app pick **Reading** in the rail or press `g r`; in the TUI it is
**Reading** in the sidebar, also `g r`. From the command line:

```bash
mxr reading
```

```text
Reading  Newsletters you chose, as an edition. Read when you like.
Later 4

SINCE YOU WERE LAST HERE
* The quiet death of the three-pane layout
    Long Reads Weekly · 14 min · you read 9 of 10
    Why every RSS reader since 2002 shipped the same window.
    0199e4a2-5b1c-7d3e-8f40-1a2b3c4d5e6f:0
* SQLite Notes, issue 112
    SQLite Notes · 6 links · 4 min · new source
    > Local-first mail is having a moment  via substack.com
      0199e4a2-6c2d-7e4f-9051-2b3c4d5e6f70:1
    > SQLite 3.51 release notes  sqlite.org
      0199e4a2-6c2d-7e4f-9051-2b3c4d5e6f70:2
    + 4 more
    0199e4a2-6c2d-7e4f-9051-2b3c4d5e6f70:0

── You left off here ──

EARLIER THIS WEEK
  Shipping a sync engine in 2026
    Platform Weekly · 9 min · you opened 3 of 5
    0199e1f0-0a1b-7c2d-8e3f-405162738495:0

FADING  Goes within a day, unless you keep it with b.
  Growth Digest #88
    Growth Digest · 3 min · you opened 0 of 11
    You opened 0 of the last 11 issues. D to unsubscribe in the app, or mxr unsubscribe.
    0199dc33-9e8f-7a6b-8c5d-4e3f2a1b0c9d:0

mxr reading open ITEM to read · later --add ITEM · let-go ITEM · sources
```

The edition has three bands:

- **Since you were last here**: what arrived after your previous visit in
  that account. A visit ends after half an hour away, so "since you were
  last here" stays put while you read. No visit is recorded while
  `MXR_ACTIVITY=off` or activity is paused.
- **Earlier this week**: still inside its source's window.
- **Fading**: in its last day.

A "You left off here" line sits between what's new and what you've seen.
Inside a band, the sources you finish most come first; a source with three
issues or fewer keeps one of the lead slots, so a new subscription is never
buried.

`mxr reading edition --peek` looks without counting as a visit, which is
what an agent or a script should use. Every item carries its key
(`<message id>:<index>`); `--format json` prints the whole edition and
`--format ids` just the keys.

## Each source fades on its own clock

An item stays for twice its source's usual gap between issues, between 2
and 14 days: a daily lasts two days, a weekly a fortnight. A source with
one issue so far gets a week. In its last day it moves to **Fading**, and
then it is done in Reading.

Fading is local. The email stays in your inbox and in search; mxr never
archives it at Gmail or your mail server because an item faded. When no
other mode holds it, it is simply out of every mode. `mxr reading sources`
lists each source's window:

```bash
mxr reading sources
```

```text
Sources, best read first

Long Reads Weekly  <editor@longreads.example>
    You opened 10 of the last 10 issues · every 7 days · fades after 14 days
SQLite Notes  <notes@sqlite-notes.example>
    You opened 1 of its 1 issue · one issue so far · fades after 7 days
Platform Weekly  <news@platform.example>
    You opened 3 of the last 5 issues · every 7 days · fades after 14 days
Growth Digest  <hello@growth.example>
    You opened 0 of the last 11 issues · every 3 days · fades after 6 days
    Unsubscribe? mxr unsubscribe --purge hello@growth.example
```

## Read in the reader

Press `Enter` on an item, or from the command line:

```bash
mxr reading open ITEM
```

The reader is a single column about the width of a book page, with the time
left at your pace and a thin progress line. The masthead, share buttons and
footer are gone. `R` switches to the sender's own layout and remembers it for
that source, for the newsletters whose design is the point.

The web reader shows the article itself: the headings, links, lists, tables
and images of the cleaned issue, set in the reader's typography. Links open in
a new tab. Remote images stay blocked until you press **Show images** (or
choose "Always" for that sender), the same as in a message, and tracking
pixels are always dropped. Colours, fonts and layout from the sender are
removed so the page reads the same everywhere. An item with no usable HTML
falls back to its plain text.

The **Aa** button beside the title sets the theme, font, size and line
spacing. The theme is **Match app** until you pick Paper, Sepia, Snow, Dusk,
Night or Ink. The choices are kept in this browser. The terminal reader shows
text only.

Minutes start at 230 words a minute. Once you have read three issues to the
end, mxr uses your own pace instead, measured from them and clamped so a tab
left open overnight can't skew it.

## Fetch the linked article, only when you ask

A digest link or a teaser points at an article on someone else's site. The
reader's **Article** tab, `L`, or `mxr reading open ITEM --article` fetches
it. So does `O` (or `mxr reading marginalia send`) on a digest link whose
article isn't saved, because Marginalia needs the article first. Nothing else
fetches one. The client names the site before it
contacts it ("Fetching from sqlite.org"), because the fetch tells that site
you clicked. Many newsletter links go through a click tracker; those show
"via substack.com" until the article is fetched, and the reader then lists
every site that was contacted.

The fetch goes through the daemon with the same guards as a remote model
endpoint:

- web links only, with tracking parameters (`utm_*`, `fbclid`, `mc_eid`
  and the like) stripped;
- never this machine or a private network: `localhost`, `.local` and
  `.internal` names are refused, and so is any name that resolves to a
  private, loopback or link-local address;
- the connection goes to the address that was checked, so a DNS answer
  can't change in between, and no system proxy is used;
- each redirect is checked like the first link, at most five of them;
- HTML only, at most 5 MB.

The article is saved, so it reads offline afterwards. A paywall or a page
that needs a browser comes back as a short reason with the link to open it
yourself.

## Keep things on Later

`b` (Not now) opens a picker whose first choice, **Later**, puts an item on
the Later shelf, the one shelf in Reading with a count; `Enter` picks it. A
time from the same picker snoozes the item's email instead. `2` opens
Later, and `1` comes back to the edition.
Putting a link on Later also fetches its article, so it is there on a
plane. Later never fades. Something on it for more than 30 days asks once,
"Still want it?", and keeping it stops the question.

```bash
mxr reading later --add ITEM --dry-run
mxr reading later --add ITEM
mxr reading later
mxr reading later --remove ITEM
```

## Let go

`e` lets go of an item: it is done in Reading. When no other mode holds the
email, it is archived at your provider, the toast says so, and `u` undoes
it. This is the same done as every mode ([Email is five apps](/guides/email-modes/)).

`E` lets go of everything in the edition. It always shows the list first,
and what it lets go of is exactly that list:

```bash
mxr reading let-go --all --dry-run
mxr reading let-go --all
```

## Unsubscribe with evidence

`D` opens an unsubscribe preview for the item's source with the evidence:
"You opened 0 of the last 11 issues". It says how the source is left
(one click, where the sender is told directly, or a page you have to open)
and that it can't be undone from mxr: you would resubscribe on their site.
It runs the existing [unsubscribe](/guides/unsubscribe/) with its dry run
first.

When a source has sent eight or more issues and you opened none of them,
its items carry a quiet "Unsubscribe?" offer. Dismiss it and it doesn't
come back for that source.

The counts only cover what mxr saw: issues that arrived after you first
opened Reading, and opens in mxr. mxr doesn't use your provider's read
flag, because letting go marks mail read there and would count a skipped
issue as opened.

## Highlight what you want to keep

In the web reader, select text and a small toolbar appears over the
selection: **Highlight**, or **Highlight with note**. `"` still highlights
the selection without the toolbar. The TUI saves the paragraph at the top of
the reader with `"`. The web reader marks a highlight on the passage you
selected: it remembers which paragraph you were in, so selecting the second
of two identical phrases marks the second. Older highlights, highlights made in
the other view (issue or article), and highlights from Marginalia are
marked wherever their text appears instead. Highlights are
listed under the article with their notes.

To change one, click it in the article or use its row in the list: edit
its note (a blank note clears it) or remove it. Removing a highlight, or
changing its note, can be undone for about a minute with `u` or `mxr undo`.
`3`, the Highlights tab, goes to the open item's highlights: on the web it
opens the item at its list, and in the TUI it lists them, where `e` edits the
note and `x` removes one (it asks first). There is no page of every
highlight yet.

Highlights you made in Marginalia are read-only here: they are read back
from Marginalia, so a change here would be undone by the next read. They are
marked "Made in Marginalia; change it there", with **Open in Marginalia**.

Highlights are found by [search](/guides/semantic-search/) and exported as
Markdown, grouped by item with the source and the link. From the CLI, the id
that `reading open` and `reading export --format json` show names a highlight:

```bash
mxr reading highlight ITEM "A delete is the absence of a row" --note "for the talk"
mxr reading highlight ITEM "the same words" --paragraph 4   # the paragraph to mark, counting from 0
mxr reading note HIGHLIGHT_ID "for the essay"          # a blank note clears it
mxr reading unhighlight HIGHLIGHT_ID --dry-run          # names what it would remove
mxr reading unhighlight HIGHLIGHT_ID                    # then `mxr undo ID` restores it
mxr reading export --markdown > reading-highlights.md
```

Highlights are stored with the email they came from. Deleting the email
deletes them, so export what you want to keep elsewhere.

## Read it in Marginalia

`O` opens the item in [Marginalia](/guides/marginalia/), a reading app with
themes, margin notes and a reading companion, once you have connected it.
The first press sends the item's text there; the highlights you make in
Marginalia come back to the item here.

## What it learns about you, and how to turn it off

To rank sources and back the unsubscribe evidence, Reading keeps a little
local engagement for each item: that you opened it, the seconds you spent,
how far you scrolled and whether you finished. It never leaves your
machine. `MXR_ACTIVITY=off` turns it off together with the
[activity log](/guides/activity-log/); Reading then ranks by recency and
makes no unsubscribe offers.

## Keys

| Key | What it does |
|---|---|
| `Enter` | Read |
| `L` | Fetch the linked article |
| `b` | Not now: Later (`Enter`), or a time |
| `e` | Let go |
| `D` | Unsubscribe, with evidence and a preview |
| `R` | The sender's own layout |
| `E` | Let go of everything shown, with a preview |
| `"` | Highlight the selection |
| `1` `2` `3` | The tabs: the edition, Later, the open item's highlights |
| `o` | Open the email itself |
| `O` | Open in Marginalia, sending it the first time |
| `?` | Help, starting with Reading |

On the web, issues are rows: the title, the dek under it, the source and
reading time on the right. The lit row shows `Enter` read, `b` later and
`e` let go (and `D` unsubscribe when mxr has an offer), and the status bar
says why the issue is here. The reader's toolbar is quiet buttons with their
keys in tooltips.

On a phone, swipe a row right to put it on Later and left to let it go.

The old grouping by sender is still there: `mxr reading senders`.
