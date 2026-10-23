---
title: Read Updates as a briefing, then let it go
description: Read notifications twice a day as one line per source, hand what needs you to To do, and let go of the whole digest in one key.
---

Updates gathers notifications from services and apps into a digest at
fixed times, 08:00 and 16:30 by default. You read one line per source,
see what changed, and let go of the whole digest in one key. Press `t` on a line that needs action, such as a failed payment, to add it
to To do. A suggested task stays a suggestion until you choose to create it.

```bash
mxr updates                  # the latest digest and what arrived since
mxr updates let-go --dry-run # what letting go would do
mxr updates let-go           # let go of exactly that set
```

In the web app press `g u`, or open the Updates card on Now. In the TUI,
pick **Updates** in the sidebar.

## Each source is one line, sorted by what it asks of you

Updates holds automated mail in your inbox: the same senders the
classifier puts in Updates, such as `notifications@github.com` or a
`no-reply` address. Thirty emails from twelve senders read as twelve
lines, in three sections that never change order:

| Section | What lands there |
|---|---|
| **Needs a look** | A new sign-in, a failed payment, a failing build, a parcel with a delivery problem, or words like "failed", "declined" or "suspended" in the subject. One line per kind of message: two failed-payment emails from one source ("Payment failed" and "Payment failed again") are one line that shows the latest and counts both. |
| **Changed** | A source that is new, a kind of message it hasn't sent before, a number that moved, a build that went green again, an incident that was resolved, a parcel that moved. |
| **Routine** | Everything else, one line per source with a count. In the web app, Routine is one line, "Routine · 16 sources", with a chevron, until you press `Enter` or click it to open it. |

