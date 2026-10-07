---
title: Read Updates as a briefing, then let it go
description: Read notifications twice a day as one line per source, hand what needs you to To do, and let go of the whole digest in one key.
---

Updates gathers notifications from services and apps into a digest at
fixed times, 08:00 and 16:30 by default. You read one line per source,
see what changed, and let go of the whole digest in one key. Anything
that needs you, like a failed payment, is already in To do.

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
| **Needs a look** | A new sign-in, a failed payment, a failing build, a parcel with a delivery problem, or words like "failed", "declined" or "suspended" in the subject. One line per kind of message. |
| **Changed** | A source that is new, a kind of message it hasn't sent before, a number that moved, a build that went green again, an incident that was resolved, a parcel that moved. |
| **Routine** | Everything else, one line per source with a count. |

Each line is a fact, not the subject as written. The rules clean the
subject (they drop "Re:", "[acme/api]" tags and the sender's brand), and
when the subject says nothing ("Your weekly update is here") they use the
first line of the body that does. No model writes any of it.

Numbers are quoted from the email. When the previous email of the same
kind quoted a number with the same unit, the line adds the change,
computed by code: "21.3 km over 3 runs, up 12% on last week". Two
different units, such as miles and kilometres or pounds and dollars, get
no change at all rather than a guess.

Builds and incidents fold into one line with their current state:
"Run failed: CI - main" for the latest of three failing runs, with how
long it has been failing. Parcels from [Deliveries](/guides/deliveries/)
show as trackers: ordered, shipped, out for delivery, delivered.

Every line says why it is there: "Here because: automated sender (rule).
In the 08:00 digest."

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
| `A` | Let go of the digest, after a preview |
| `e` | Let go of this source |
| `t` | This needs me: make a to-do from the line |
| `K` | Tune this source |
| `L` | Open the line's link (a tracker page, a build). Never a pay link |
| `o` | Open the email itself |
| `u` | Undo |
| `?` | What Updates is for, then its keys |

## What needs you goes to To do on arrival

A new sign-in alert, a failed payment and a parcel with a delivery problem
don't wait for the cut. They become a to-do as soon as they sync ("Check
new sign-in to Google", "Fix failed payment to Stripe") and still show in
the next digest under Needs a look, marked "already in To do". One source's alerts
of the same kind make one to-do a day, however often they sync. Mail older than two days and
mail past its window never does.

`t` on any line makes a to-do from it, with the title filled in.

## One-time codes and alerts expire and never show

Some updates stop mattering:

| Kind | Stops showing |
|---|---|
| One-time code | After the lifetime it states, else 10 minutes |
| Verify or confirm link | After the expiry it states, else 3 days |
| Sign-in or security alert | 2 days after it arrived |
| Sale or offer | When it says it ends ("ends Sunday"), else 7 days |

An update past its window never enters a digest and never goes to To do.
It stays in your inbox and in search. List them:

```bash
mxr updates --expired
```

The digest also says how many expired since you last opened Updates.

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
| `breakthrough` | Every message goes to To do on arrival |

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
- **A code isn't there:** it expired. `mxr updates --expired` lists it.
