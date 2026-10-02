# Now, identity and handoff: what connects the five modes

Research for [22-email-modes.md](../../blueprint/22-email-modes.md), covering
the parts that sit between the modes: the Now view, how one email shows in
several modes, how an item passes from one mode to the next, the rail and
keys, and how each mode gets its own feel. Code claims are against
`bdf997c4` on the `modes` clone (`/tmp/mxr-feel-clones/modes`). Screenshots
are the judge's tour of the demo mailbox (`01-desk-after.png` to
`18-default-theme-os-light-after.png`).

Evidence and opinion are kept apart. A line backed by a study or a vendor's
own docs cites it. A line that is design judgement says so ("judgement:").
Two sources below are weak (an AI-flavoured review blog and vendor
marketing) and are marked where used.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## The five modes are what people already do with email, so the research backs the split

Thirty years of field studies describe email as several tools sharing one
list. Mackay found people using mail for time and task management as well
as communication, and split them into "prioritizers" (manage what comes in)
and "archivers" (keep things for later use)
([Mackay 1988](https://dl.acm.org/doi/10.1145/62266.62293)). Whittaker and
Sidner named three jobs that email was doing badly at once: task
management, personal archiving and asynchronous communication, and found
inboxes cluttered with "outstanding tasks, partially read documents and
conversational threads"
([Whittaker and Sidner 1996](https://dl.acm.org/doi/pdf/10.1145/238386.238530)).
That list is Messages, To do, Reading and Archive almost word for word.
Bellotti's Taskmaster then tried to recast email as task management and
grouped messages, links and drafts by task into "thrasks"
([Bellotti et al. 2003](http://www.chi2003.org/docs/takingemail.pdf)).

What mxr adds to that literature is the claim that one message can be in
several of these jobs at once, with a different view of it in each. The
studies support the jobs; nothing found tests the multi-view claim
directly, so that part is a design bet (see Risks).

Two findings set constraints on the whole design:

- Filing doesn't pay; search does. In a study of 345 long-term users and
  over 85,000 refinding actions, people who built folders relied on them,
  but that preparation was inefficient and did not improve retrieval
  success; search and threading did
  ([Whittaker et al. 2011](https://research.ibm.com/publications/am-i-wasting-my-time-organizing-email-a-study-of-email-refinding)).
  Archive should be search with a records facet, and handing something to
  Archive must cost the user nothing (no folder to pick).
- Batching helps how productive people feel, not how stressed they are. In
  an in-situ study of 40 information workers over 12 days, batchers rated
  their productivity higher, but the authors "found no evidence that
  batching email leads to lower stress"
  ([Mark et al. 2016](https://doi.org/10.1145/2858036.2858262)). The daily
  Updates digest is justified on attention, and mxr should not market it
  as calm.

## A. The Now view: four sections, three items each, fixed order, a real end

### What the best apps do

Things 3 separates when a to-do should show up from when it is due. A
to-do appears in Today when its start date, deadline or repeat matches
today; a future start date hides it until then; "This Evening" keeps later
items "still present, so you know there's more to do, but unobtrusive
enough to not bother you until you have time"
([Things support](https://culturedcode.com/things/support/articles/4001304/)).
That is the blueprint's `surface_at` versus `due_at`, already proven in a
shipping product.

Sunsama is built on a ritual with a start and an end: review yesterday's
carry-overs, pull in today's items, then a shutdown ritual that reviews
what was done and carries forward the rest. It warns when planned time
exceeds a workload threshold
([Sunsama manual](https://help.sunsama.com/docs/usage-guides/daily-planning/)).
The warning is the interesting part for mxr: a home that knows when you
have too much is more honest than one that lists everything.

HEY's Imbox puts new mail at the top and previously seen mail below, "like
a newsfeed", with no obligation to archive
([HEY features](https://www.hey.com/features/),
[Moving from Gmail](https://www.hey.com/moving-from-gmail/)). HEY Bundles
collapse a repetitive sender to a single row "no matter how many emails
they send you". Shortwave bundles likewise make a group one item you can
snooze or mark done as a unit
([Shortwave bundles](https://www.shortwave.com/docs/guides/bundles/)).
A digest is one thing, so it should take one slot.

Superhuman's answer to a big inbox is a command: Get Me To Zero archives
everything older than a chosen age
([Superhuman updates](https://new.superhuman.com/get-me-to-zero-260284)).
It produces an end state by force. Judgement: it is a reset button, not a
daily rhythm, and it treats "old" as "done", which is the opposite of the
Now view's job (an old ask from someone you care about is the thing to
show).

Working memory holds about four chunks, three to five in practice
([Cowan 2001](https://philpapers.org/rec/COWTMN)). A home screen read at a
glance should present no more than four groups, each a chunk.

### What fails today

The desk (`01-desk-after.png`) is three lanes of 14, 4 and 7 rows, every
row in the same typography. It answers "what is in my mail" rather than
"what needs me now". Two defects come from classification, not layout:
"Action required: unusual sign-in attempt" sits in You owe, and "Build
failed on release branch" from "Account Verification" sits in New from
people. Waiting on includes "Weekly local-first reading list", which is a
newsletter. The Owed replies page (`14-owed-after.png`) lists UPS, Amazon
and Payroll Notice as replies owed. Fourteen rows in You owe is beyond what
anyone acts on in one sitting, and the page has no point at which it says
"that's enough for today".

Two things on today's desk are worth keeping. The headline ("Friday
afternoon. 14 replies.") already reads the time of day. The "Everything
else" strip at the foot (Reading 6 this week, Paper trail 3, Deliveries 2,
Screener 27) is the right shape for modes that don't need you now: a count
and a door, not rows.

### Proposal: Now is a front page with four fixed sections

Judgement, built from the evidence above:

1. Four sections in a fixed order: People (You owe, from Messages), Due
   soon (To do), Today's updates (one card), and a Reading pick (one item,
   afternoon and evening only). The order never changes, so the eye learns
   where things are. Urgency shows in the item's time text, not by
   reshuffling sections.
2. At most three items per section, then "and 9 more in Messages". The
   cap is per section, so Now never shows more than ten things.
3. A section with nothing in it disappears. When every section is empty,
   the existing low tide scene (`apps/web/src/features/low-tide/LowTide.tsx`,
   `--motion-duration-scene` in `apps/web/src/styles/tokens.css`) plays
   with one line that says when Now will next have something: "Clear. The
   next to-do surfaces Monday 09:00."
4. An overload line in the Sunsama style when You owe is long: "11 people
   are waiting. Three are close; start there." This replaces showing all
   11.
5. Time of day: morning shows yesterday's updates digest; after you open or
   let go of it, the card shrinks to one line. From 17:00 the Reading pick
   appears ("for tonight"), as Things' This Evening does.

Ordering inside each section: People by relationship band then owed age
(the blueprint's Messages ranking); Due soon by `due_at`; one digest; one
reading pick by the sender you open most.

Desktop web:

```text
+----------+---------------------------------------------------------------+
| Now    3 |  Friday afternoon. 3 people, 2 things due.                    |
|----------|                                                               |
| Messages |  PEOPLE                                     all 11 in Messages |
| To do  2 |  Maya Ortiz     Launch checklist for Aurora      22h  [r] [e]  |
| Updates  |  Sam (landlord) Lease renewal, around Thursday?   3h  [r] [e]  |
| Reading  |  Iris Chen      Incident review: delayed sync    1d  [r] [e]  |
| Archive  |                                                               |
|----------|  DUE SOON                                     all 4 in To do  |
| Inbox    |  [ ] Sign lease renewal          due Wed 15 Oct   Sam         |
| More  >  |  [ ] Pay council tax             due Mon 6 Oct    Council     |
|          |                                                               |
|          |  TODAY'S UPDATES  ------------------------------------------  |
|          |  | 9 updates since yesterday from 6 senders                 | |
|          |  | 2 deliveries arriving . 1 sign-in from new device . ...  | |
|          |  | [Open digest  g u]   [Let go of the day  A]               | |
|          |  ----------------------------------------------------------  |
|          |                                                               |
|          |  Not now: Reading 6 this week . Screener asks about 2 people  |
+----------+---------------------------------------------------------------+
```

Mobile (390 px):

```text
+------------------------------+
| Friday afternoon             |
| 3 people, 2 things due       |
|                              |
| PEOPLE                  11 > |
| Maya Ortiz            22h    |
|  Launch checklist for Aurora |
| Sam (landlord)         3h    |
|  Lease renewal, Thursday?    |
|                              |
| DUE SOON                 4 > |
| ( ) Sign lease  Wed 15 Oct   |
| ( ) Pay council tax  Mon 6   |
|                              |
| [ 9 updates since yesterday ]|
|                              |
|------------------------------|
| Now  Msgs  To do  Read  Find |
+------------------------------+
```

TUI (the desk lens, `crates/tui/src/ui/desk_lens.rs`, rebuilt):

```text
 Now                        Fri 15:10 . 3 people, 2 due
 ── People ─────────────────────────────── 11 in Messages
 > Maya Ortiz      Launch checklist for Aurora      22h
   Sam (landlord)  Lease renewal, around Thursday?   3h
   Iris Chen       Incident review: delayed sync     1d
 ── Due soon ───────────────────────────────── 4 in To do
   [ ] Sign lease renewal           Wed 15 Oct  Sam
   [ ] Pay council tax              Mon 6 Oct   Council
 ── Today's updates ──────────────────────────────────────
   9 since yesterday from 6 senders   Enter open  A let go
 r reply  e done here  t to do  Enter open  g m messages
```

Daemon and CLI: the blueprint's `GetUpdatesDigest` plus a `GetNow` request
that returns the four sections with their caps and "more" counts in one
response, so web, TUI and `mxr now --format json` agree on what Now shows.
The section cap belongs in the daemon, not the client, so the counts match
everywhere (the blueprint makes the same argument for one owed rule).

## B. One email, many views: the identity strip and "also in"

### What the best apps do

OOUX starts from objects, then gives each object its actions, so "when
users see an object, they already know what to do with it based on how
similar objects worked on other pages"
([Prater, A List Apart](https://alistapart.com/article/object-oriented-ux/)).
For mxr the object is the email (really the thread), and each mode is a
context that shows a subset of its properties and actions. OOUX's rule
that an object keeps the same core properties wherever it appears is what
lets a user see "this is the same email".

Gmail's schema.org actions show one email offering a different primary
action depending on what it is: a one-click action confirms "a predefined
request" in place, and a go-to action opens the page that does the work,
such as an airline check-in
([Gmail markup actions](https://developers.google.com/gmail/markup/actions/actions-overview)).
The action comes from the content, not from the folder.

Gmail categories are the closest shipping precedent for one email in
several places: categorised mail is still in the inbox, a user drags a
message to another tab to correct it, a confirmation appears with undo,
and starred mail shows in more than one place
([Gmail help](https://support.google.com/mail/answer/3055016)).

Linear's Inbox separates a notification about an issue from the issue: you
delete or snooze the notification, and deleting it "doesn't affect the
underlying issue"
([Linear docs](https://linear.app/docs/inbox)). That is the per-mode done
state in the blueprint: done in Updates, the to-do stays.

### What fails

A traditional client shows one row shape everywhere: sender, subject,
snippet, time. mxr does this today in Inbox, Owed and the desk
(`03-inbox-after.png`, `14-owed-after.png`). Reading is the exception and
the model to follow: it shows the newsletter as an article with its own
verbs (Pin, Unsubscribe, Move sender) and a reason ("Here because: has
List-Unsubscribe", `07-reading-after.png`).

The opposite failure is a view so different that the user can't tell it is
the same email. A to-do titled "Sign lease renewal" with no sender and no
link back reads like a separate item from a separate app.

### Proposal: same anchor, different body, and a quiet "also in" line

Judgement:

- Every view of an item keeps an identity anchor: the sender's initials in
  the same avatar, and the thread subject in the same muted mono line.
  What changes per mode is the headline and the verbs. In To do the
  headline is the task ("Sign lease renewal"); in Messages it is the
  person; in Archive it is the record ("Lease renewal 2026-27, PDF").
- An "Also in" line under the anchor names the other modes holding the
  item and what that aspect is: "Also in To do: sign by Wed 15 Oct". It is
  a link (`g` then the mode's letter jumps to that item in that mode). Only
  other modes show, never the current one, and nothing shows when the item
  is in one mode.
- The reason line stays, as Reading already does it: "Here because: Sam is
  someone you write to (12 emails)".
- Detected actions from schema.org data become the mode's primary button
  where they exist (a `ConfirmAction` becomes the To do row's "Confirm").

The shared component is a `ModeItemAnchor` (avatar, subject, also-in,
reason) used by every mode route; each mode supplies its own headline and
verb row. `GetModeMembership` already in the blueprint returns what the
also-in line needs.

## C. Handoff: one key per destination, the toast names where it went, the destination shows it arrived

### What the best apps do

Gmail's tab correction is drag, confirmation, undo
([Gmail help](https://support.google.com/mail/answer/3055016)). Superhuman
and Shortwave make "Done" mean archive, and Superhuman pairs deferral with
a concrete return time ("pick a concrete time")
([Superhuman blog](https://blog.superhuman.com/inbox-zero-method/),
[Shortwave](https://www.shortwave.com/docs/guides/bundles/)). HEY moves an
email to a named pile (Reply Later, Set Aside) that lives in "a
predictable place at the bottom of the screen"
([HEY Set Aside](https://www.hey.com/features/set-aside/)): the
destination is always visible, so the user can see where the thing went.

NN/g's guidance is to offer undo rather than confirm routine actions,
because people habituate to confirmations and click through them
([NN/g confirmation dialogs](https://www.nngroup.com/articles/confirmation-dialog/)).
mxr already follows this: `apps/web/src/features/mail-actions/verbFeedback.ts`
gives every verb a trigger, an optimistic change, a past-tense toast, a
sound and an undo path, and only irreversible verbs confirm.

For motion, NN/g puts simple feedback at about 100 ms and larger screen
changes at 200 to 300 ms, with 500 ms already a drag
([NN/g animation duration](https://www.nngroup.com/articles/animation-duration/)).
The View Transitions API lets a single-page app animate between two DOM
states with `document.startViewTransition()` and lets a named element
(`view-transition-name`) move on its own, which is what a shared-element
handoff needs; MDN warns about lost reading position and focus confusion
while old and new content coexist
([MDN View Transition API](https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API)).

### What fails

Today an item can only leave a place: Done, archive, snooze, sweep. The
toast says "Done. Press u to undo" (`12-done-toast-after.png`) and the row
vanishes. Nothing says where it went, because it went nowhere. A handoff
that looks like this would read as "deleted", which is the wrong feeling
for "passed to To do".

### Proposal: handoff verbs

| From | Key | Verb | What happens | Toast |
|---|---|---|---|---|
| Any mode | `e` | Done here | Mode done for this thread (`SetModeDone`); provider archive only if no other mode holds it (see section F) | "Done in Messages. Still in To do." or "Done. Archived." |
| Messages, Updates | `t` | To do | Creates a to-do prefilled from the ask and due words; a one-line editor opens inline, Enter accepts | "Added to To do: Sign lease renewal, due Wed 15 Oct. g x" |
| Messages | `t` after a reply | Reply, then To do | Composer's send menu gets "Send and add to To do" | as above |
| To do | `e` | Tick off | Todo done; the source email is filed in Archive (records facet) with no prompt | "Ticked off. Filed in Archive." |
| Reading | `b` | Later | Into Reading's later list | "Saved for later" |
| Any mode | `T` | Pass to... | A small menu of the other modes with their letters, for the less common moves (an Update that is really a person: Messages) | names the destination |
| Any mode | `u` | Undo | Existing daemon undo | existing |

Key check against `docs/reference/tui-keymap.json` and the web registry
(`apps/web/src/lib/actions/`): `t` and `T` are unused in every context.
`e` is Archive in list contexts and is already Done on the desk
(`apps/web/src/features/desk/deskDone.ts`, the `desk-done` entry in
`verbFeedback.ts`), so "done here" extends a habit mxr has already taught.
`b` is reply later today (`FlagReplyLater`), so "later" in Reading keeps the
same key with the same meaning: come back to this. `A` is Sweep all in the
place keymap (`SweepPlace`), so "let go of the day" in Updates reuses it with
its dry-run preview. `>` was considered for Pass to and rejected: it is
`MorePlaceSenders` in the place keymap. Per-sender correction keeps `K`
(`OpenSenderKindMenu`), with the modes as its choices. Each new verb gets a
`VERB_FEEDBACK` entry (`handoff-todo`, `mode-done`, `todo-done`) so it
can't ship without its toast, sound and undo.

Arrival, in three layers (judgement, timed per NN/g):

1. The toast names the destination and its jump key. The user learns the
   map from the toasts.
2. The destination's rail entry ticks its count up with a 120 ms scale
   (`--motion-duration-fast`).
3. In the destination, the item carries "Just now, from Messages" until
   the user next leaves that mode.

On Now, where both sections are on screen, the row moves from People to
Due soon with a View Transition (`view-transition-name: thread-<id>`),
180 ms (`--motion-duration-base`). Under `data-motion="reduced"`
(`apps/web/src/state/uiPrefsStore.ts`) there is no move: the row appears in
its new section highlighted. Across routes there is no shared-element
flight; the rail tick and toast carry it, because a flight across a route
change is the case MDN warns loses reading position.

Sound: `SoundEvent` in `apps/web/src/features/sound/player.ts` has `sent`,
`archived`, `snoozed` and `low_tide`. Add one, `handed`, for any handoff,
and a `ticked` for To do done. Judgement: one sound per kind of outcome,
not per mode, so the set stays small enough to learn.

## D. Navigation: Now plus five modes on desktop; five tabs on mobile with Updates inside Now

### What the guidance says

Apple: a compact tab bar shows at most five tabs, and beyond that the
fifth becomes More, which "makes it harder for people to reach and notice
content on tabs that are hidden"
([Apple HIG tab bars](https://developers.apple.com/design/human-interface-guidelines/components/navigation-and-search/tab-bars)).
Material 3: navigation bars are for three to five destinations
([M3 navigation bar](https://m3.material.io/components/navigation-bar/guidelines)).
NN/g: past five options a tab bar can't keep good touch targets, and a hub
works when people use one branch per session
([NN/g mobile navigation](https://www.nngroup.com/articles/mobile-navigation-patterns/)).
Linear's keyboard model is `g` then a letter (`G I` for Inbox)
([Linear docs](https://linear.app/docs/inbox)); mxr already uses the same
grammar (`g h`, `g i`, `g r`, `g p` in
`apps/web/src/lib/actions/navigationActions.ts`).

### What fails

The rail (`apps/web/src/components/Sidebar.tsx`) is Desk, Inbox, Reply
queue, Waiting on, Snoozed, Reading, Paper trail, Screener, then More,
Labels, Tools: eight top entries organised by mail mechanics. On mobile
(`15-mobile-desk-after.png`) it collapses to eight unlabelled icons in a
side strip, which is the case both Apple and Material advise against.

### Proposal

Desktop rail, seven entries in two groups:

```text
Now        g h   3       (home; badge = people owed + due soon)
──────────
Messages   g m
To do      g x   2       (badge = due soon only)
Updates    g u           (no badge: never work)
Reading    g r           (no badge)
Archive    g e           (no badge; opens on search)
──────────
Inbox      g i           (everything, arrival order)
More  >                  (Snoozed, Screener, Drafts, Sent, Labels, Tools)
```

Keys: `g h`, `g i` and `g r` keep their meaning. `g m`, `g x` and `g e` are
unused in both keymaps. `g e` pairs with `e` (done here files into
Archive). `g x` reads as the checkbox. `g t` stays Sent (Gmail's
convention, already shipped). `g u` today opens Subscriptions; Subscriptions
becomes Reading's manage view, so `g u` moves to Updates and the old
binding goes in `retiredAliases` as `g R` and `g P` did. `g p` (Paper
trail) redirects to Updates, as the blueprint says. `Mod+k` stays the
palette, and every mode and handoff verb is in it.

Mobile: five tabs, Now, Messages, To do, Reading, Find. Updates is not a
tab, because its rhythm is once a day and Now's digest card is its door.
Find is Archive plus search plus Inbox, since Archive is search
(Whittaker 2011). This keeps the bar inside the five-tab limit without a
More tab.

TUI: the sidebar (`crates/tui/src/ui/sidebar.rs`) lists the same seven with
the same `g` keys, enforced by `apps/web/src/lib/actions/keymapParity.ts`.

```text
 mxr . alex@demo
 ▸ Now          3
 ─────────────────
   Messages
   To do        2
   Updates
   Reading
   Archive
 ─────────────────
   Inbox
   More …
```

## E. Feel: one shell, five bodies

### Evidence on switching

Leroy found that when people move from one task to another, part of their
attention stays on the first ("attention residue"), and performance on the
next task drops, worse when the first was left unfinished
([Leroy 2009](https://doi.org/10.1016/j.obhdp.2009.04.002); summary from
the paper's abstract as known, not re-read for this note). Mark et al.
found self-interrupting to check mail went with higher reported
productivity than notification-driven checking
([Mark et al. 2016](https://doi.org/10.1145/2858036.2858262)). Together:
a mode switch should be a chosen, complete move, and a mode should let you
finish (the end states below), so you leave with less residue.

### Proposal

Judgement. The shell (rail, topbar, status bar, keys, toasts) is the same
everywhere, so the user always knows they are in mxr. The body differs in
density, type and what is up front:

| Mode | Row shape | Type | Density | Up front | End state |
|---|---|---|---|---|---|
| Messages | One row per person, chat-list style, last line of their message | Sans, names bold | Medium | Reply (`r`), done here (`e`) | "Nobody is waiting on you" |
| To do | Checkbox, task title, due chip, sender as small context | Sans, task title regular weight | Tight, like a checklist | Tick (`e`), schedule (`Z`), open source | "Nothing due before Mon" |
| Updates | One card per day, grouped by sender, one line each | Small, muted | Dense | Let go of the day (`A`), this needs me (`t`) | "Let go. Next digest tomorrow 08:00" |
| Reading | Article column, serif body, generous measure | Serif for body | Sparse | Later (`b`), unsubscribe (`D`) | none; reading is a feed, not a queue |
| Archive | Search field first, results as records (type, amount, date) | Mono for amounts and dates | Table | Find, open attachment | none |

Colour: one accent per mode, used only on the rail's active bar, the mode
header and the "also in" chip for that mode, so the chip in Messages
saying "Also in To do" carries To do's colour. Judgement: tinting the
whole page per mode would break the "same app" feeling and fight the
theme tokens in `apps/web/src/styles/tokens.css`.

## F. Should the provider archive happen when the last mode lets go? Yes, by default, and the toast must say so

### What users already expect

Three products, three models:

- Superhuman and Shortwave: Done is archive. Users of both accept that
  finishing an email removes it from the provider inbox
  ([Superhuman blog](https://blog.superhuman.com/inbox-zero-method/),
  [Shortwave](https://www.shortwave.com/docs/guides/bundles/)).
- HEY: no archive obligation at all. Mail slides into Previously Seen; "HEY
  eliminates the obligation to archive or delete"
  ([Moving from Gmail](https://www.hey.com/moving-from-gmail/)). HEY owns
  the mailbox, so there is no other client to disagree with.
- Gmail: categories never archive. A message in Promotions is still in the
  inbox, and archived mail leaves every category
  ([Gmail help](https://support.google.com/mail/answer/3055016)).

mxr sits on top of a Gmail or IMAP account the user also opens on a phone.
If mode done never archived, the provider inbox would grow without end and
the phone's Gmail would show everything mxr had already handled. If mode
done always archived at once, finishing the reply in Messages would hide a
still-open to-do from the phone. The blueprint's rule (archive when the
last mode lets go) is the Superhuman model applied per mode, which is the
one users of keyboard-first clients already hold.

### The risk is surprise, not loss

The mail is never deleted, `u` undoes, and Inbox (everything) shows mail
some mode still holds. The real risk is a user who presses `e` in Messages
expecting the email to leave Gmail, and it doesn't because To do holds it,
or the reverse. Evidence from NN/g says the fix is feedback and undo, not a
confirm dialog
([NN/g](https://www.nngroup.com/articles/confirmation-dialog/)).

### Proposal

- Default on: provider archive when the last mode lets go.
- The toast always says which happened: "Done in Messages. Still in To do
  (due Wed)." or "Done. Archived in Gmail." The difference is visible every
  time, so the user learns the rule from use.
- Inbox shows a "held by To do" chip on mail a mode holds, so the
  everything view explains why a handled email is still there.
- A setting, `modes.archive_on_last_done` (default `true`), for people who
  want archive to stay a separate verb; with it off, `e` hides the item in
  the mode only and archive is the palette's "Archive in Gmail".
- Batch let-go (a day of Updates) keeps the existing dry-run preview
  (`13-sweep-preview-after.png`), listing what will be archived and what
  stays because another mode holds it.

## G. The Screener should fold into the modes, with a page only for history

### Evidence on HEY's Screener in practice

The evidence is thin and anecdotal, but consistent:

- After two months, a HEY user saw new senders fall to "a maximum of three
  or four per week", many of them already-screened-out senders on a new
  address
  ([The Sweet Setup](https://thesweetsetup.com/hey-email-two-months-with-the-new-email-service/)).
  The queue is heavy at the start and light later.
- The known cost is a legitimate first email held until the user checks
  the Screener; one review describes a client's new project manager
  waiting six hours and calls it "the price the design refuses to
  negotiate"
  ([aiemaily review](https://aiemaily.com/blog/hey-email-review-2026); weak
  source, the scenario reads as illustrative rather than reported).
- HEY's own pages describe the Screener as a gate ("decide if you want to
  hear from them again") and mention no way to turn it off
  ([HEY features](https://www.hey.com/features/),
  [HEY Screener](https://www.hey.com/features/the-screener/)).
- A user who left HEY found the Imbox, Feed and Paper Trail "simply not
  enough places to sort mail" and couldn't add more
  ([Night Water](https://www.nightwater.email/hey-email-review-fastmail/)).
  This is about destinations, not the Screener, but it shows the cost of a
  fixed sorting step with fixed outcomes.

### What fails in mxr today

The Screener is a rail entry with a badge of 27 (`09-screener-after.png`)
because `list_screener_queue` lists every inbound sender without a
decision, including Maya Ortiz with four messages in a live thread. The
buttons are mail mechanics (Allow, Deny, Feed, Paper trail), not the modes.
And unlike HEY, mxr doesn't withhold the mail, so the Screener is a second
copy of a decision the classifier already made.

### Proposal

- No Screener on the rail. A first-time sender's mail goes straight to its
  best-guess mode with the blueprint's reason line, plus one inline
  question on the row: "New sender. Keep in Updates? [Messages] [To do
  never] [Block]". Answering writes the per-sender correction.
- Anyone you've written to is never asked (the fix in flight on another
  branch).
- Now mentions a person only when a probably-human first-time sender
  wrote and the classifier is unsure: "Screener asks about 2 people" in
  the "Not now" line, not a count of every sender.
- The `/screener` page stays under More as decision history and bulk
  review, keeping `g S`.

This keeps HEY's benefit (you decide once per sender) without its cost
(first mail delayed), because nothing is held back.

## How one email looks in each mode

The email:

> From: Sam Okafor (your landlord) . Thu 2 Oct 09:14
> Subject: Lease renewal
> Hi, the renewal for next year is attached, please sign and send it back
> by 15 Oct. Also, are you around Thursday? The boiler engineer wants to
> come by in the morning.
> Attachment: lease-renewal-2026.pdf

It holds three aspects: a conversation (Thursday?), a task (sign by 15
Oct) and a record (the lease). Phase 1 rules find the task from "sign" and
"by 15 Oct". The blueprint's lead time for sign is two days, so the to-do
appears in To do at once and on Now from Mon 13 Oct 09:00.

Messages: the person, and the question waiting on you.

```text
 Sam Okafor                                          09:14
 SO  landlord . you've written 12 times, last in August
     "...are you around Thursday? The boiler engineer
      wants to come by in the morning."
     Also in To do: sign lease renewal, due Wed 15 Oct
     [r Reply]  [t To do]  [e Done here]
```

To do: the task, with the person as context.

```text
 DUE THIS WEEK
 [ ] Sign lease renewal              due Wed 15 Oct
     SO Sam Okafor . Lease renewal . lease-renewal-2026.pdf
     Here because: asks you to sign, "by 15 Oct" (rule)
     [e Tick off]  [Z Schedule]  [Enter Open email]
```

Archive, after the to-do is ticked off and the reply sent:

```text
 / lease
 RECORD                         FROM        DATE        FILE
 Lease renewal 2026-27          Sam Okafor  2 Oct 2026  lease-renewal-2026.pdf
   Signed, to-do done 14 Oct . thread: Lease renewal (4)
```

The flow: BK replies "Thursday works" from Messages and presses `e`. The
toast says "Done in Messages. Still in To do (due Wed 15 Oct)." Nothing
leaves Gmail. On Tuesday 14 Oct he signs, sends, and presses `e` on the
to-do. The toast says "Ticked off. Filed in Archive. Archived in Gmail."
Next April, `/ lease` finds the record.

## Open questions

- Is the Reading pick on Now worth its slot, or does it pull a "not now"
  mode into the "now" view? The evidence is silent; BK's dogfooding should
  decide.
- Should Now's per-section cap be three, or fewer on mobile?
- Is `x` for To do (`g x`) guessable enough, or is the mnemonic too weak
  next to `g t` for Sent?
- Does folding Updates into Now on mobile hide it from people who want to
  browse a past day?
- Should "Done here" in Messages offer to make a to-do when the classifier
  found an ask and none exists yet, or is that one prompt too many?

## Risks

- The multi-mode model is untested in the literature. Taskmaster's thrasks
  are the nearest study, and its evaluation was small. If users see the
  same email in two modes as a duplicate, the "also in" line has to carry
  more weight than designed.
- Classifier errors are what users see first. The desk already shows a
  sign-in alert as a person owed and couriers as replies owed. A Now view
  capped at three items amplifies each error, because a wrong item takes a
  third of the section.
- Archive on last let-go depends on mode membership being right. A
  missed aspect means mail leaves Gmail early. The toast and undo cover
  the single case; the batch case relies on the dry-run preview.
- Per-mode accents and sounds could become decoration. Keep them to the
  rail bar, header and chip, and one sound per outcome.

## Sources

Sources marked (second-hand) were read through search-result summaries, not fetched; the rest were fetched for this note. Leroy 2009 is from memory of the abstract, with only its DOI resolved.

Research:

- Mackay, W. (1988). More than just a communication system. CSCW '88. https://dl.acm.org/doi/10.1145/62266.62293 (second-hand)
- Whittaker, S. and Sidner, C. (1996). Email overload. CHI '96. https://dl.acm.org/doi/pdf/10.1145/238386.238530 (second-hand)
- Bellotti, V., Ducheneaut, N., Howard, M. and Smith, I. (2003). Taking email to task. CHI '03. http://www.chi2003.org/docs/takingemail.pdf (second-hand)
- Whittaker, S. et al. (2011). Am I wasting my time organizing email? CHI '11. https://research.ibm.com/publications/am-i-wasting-my-time-organizing-email-a-study-of-email-refinding (second-hand)
- Mark, G., Iqbal, S., Czerwinski, M., Johns, P. and Sano, A. (2016). Email duration, batching and self-interruption. CHI '16. https://doi.org/10.1145/2858036.2858262
- Cowan, N. (2001). The magical number 4 in short-term memory. Behavioral and Brain Sciences 24(1). https://philpapers.org/rec/COWTMN (second-hand)
- Leroy, S. (2009). Why is it so hard to do my work? Attention residue. OBHDP. https://doi.org/10.1016/j.obhdp.2009.04.002 (second-hand)
- Aberdeen, D., Pacovsky, O. and Slater, A. (2010). The learning behind Gmail Priority Inbox. https://research.google/pubs/pub36955/ (ranking by predicted action; background, not cited for a specific claim above) (second-hand)

Design guidance:

- Prater, S. Object-Oriented UX. A List Apart. https://alistapart.com/article/object-oriented-ux/
- NN/g. Basic patterns for mobile navigation. https://www.nngroup.com/articles/mobile-navigation-patterns/
- NN/g. Animation duration. https://www.nngroup.com/articles/animation-duration/
- NN/g. Confirmation dialogs. https://www.nngroup.com/articles/confirmation-dialog/
- Apple HIG. Tab bars. https://developers.apple.com/design/human-interface-guidelines/components/navigation-and-search/tab-bars (second-hand)
- Material 3. Navigation bar. https://m3.material.io/components/navigation-bar/guidelines (second-hand)
- MDN. View Transition API. https://developer.mozilla.org/en-US/docs/Web/API/View_Transition_API
- Google. Gmail markup actions. https://developers.google.com/gmail/markup/actions/actions-overview

Products:

- Things support: Today, Upcoming, Anytime, Someday. https://culturedcode.com/things/support/articles/4001304/
- Sunsama manual: daily planning. https://help.sunsama.com/docs/usage-guides/daily-planning/ (second-hand)
- HEY features. https://www.hey.com/features/
- HEY Set Aside. https://www.hey.com/features/set-aside/
- HEY Screener. https://www.hey.com/features/the-screener/
- HEY: Moving from Gmail. https://www.hey.com/moving-from-gmail/
- Superhuman: Get Me To Zero. https://new.superhuman.com/get-me-to-zero-260284 (second-hand)
- Superhuman blog: inbox zero method. https://blog.superhuman.com/inbox-zero-method/
- Shortwave: bundles. https://www.shortwave.com/docs/guides/bundles/
- Gmail help: inbox categories. https://support.google.com/mail/answer/3055016
- Linear docs: Inbox. https://linear.app/docs/inbox

Reviews:

- The Sweet Setup: HEY two months in. https://thesweetsetup.com/hey-email-two-months-with-the-new-email-service/
- Night Water: leaving HEY for Fastmail. https://www.nightwater.email/hey-email-review-fastmail/
- aiemaily: HEY review 2026 (weak source). https://aiemaily.com/blog/hey-email-review-2026