Each line is a fact, not the subject as written. The rules clean the
subject (they drop "Re:", "[acme/api]" tags and the sender's brand), and
when the subject says nothing ("Your weekly update is here") they use the
first line of the body that does. The rules write the line.

### A model can say what the email says

When the fact is only the subject, "Your receipt from Perplexity", and you
have a model configured for summaries (`[llm]`, the same one that writes
the gist in the reader), the line swaps to a one-line gist once it is ready:
"Receipt: $24.00 for Perplexity Pro, renews 14 Nov". The subject moves to
the line's hover text and the top of its details. Until the gist arrives,
with no model, or when it fails, the line shows the rules' fact exactly as
before. There is no spinner.

A gist is only kept when every number in it appears in the email, in any
spelling ("24" for "24.00", "1.234,50" for "1,234.50"). A gist with an
amount, date, count or ID the email does not contain is dropped, and so is
one with a currency the email never names. The email's text goes to the
model under the same privacy rule as `mxr summarize`; a cloud model sees it
only if that is what you configured for summaries.

```bash
mxr briefing gists THREAD_ID --generate --automated --format json
```

`--automated` also queues receipts, alerts and other automated mail, which
`--generate` alone leaves out.

Numbers are quoted from the email. When the previous email of the same
kind quoted a number with the same unit, the line adds the change,
computed by code: "21.3 km over 3 runs, up 12% on last week". Two
different units, such as miles and kilometres or pounds and dollars, get
no change at all rather than a guess.

Builds and incidents fold into one line with their current state:
"Run failed: CI - main" for the latest of three failing runs, with how
long it has been failing. Parcels from [Deliveries](/guides/deliveries/)
show where the parcel is on its way under the fact, such as "Shipped ·
next: out for delivery". Their available item names, tracking or order
references, and delivery estimates sit in the details.

In the web app, the page says its count once ("31 updates from 12 sources")
and names the cut in its header ("16:30 digest"). A line is the source and
its fact on one row. The lit line shows its keys, `Space` peek, `e` let go,
`t` to-do and `o` email, with a **⋯** menu for the rest, and the same keys
show in the status bar. Press `Space` on a line to open its details under it:
the first few lines of the latest email, why the update is here, the numbers
the mail quoted, a parcel's items and references, and the link and the
email. When a gist replaced the subject, the details also show the subject,
what the email asks of you if it does, and which model wrote the line. The
email's lines load when the details open, as plain text. `Escape` closes
them.

## The digest is a fixed cut; mail after it waits

The digest holds everything that arrived by the latest cut and that you
haven't let go of. Anything left from an earlier digest folds into this
one instead of stacking. Mail that arrives after the cut sits below the
digest, quiet and uncounted, as "arriving for 16:30", until the next cut.

```bash
mxr updates --cut 08:00                    # the 08:00 cut, today or yesterday
mxr updates --cut 2026-10-07T08:00:00Z     # or RFC3339
```

To change the times, set `updates.cuts` in your config to one to four
times. See the [config reference](/reference/config/#updates).

```toml
[updates]
cuts = ["07:30", "12:00", "18:00"]
```

## Let go of the digest in one key

Letting go takes every update in the cut out of Updates. Shown or hidden,
the whole cut goes, and nothing that arrived after it. A conversation no
other mode holds is archived at your provider, the same rule as
[done here](/guides/now/#act-on-a-row-in-its-own-mode); one that To do
still holds stays in the inbox.

Preview first:

```bash
mxr updates let-go --dry-run
```

The preview says how many updates from how many sources, how many of
those the digest wasn't showing (tuned away or past their window), and how
many To do keeps. Without `--dry-run`, `mxr updates let-go` previews and
then lets go of exactly the set it previewed, and prints an id for
`mxr undo`. If mail landed in the cut between the two, it refuses and asks
you to preview again.

To let go of one source only:

```bash
mxr updates let-go --source github.com/acme/api --dry-run
```

The keys:

| Key | What it does |
|---|---|
| `Enter` | Open Routine, when it is collapsed (web) |
| `Space` | Open or close the details under this line (web) |
| `E` | Let go of the digest, after a preview |
| `e` | Let go of this source |
| `t` | This needs me: make a to-do from the line |
| `P` | Tune this source |
| `L` | Open the line's link (a tracker page, a build). Never a pay link |
| `y` | Copy the line's one-time code |
| `o` | Open the email itself |
| `s` | Pin or unpin the latest email behind this line |
| `S` | Preview a sweep of this sender in this account; unavailable for parcels |
| `u` | Undo |
| `?` | What Updates is for, then its keys |

## Pin an email before sweeping its sender

In Updates, `s` pins the selected line's latest email. The pin is local and
survives reloads; changes made in the CLI, terminal or web app appear when
the digest refreshes. It protects that email from a sweep, rather than
pinning every future email from the sender.

Press `S` to preview a sender sweep. It applies to that line's account,
leaves pinned emails in place and asks before archiving. Cancel to leave
the mail untouched, or use `u` after applying to undo it. Parcel trackers
do not offer a sender sweep.

In the TUI, the `Account:` line shows the configured name and address when
they fit; otherwise, it favors the address.

## What needs you leads Needs a look as a suggested to-do

A new sign-in alert, a failed payment and a parcel with a delivery problem
sit at the top of Needs a look. The line carries a To do button, and its
details say why: "Suggested to-do: new sign-in alert from Google
(accounts.google.com) (rule). t adds it." mxr never adds the to-do for
you; `t` or the button does, with the title filled in ("Check new sign-in
to Google (accounts.google.com)"). The line then shows "already in To do".

Anyone can put "New sign-in" in a subject or "Google" in a display name, so
every line in Needs a look and every title `t` fills in shows the host the
mail came from beside the name. A sign-in alert from "Google" at
`g00gle-alerts.example` reads "Google (g00gle-alerts.example)".

`t` on any other line makes a to-do from it too.

## One-time codes and alerts expire and never show

Some updates stop mattering:

| Kind | Stops showing |
|---|---|
| One-time code | After the lifetime it states, else 10 minutes |
| Verify or confirm link | After the expiry it states, else 3 days |
| Sign-in or security alert | 2 days after it arrived |
| Sale or offer | When it says it ends ("ends Sunday"), else 7 days |

An update past its window never enters a digest.
It stays in your inbox and in search. List them:

```bash
mxr updates --expired
```

The digest also says how many expired since you last opened Updates.

## Copy a one-time code

A code you just asked for arrives between cuts, before any digest line
shows it, so Updates puts the newest one still in its window at the top
of the web app's briefing, with the sender's host beside its name ("Google
(accounts.google.com)") so a borrowed brand shows before you paste. Click
it, or press `y`: `y` copies the lit line's code, else that newest one. In
the TUI, `y` does the same, asking the daemon for the newest code when the
line has none.

mxr finds the code by rules in the subject and the top of the body, never
by a model: "Your verification code is 482913", "G-123456 is your Google
verification code", "Your login code is:" with the code on the line below,
or a subject that says "verification code" over a body that is the code. A
promo, gift or discount code, a booking PIN, a bank's branch code or a
card's last four digits is never one. A code lasts the lifetime its email
states, else 10 minutes; a sign-in alert that carries one still stays two
days in Updates. Mail dated in the future (a sender's clock or a trick) is
not taken as the latest code.

From the command line, `mxr code` prints the newest code still in its
window, whichever folder the email is in:

```bash
mxr code --copy                   # also put it on the clipboard
mxr code --format ids | pbcopy    # the code alone
mxr code --all                    # the last day's codes, expired ones marked
```

It exits with an error when no code is current, naming the last one that
expired.

## Tune a source

```bash
mxr updates source github.com/acme/api changes-only --dry-run
mxr updates source notifications@strava.com muted
```

| Setting | The source's mail |
|---|---|
| `every-digest` | In every digest (the default) |
| `changes-only` | Only when something changed or needs a look |
| `muted` | Never in the digest; still in your inbox, Archive and search |
| `breakthrough` | Every message leads Needs a look with a suggested to-do |

A source is named by the key the digest prints (the sender's domain, plus
a repository for GitHub), or by any address it sends from. The output
prints the command that puts the old setting back.

If you let go of a source in eight digests in a row without opening any
of its mail, its line asks once whether to mute it, and asks again no
sooner than a month later.

## Script it

`mxr updates --format json` prints the whole digest: the cut, the three
sections, `since`, and `selection_token`. `--format jsonl` prints one line
per source line with a `section` field (`needs_a_look`, `changed`,
`routine`, `since`). See [JSON output](/reference/json-output/#mxr-updates).

```bash
# Facts that need a look, with their source.
mxr updates --format jsonl | jq -r 'select(.section == "needs_a_look") | "\(.source_name): \(.fact)"'
```

Agents can read the digest and preview a let go over MCP with
`mxr_updates_digest` and `mxr_updates_let_go_preview`; letting go stays
with you.

## When something is not where you expect

- **A notification isn't in Updates:** run `mxr why MESSAGE_ID`. A sender
  the rules call a person goes to Messages; move it with
  `mxr sender kind ADDRESS paper-trail`.
- **A source shows nothing:** it may be muted or changes-only. The digest
  says how many updates tuning hid.
- **A parcel vanished:** delivered parcels leave a day after delivery, and
  a parcel with no news goes quiet. See [Deliveries](/guides/deliveries/).
- **A code isn't there:** it expired. `mxr updates --expired` lists it, and
  `mxr code --all` lists the last day's codes.
