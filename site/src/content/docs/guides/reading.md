---
title: Read newsletters as an edition
description: Reading turns your newsletters into an edition you visit, a reader with a 66-character column, and a Later shelf. Nothing in it is owed.
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

## Open the edition

In the web app pick **Reading** in the rail or press `g r`; in the TUI it is
**Reading** in the sidebar, also `g r`. From the command line:

```bash
mxr reading
```

```text
EDITION_OUTPUT
```

The edition has three bands:

- **Since you were last here**: what arrived after your previous visit. A
  visit ends after half an hour away, so "since you were last here" stays
  put while you read.
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
SOURCES_OUTPUT
```

## Read in the reader

Press `Enter` on an item, or from the command line:

```bash
mxr reading open ITEM
```

The reader is a single column about 66 characters wide, the width a book
uses, with the time left at your pace and a thin progress line. The
masthead, share buttons and footer are gone. `R` switches to the sender's
own layout and remembers it for that source, for the newsletters whose
design is the point.

Minutes start at 230 words a minute. Once you have read three issues to the
end, mxr uses your own pace instead, measured from them and clamped so a tab
left open overnight can't skew it.

## Fetch the linked article, only when you ask

A digest link or a teaser points at an article on someone else's site. The
reader's **Article** tab, `L`, or `mxr reading open ITEM --article` fetches
it, and nothing else ever does. The client names the site before it
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

`b` puts an item on **Later**, the one shelf in Reading with a count.
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

`A` lets go of everything in the edition. It always shows the list first,
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

Select text in the reader and press `h` (the TUI saves the paragraph at the
top of the reader). Highlights are found by [search](/guides/semantic-search/)
and exported as Markdown, grouped by item with the source and the link:

```bash
mxr reading highlight ITEM "A delete is the absence of a row" --note "for the talk"
mxr reading export --markdown > reading-highlights.md
```

Highlights are stored with the email they came from. Deleting the email
deletes them, so export what you want to keep elsewhere.

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
| `b` | Later |
| `e` | Let go |
| `D` | Unsubscribe, with evidence and a preview |
| `R` | The sender's own layout |
| `A` | Let go of everything shown, with a preview |
| `h` | Highlight the selection |
| `o` | Open the email itself |
| `?` | Help, starting with Reading |

On a phone, swipe a card right to put it on Later and left to let it go.

The old grouping by sender is still there: `mxr reading senders`.
