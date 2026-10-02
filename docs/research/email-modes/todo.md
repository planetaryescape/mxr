# To do: email shown as the thing you have to do, not the thing you received

Research for the To do mode in [22-email-modes.md](../../blueprint/22-email-modes.md).
Code references are at `bdf997c4` on `docs/email-modes` in
`/tmp/mxr-feel-clones/modes`. Claims marked "evidence" come from a cited
source; claims marked "judgement" are this proposal's reasoning and need
dogfooding to confirm.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## 1. The job is to never miss a consequence, at the lowest attention cost

The To do mode holds admin with an outside consequence: a late fee, an
expired passport, a lapsed return window, a host who planned without you.
Success feels like this:

- You never learn about a deadline on the day it bites. It shows up while
  there is still time to act at ease.
- Each item reads as an instruction you could act on without opening the
  email: "Pay Thames Water £48.20, act by Wed".
- Doing it is one step from the row. Paying opens the payee's page, RSVP
  answers the invite, "send the signed form" starts the reply.
- When it's done, it leaves, and the email goes where it belongs (Archive).
- When nothing needs you, the screen says so and says when the next thing
  will, so calm is informed rather than suspicious.
- A missed date gets a neutral "was due Tue", never a red pile.

Failure feels like a second inbox: a list of subjects with flags that you
still have to open, read and decode.

## 2. The best task apps separate "when you'll act" from "when it's due" and put the verb first

### Start date and deadline are two different dates, and the best apps keep both

Evidence. Things 3 gives every to-do a "When" (the day you plan to start)
and an optional deadline. A When date "cannot ever be overdue since it's a
scheduling mechanism", while deadlines can. To-dos with a future When date
"hibernate in Upcoming" and move to Today on the day. Things advises
combining them for work that takes time: "add a start date several days
before the deadline"
([Things: Scheduling](https://culturedcode.com/things/support/articles/2803579/),
[Things: Today, Upcoming, Anytime, Someday](https://culturedcode.com/things/support/articles/4001304/)).
Things also notes deadlines "are tied to external pressure, and missing
them could incur negative consequences", which is exactly the To do mode's
material.

Evidence. Todoist added deadlines as a separate field in January 2025 and
renamed "Due date" to "Date" because "the Date you set for a task is the
date you plan to do it. A Deadline, on the other hand, is the date a task
needs to be completed by." Within 7 days of a deadline "a countdown will
show to give you a subtle reminder"
([Todoist: Add deadlines to tasks](https://www.todoist.com/help/todoist/product-updates/add-deadlines-to-tasks-jan-7-2025-WFbv37eEw)).

Evidence. OmniFocus has the same split as defer (greyed out until then)
and due, and its Forecast perspective lays both out by day beside the
calendar, with drag to re-date
([OmniFocus manual: dates](https://support.omnigroup.com/documentation/omnifocus/ios/2.22/en/using-dates-and-times/),
[OmniFocus 4 perspectives](https://support.omnigroup.com/documentation/omnifocus/universal/4.3.3/en/perspectives/)).

What this means for mxr: a to-do derived from email has three dates, not
one. `due_at` is the outside deadline. `surface_at` is the When date the
system picks (deadline minus lead time). `scheduled_for` is the When date
the user picks. The blueprint's `todos` table already has all three; the
view must show the act-by date first and the deadline second.

### A glanceable "runway" beats a date for judging urgency

Evidence. In Bellotti et al.'s Taskmaster study, the most popular feature
(4.4 of 5) was the warning bar: a bar per task, green for time left in a
two-week window, red for time used. Users: "Seeing the growing red, just
having an idea at a glance"; "I could see sort of the slack time that I
have"; "to walk in, in the morning and to see the change over time and be
able to know what was due Monday morning, versus Tuesday morning"
([Bellotti et al., CHI 2003](http://www.chi2003.org/docs/takingemail.pdf)).

### A marker that says "something to do" without saying what fails

Evidence. In the same study, action balls (a dot meaning "there is an
action here") "gave a sense of to-do-ness" but "did not help much in
planning one's work. One still had to examine the contents of a thrask to
find out what to do." Users asked to write a few words of what to do,
which the authors thought "might have been a better aggregation"
([Bellotti et al.](http://www.chi2003.org/docs/takingemail.pdf)). This is
the case against a flag, a star or a "To do" label on an email row.

Evidence. GTD defines a next action as "the next physical, visible
activity" that moves something forward; "think about" doesn't count
([43 Folders on Allen's definition](http://www.43folders.com/2004/09/17/next-actions-both-physical-and-visible),
[super-productivity summary](https://super-productivity.com/blog/gtd-next-actions-guide/)).
The title of a to-do must be a verb and an object: "Renew car insurance",
not "Your policy documents".

### Show the payee, the amount, the date and a way to pay, and nothing else

Evidence. Google Now's bill cards (2014, built from Gmail) showed "the name
of a biller as well as the amount due, the due date, and an option to view
the original billing email", and sometimes a "Pay now" link to the biller's
site
([Pocket-lint](https://www.pocket-lint.com/apps/news/google/128940-google-now-adds-bill-reminder-cards-pulled-from-gmail-so-you-can-pay-bills-on-time/),
[Search Engine Land](https://searchengineland.com/google-now-reminds-pay-bills-205379)).
The source email is a link, not the content. That is the right anatomy for
a bill row.

### Triage is a short, keyed decision, and snooze returns on time or on news

Evidence. Linear's Triage inbox offers four decisions with single keys:
accept (1), decline (2), duplicate (3) and snooze (H). Snooze hides the
issue "to return at a time of your choosing, or when there's new activity
on that issue: whichever comes first"
([Linear: Triage](https://linear.app/docs/triage)). For mxr: a scheduled
to-do should come back early when a new email lands in its thread ("final
reminder", "payment failed").

### Pulling email into a plan works as a short daily ritual, not a constant stream

Evidence. Sunsama's morning ritual shows flagged Gmail as candidates in an
import panel; the user drags what they commit to into today and skips the
rest
([Sunsama](https://www.sunsama.com/daily-planning),
[review](https://calmevo.com/how-to-use-sunsama/)). HEY's "Reply Later"
and "Focus & Reply" do the same for replies: set aside now, clear as a
batch later ([HEY features](https://www.hey.com/features/)). mxr already
has Focus & reply (`apps/web/src/routes/focus.tsx`). To do needs the same
batch shape for admin: "Do the 3 things due this week".

### Missed dates need a one-step way back, without scolding

Evidence. Todoist puts a "Reschedule" button on its Overdue section, and
its own writing names the problem: overdue counts "tick up to panic-inducing
levels" and users avoid the app "out of guilt and anxiety"
([Todoist: GTD tips](https://www.todoist.com/inspiration/gtd-tips)). mxr
already decided no scolding copy and no red (D101, and the "was due Tue"
rule in the blueprint).

### Separating "mine" from "someone else's" helps users see who must move

Evidence. Taskmaster drew actions as red or blue balls, for yourself or
for another. A user: "it's helpful for me to quickly look and go oh I'm
the one that needs to do something here [...] OK this is here because I'm
waiting for someone else." Another: "I would have forgotten ever to reply
to that, but it's still flagged, it's overdue [...] it means I owe
something to somebody"
([Bellotti et al.](http://www.chi2003.org/docs/takingemail.pdf)). Microsoft's
Viva Briefing email splits detected items into Commitments (you promised),
Requests (someone asked you) and Follow-ups (you asked someone)
([Microsoft Support](https://support.microsoft.com/en-gb/topic/how-cortana-helps-you-in-briefing-email-from-microsoft-viva-478c90f6-a9b4-3c24-6a63-ff85426611a5)).

## 3. Email-to-task tools fail when the task is just the email with a checkbox

### The subject line becomes the task title, which says nothing

Evidence. Microsoft To Do's flagged-email list names each task after "the
subject of the flagged message" with a body preview
([Microsoft Support](https://support.microsoft.com/en-us/office/using-microsoft-to-do-with-flagged-email-from-outlook-f90c37b0-4453-4756-a6d5-e2ef8d33b395)).
Subjects are written by senders to get opened ("Your October statement is
ready"), not to tell you what to do. This is the action-ball failure again.

### The task loses its way back to the email

Evidence. A recurring complaint about Gmail-to-task integrations is the
missing link back to the original email, which makes it hard to find and
reply once the task is done
([Zoho community](https://help.zoho.com/portal/zh/community/topic/gmail-extension-really-needs-a-link-back-to-the-original-email),
[GQueues group](https://groups.google.com/g/gqueues/c/Cc4Tm8mflOY)). mxr
avoids this structurally: the to-do keeps `thread_id` and
`source_message_id` and never leaves the app.

### Tasks drift out of sight in a stream of new mail

Evidence. Bellotti's participants spent about 10% of email time filing
messages for future work and about 8% scrolling and inspecting folders for
active threads, and the worst cost was "when a to-do has drifted out of
sight and has not been acted upon"
([Bellotti et al.](http://www.chi2003.org/docs/takingemail.pdf)). Users
defer often: 12% of triage sessions in a 40,000-user log study had a
deferred email, more so when handling needs a reply, careful reading or
clicking links; users improvise with mark-as-unread and flags
([Sarrafzadeh et al., WSDM 2019](https://arxiv.org/abs/1901.04375)). These
are the people who need deferral to be a first-class state with a return
time, not an unread dot.

### Guessed dates that are wrong get switched off

Evidence. Todoist's Smart Schedule, an ML feature that suggested dates from
habits, was withdrawn; Todoist said "the primary feedback was that our
Smart Schedule algorithm was not accurate enough to be helpful for users"
([Todoist on X](https://x.com/todoist/status/1260030543556603904),
[MacStories on launch](https://www.macstories.net/news/todoist-launches-smart-schedule-an-ai-based-feature-to-reschedule-overdue-tasks/)).
Judgement: a model-invented date in a deadline view is worse than no date.
The blueprint's rule already covers this: the model copies the due words,
the daemon checks they appear verbatim, and `natural_time` resolves them.

### Bills you don't have to pay are noise

Evidence. Monzo treats direct debits and standing orders as scheduled
payments the bank moves on its own, shown under Payments > Scheduled, not
as things to do
([Monzo Community](https://community.monzo.com/t/pay-day-salary-sorter-pots-direct-debits/135249),
[Monzo blog](https://monzo.com/blog/2019/09/26/introducing-salary-sorter-and-bills-pots)).
Judgement: a bill email that says "will be collected by direct debit on 9
Oct" or "your card will be charged" is Updates (glance and let go), not To
do. Google Now showed every bill it understood; mxr should not.

### Extractors that can't say why lose trust

Evidence (vendor claim, weak). Vendors of AI task extractors report false
positives fall as dismissals teach the system and that showing "which
sentence triggered the detection" builds trust
([Alfred](https://get-alfred.ai/blog/ai-that-extracts-tasks-from-emails-automatically)).
This matches D097 and D111 (every item says why). I found no independent
study of email-task extraction precision; mxr's `mxr modes eval` (phase 5)
would be the first real number on BK's mail.

## 4. The view needs ten fields per to-do, and mxr already derives half of them

| Field | Shown as | How it's derived | In mxr today | Needed |
|---|---|---|---|---|
| verb | first word of the title, icon | Admin-verb lexicon over subject and first lines; schema.org type (`Invoice` is pay, `EventReservation` with no reply is rsvp); model fallback | `verb` column designed in `todos` (blueprint) | Detector `todo_detect`, modelled on `crates/deliveries/src/heuristics.rs` |
| object | rest of the title ("car insurance") | Noun phrase after the verb; payee for pay; invite summary for rsvp | Nothing | Detector; model fallback copies words verbatim |
| counterparty | "Thames Water", or Maya's face | Organisation from display name or domain; a person when sender kind is Person (`mail_kind::classify`) | `mail_kind.rs`, `contacts` | Join only |
| amount | "£48.20" | Currency regex near "due", "total", "amount"; schema.org `Invoice.totalPaymentDue` | Nothing (`crates/deliveries/src/schema_org.rs` parses only `ParcelDelivery`/`Order`) | Extend the schema.org reader to `Invoice`; amount regex |
| due_at + due_words | "due Fri 9 Oct" | Due phrase verbatim, resolved by `mxr_core::natural_time::resolve_time` in the user's zone | Parser in `crates/core/src/natural_time/` handles "oct 3", "3rd october 2027", "oct 3, 2025", weekdays, ISO dates | Numeric dates ("09/10/2026") are not parsed and are locale-ambiguous; read schema.org dates first, then the account's locale |
| act_by (`surface_at`) | "act by Wed" | `due_at` minus lead time per verb (section 5); for a return, `delivered_at + window - lead` | `056_reply_later_due.sql` claim pattern; `process_due_timers` | Lead-time table and working-day arithmetic |
| action | the primary button | Pay/renew/verify: the body link whose anchor text or nearby words match the verb, on the sender's domain; rsvp: the invite; reply: compose | Invites in `calendar_invites` (`038`) with RSVP sending in `handler/mailbox.rs`; links counted with tracker filtering in `crates/core/src/types.rs`; web linkifies in `MessageText.tsx` | A daemon-side "action link" picker that returns one URL plus its domain |
| trust | quiet tick, or "check this sender" | `Authentication-Results` DMARC/SPF pass plus prior mail from the domain | `auth_results` captured in `crates/mail-parse/src/lib.rs` (`MessageMetadata`) | Parse pass/fail; gate the one-click button on it |
| reason + origin | "Here because: 'payment due 9 Oct' (rule)" | Matched phrase and layer | `reason`, `origin`, `model` columns designed | Detector fills them |
| completion signal | "Paid, receipt 3 Oct" | A later message in the same thread or from the same domain that matches the to-do (section 5) | Deliveries already resolve on "delivered" in `crates/deliveries/src/lifecycle.rs` (`merge_into`) | Matcher per verb |

Promises come from `contact_commitments` (`024_contact_commitments.sql`,
`handler/promises.rs`) with `what`, `by_when` and `email`. They map onto
the same row shape with the person as counterparty and no action link.

Return windows can be computed rather than read: `deliveries.delivered_at`
plus the merchant's window (14 days by default in the UK; see section 6)
gives a "Return by" to-do only when the user asks for it ("I might return
this", one key on the delivery). Judgement: auto-creating a return to-do for
every parcel would flood the list.

## 5. The proposed To do view: a runway, not a list

### The page is organised by when you must act, in four bands

Top to bottom:

1. A one-line headline in the desk's voice, saying the count and the first
   thing: "3 things need you this week. Council tax first, act by Wed."
2. Now: `surface_at <= now` and not done, sorted by act-by date. Each
   row has its runway bar.
3. Coming up: open to-dos whose surface time is in the next 30 days,
   grouped by week, dimmed, with "shows up Mon 12" instead of a button.
   This is Things' Upcoming and OmniFocus' Forecast.
4. Whenever: no deadline. Collapsed to a count by default.

Scheduled to-dos sit in Coming up at their scheduled day, still showing
their deadline. Done to-dos go to a "Done this week" line at the bottom
(Things' Logbook), one line, expandable.

### Each row is a sentence with one button

Row anatomy, left to right: verb icon, title as verb plus object, the
counterparty, the amount, a runway bar, "act by Wed · due Fri 9", and one
primary button labelled with the verb and destination. Under the title, in
small type, the source: sender, received date, and the reason. The email
body is not on the row. Enter on the row does the primary action; `o`
opens the email.

### Desktop web

```text
+--------+-------------------------------------------------------------------------------+
| Now    |  To do                                                       [ Do this week ] |
| Msgs   |  3 things need you this week. Council tax first, act by Wed.                  |
|>To do 3|                                                                               |
| Updates|  NOW                                                                          |
| Reading|  £  Pay council tax            Camden Council   £142.00                       |
| Archive|     [######----]  act by Wed 7 · due Fri 9      [ Pay on camden.gov.uk  ↵ ]   |
|        |     from Camden Council, 28 Sep · Here because: "payment due 9 October"       |
| Inbox  |                                                                               |
|        |  ✎  Send the signed engagement form   Priya Shah (you promised)               |
|        |     [########--]  act by Thu 8 · due Fri 9      [ Reply to Priya        ↵ ]   |
|        |     you wrote "I'll send it back Friday", 30 Sep                              |
|        |                                                                               |
|        |  ✓  Verify your new sign-in email   Octopus Energy                            |
|        |     link expires today 18:00                    [ Verify on octopus...  ↵ ]   |
|        |                                                                               |
|        |  COMING UP                                                                    |
|        |  wk of 12 Oct  RSVP Sam's leaving drinks           shows up Mon 12 · Thu 15   |
|        |  wk of 19 Oct  Renew car insurance  Admiral £412   shows up Mon 19 · due 26   |
|        |  Nov           Renew passport                      shows up 14 Nov · exp 23 Jan|
|        |                                                                               |
|        |  WHENEVER  2 ›                     DONE THIS WEEK  4 ›                        |
|        |                                                                               |
|        |  ↵ do it   e done   Z schedule   , edit   x not a to-do   o open email        |
+--------+-------------------------------------------------------------------------------+
```

The runway bar is Taskmaster's warning bar adapted to lead time: it fills
from the surface date to the deadline, so a fresh item is mostly empty and
one past its act-by date is full. One colour, the accent; no red, ever.
Past the deadline the bar is replaced by "was due Fri".

Selecting a row opens a side panel, not the reader: the extracted fields as
editable chips (title, amount, act-by, due), each with "from: 'payment due
9 October'" under it, the action link with its full domain, then the source
email collapsed to three lines with "Open email".

### Mobile

```text
+------------------------------------+
| To do                         3 now|
| Council tax first, act by Wed.     |
|------------------------------------|
| £ Pay council tax          £142.00 |
|   Camden Council                   |
|   [######----] act by Wed · Fri 9  |
|   [ Pay on camden.gov.uk ]         |
|------------------------------------|
| ✎ Send the signed form             |
|   Priya Shah · you promised        |
|   [########--] act by Thu · Fri 9  |
|   [ Reply to Priya ]               |
|------------------------------------|
| Coming up                        5›|
| Whenever                         2›|
+------------------------------------+
 swipe right: done   swipe left: schedule
```

The button spans the card width so the thumb hits the action, not the row.
Swipe right is done, swipe left opens the schedule sheet with three
suggested times and the natural-time field.

### TUI

```text
 To do ─ 3 need you this week. Council tax first, act by Wed.
 NOW
 > £ Pay council tax             Camden Council   £142.00  ▓▓▓▓▓▓░░░░ act Wed 7  due Fri 9
   ✎ Send the signed form        Priya Shah (promised)     ▓▓▓▓▓▓▓▓░░ act Thu 8  due Fri 9
   ✓ Verify sign-in email        Octopus Energy            expires today 18:00
 COMING UP
   RSVP Sam's leaving drinks     shows Mon 12   due Thu 15
   Renew car insurance  £412     shows Mon 19   due Mon 26
 WHENEVER (2)   DONE THIS WEEK (4)
 ─────────────────────────────────────────────────────────────────────────────────
 camden.gov.uk/pay · DMARC pass · Here because: "payment due 9 October" (rule)
 ↵ open link  e done  Z schedule  , edit  x not a to-do  o email  ? keys
```

The footer shows the selected row's action domain and trust line, so the
terminal user sees where Enter goes before pressing it. Enter on a link
opens it with the system opener, as the TUI's links view already does.

### Six keys cover the job, and they match mxr's existing verbs

| Key | Action | Why this key |
|---|---|---|
| Enter | Do it: open the action link, start the reply, or open the RSVP | The primary verb, the same as "open" elsewhere |
| `e` | Done (tick off), with undo | `e` is done everywhere in mxr (archive, focus "done, no reply needed") |
| `Z` | Schedule: natural-time field with preview (D096) | `Z` is snooze in the bulk bar and focus mode |
| `,` | Edit the fields (title, amount, act-by, due) | Free key; a light edit, not a mode |
| `x` / `X` | Not a to-do, for this email / for this sender | Correction per message and per sender (D111); `x` is "screened out" in the place picker, the nearest meaning |
| `o` | Open the source email | Leaves To do for the reader |
| `t` (from Messages, Updates) | Make a to-do from this email | Already in the blueprint's handoff |

Batch: `Do this week` (and `g F` parity with focus mode) steps through the
Now band one at a time, with Enter to act and `e` to tick, like Focus &
reply. Bulk done on a selection goes through the preview path with a dry
run, as AGENTS.md requires for batch mutations.

### The rhythm is once a morning, plus a weekly look ahead

- Judgement, following Sunsama's ritual and the desk's role (D110): to-dos
  surface once, at the start of working hours on their surface day, via
  the existing `surfaced_at` claim. The desk's "Due soon" lane shows them;
  To do itself shows them as soon as they're detected.
- No per-item push notifications except for verify/confirm links that
  expire within hours, which surface at once (blueprint table).
- On Mondays the headline changes to the week: "This week: 3 to do, £554
  out." Judgement: the weekly total helps bills feel like a plan, not a
  surprise; it costs nothing because amounts are already extracted.

### The empty state says when the next thing arrives

```text
 Nothing needs you.
 Next: Renew car insurance shows up Mon 19 Oct.
```

When there's nothing scheduled at all: "Nothing needs you, and nothing is
coming up." No illustration, no confetti; the one earned moment is low
tide on the desk (D101).

### An email enters To do in four ways and leaves in three

Enters:

1. Detected by rules in `post_sync_fanout` (`crates/daemon/src/loops.rs`),
   like deliveries. Lands directly in To do with its reason.
2. Detected by the user's model (phase 5) for what rules shortlist.
   Same row, with "(local model)" in the reason and a quiet "check" style
   on the date chip until the user acts on or edits it once.
3. Handed off from Messages with `t` after replying, prefilled from
   the ask and due words, and from Updates with "this needs me". The
   handed-off row says "from your conversation with Priya".
4. Your promises, from `contact_commitments` when recorded through
   `RecordPromise`.

A new message in the thread updates the row (dedup key) and, if it was
scheduled, brings it back early, as Linear's snooze does.

Leaves:

1. Done by you (`e`). The row strikes through, collapses, and an inline
   chip offers "File in Archive" (the blueprint's handoff). Accepting is
   one key; ignoring it leaves the email where it is.
2. Done by the world. When a confirmation arrives, the row changes to
   "Looks done: payment received 3 Oct" with the confirming email linked.
   Strong matches complete on their own with a visible undo for a day;
   weak ones wait for one key. Rules, by verb (judgement, to be measured):
   - pay: same sender domain and the same amount or reference number, and
     a receipt word ("received", "thank you for your payment").
   - renew: same domain, "renewed", "your new policy", "confirmation".
   - rsvp: the user sent an RSVP (`calendar_invites.current_partstat` is
     set). This one is exact, so it always auto-completes.
   - verify/confirm: a "verified" or "welcome" message from the same
     domain within a day.
   - promise: a message sent to that person in the thread after the
     promise, with an attachment when the promise named a file. Always an
     offer, never automatic, because "sent something" isn't "kept the
     promise".
   This follows the deliveries pattern, where "delivered" resolves the row
   in `lifecycle::merge_into` and the row and its provenance are kept.
3. Dismissed (`x`): not a to-do. Kept as `dismissed` for the
   correction record, never shown again for that email.

### Overdue reads as a fact with a way forward

- Past act-by but before the deadline: the bar is full and the date reads
  "act by Wed (yesterday)". Nothing else changes.
- Past the deadline: "was due Fri". The item stays in Now, sorted after
  items still in time, because a late fee is usually already incurred and
  the remaining job is damage control.
- When two or more slip, one line above the band: "2 slipped. Do them,
  reschedule, or let them go." Reschedule offers one date for all of them,
  as Todoist's overdue button does. No count badge on the rail for slipped
  items; the rail badge counts only Now.

### Small motion and sound earn their place in two moments

- Tick off: a 150 ms strike-through, then the row collapses over 200 ms;
  the next row takes focus. Under reduced motion it disappears at once.
  Held `e` never animates (D101).
- The chime (off by default, D101) plays once per item ticked off by hand,
  not for auto-completions, so the sound means "you did that".
- The runway bar animates only once a day, when the page first opens that
  morning, from yesterday's fill to today's, which is Taskmaster's "see
  the change over time" made literal. Never on navigation.
- Returning from an action link (the window regains focus within 10
  minutes of Enter), the row shows an inline "Done?" with `e`/Esc. This is
  the "do it now" loop for links, where mxr can't see the payment.

### Calendar stays optional and one-way

Evidence: Bellotti's users copied deadlines into a calendar by hand and the
authors expected calendar integration to raise the value of task metadata
([Bellotti et al.](http://www.chi2003.org/docs/takingemail.pdf)). Judgement:
offer "Add to calendar" only for `book` to-dos that end in an appointment,
and an opt-in local `.ics` feed of act-by dates for people who live in a
calendar. Deadlines as all-day events clutter a calendar, so it is off by
default. mxr already parses and replies to invites, which covers RSVP
without any calendar sync.

## 6. Lead times should be a fixed table per kind of action, computed from how long the outside world takes

### The lead time is the action's processing time, not the user's habit

Evidence for each type:

- Bill paid by bank transfer. UK Bacs takes three working days, and
  "weekends and public holidays are not included"; Faster Payments are
  near-instant
  ([Access PaySuite](https://www.accesspaysuite.com/blog/how-long-do-bacs-payments-take/),
  [IRIS](https://www.iris.co.uk/blog/payroll/whats-the-difference-between-bacs-and-faster-payments/)).
  Card payments on a biller's site are immediate.
- Passport. HM Passport Office's target is three weeks, but it has
  advised allowing up to 10 weeks since 2021
  ([Lonely Planet](https://www.lonelyplanet.com/articles/when-to-renew-your-uk-passport-as-wait-times-increase),
  [Wego](https://blog.wego.com/uk-passport-renewal-delays/)). US routine
  processing is 4 to 6 weeks, plus up to 2 weeks each way in the post
  ([US State Department](https://travel.state.gov/content/travel/en/passports/how-apply/processing-times.html)).
  Many countries require six months' validity on entry
  ([same source](https://travel.state.gov/content/travel/en/passports/how-apply/processing-times.html)),
  so the real deadline can be the next trip, not the expiry date.
- Lease. In England, under the Renters' Rights Act in force from 1 May
  2026, a tenant ends a periodic tenancy with at least two months' written
  notice
  ([NRLA](https://www.nrla.org.uk/resources/renters-rights/ending-a-periodic-assured-tenancy),
  [Citizens Advice](https://www.citizensadvice.org.uk/housing/ending-a-private-tenancy/ending-your-tenancy/)).
  The act-by date is the end date minus the notice period, not the end
  date.
- Return window. UK distance-selling rules give 14 days to cancel from
  the day after delivery, then 14 days to send the goods back
  ([BIS guidance](https://assets.publishing.service.gov.uk/media/5a817b92ed915d74e33fe73a/bis-13-1368-consumer-contracts-information-cancellation-and-additional-payments-regulations-guidance.pdf)).
  Retailers often offer longer windows, stated in the email.
- RSVP. No standard; hosts name a "reply by" date.

### Recommended defaults, by type

| Type | Act-by rule | Surfaces | Basis |
|---|---|---|---|
| Bill, pay link on biller's site | due date | 3 days before act-by | Immediate payment; 3 days is slack for "not today" |
| Bill, bank details only | due minus 3 working days | 3 days before act-by | Bacs cycle |
| Bill collected by direct debit or card on file | not a to-do | Updates | Monzo's model: scheduled, not a task |
| Subscription or insurance renewal | due date | 14 days before | Time to compare quotes; the blueprint's 7 is too short to shop around (judgement) |
| Passport, visa | expiry minus 6 months for travel validity, or the trip date if known | 10 weeks before act-by | HMPO's advice; US 4 to 6 weeks plus post |
| Driving licence, ID card | expiry | 8 weeks before | Judgement, between the two above; needs a source |
| Lease or tenancy | end minus notice period (2 months default in England) | 4 weeks before act-by | Renters' Rights Act notice |
| Return window | delivered + stated window (14 days default) | 4 days before close | Packing plus drop-off |
| RSVP | the "reply by" date, else 2 days before the event | 2 days before | Blueprint default |
| Verify, confirm | link expiry | at once | Links expire in hours |
| Promise you made | the date you named | 1 working day before | You chose the date, so less slack is needed (judgement) |
| Other | due date | 2 days before | Blueprint default |

This amends the blueprint's table in three places: renewals of insurance
and subscriptions get 14 days, documents get 10 weeks (30 days is shorter
than HMPO's own advice), and leases and returns compute an act-by date
before the lead time applies.

### Per-type defaults beat one fixed default and beat a learned one

- One fixed default fails because the spread is two orders of magnitude:
  a verify link needs hours, a passport needs ten weeks.
- A learned lead time fails for the reason Todoist withdrew Smart
  Schedule: inaccurate date guesses are worse than none, and the user
  can't see why a date moved. Most of these events happen once a year or
  less, so there's too little per-user data to learn from anyway.
- What can adapt is explicit and visible: the user edits a lead time per
  type in config, or per sender with one action on a row ("Remind me 2
  weeks earlier for Admiral"), stored like the per-sender corrections in
  `mode_corrections`. The row then says "act by 12 Oct (your setting for
  Admiral)".

## 7. Your promises and admin to-dos belong in one list, with the person shown

### One list, because the verb and the clock are the same

Both are "do it by a date, then tick it off", which is the mode's verb
(D107). Splitting them into two sections makes the user merge two
deadline-ordered lists in their head, which is the job the view exists to
do. GTD keeps all next actions in one system and separates only what
others owe you ("Waiting for"); in mxr that already lives in Messages as
Waiting on.

### The person is the difference, so the row shows the person

Evidence from Taskmaster and Viva (section 2) says users want to see whose
move it is and who is owed. So:

- A promise row shows the person's name and face and "you promised", with
  your own words quoted as the reason ("I'll send it back Friday").
- An admin row shows the organisation and the amount.
- A filter chip, "For people", shows only promises, for the moment before
  a meeting or a weekly review.
- On equal act-by dates, promises sort first. Judgement: a missed bill
  costs a fee that's usually fixed; a missed promise costs trust that
  isn't.
- The promise also shows in Messages, in that person's conversation, as
  "you promised: send the signed form, Fri" (the blueprint's multi-mode
  rule). Ticking it in either place ticks it in both, because it's one row.

Requests from others ("Can you send the form by Friday?") are admin-like
to-dos with a person as counterparty; they render as promise rows do,
labelled "Priya asked", so the three Viva categories collapse to two
labels in one list (you promised, they asked) plus Waiting on in Messages.

Storage stays as designed: promises stay in `contact_commitments`, admin in
`todos`, and `ListTodos` merges them into one ordered response with a
`kind` field. No migration of promises is needed.

## 8. How this differs from a list of emails

| A list of emails | The To do view |
|---|---|
| One row per message or thread, titled by the sender's subject | One row per thing to do, titled verb plus object, written for you |
| Ordered by arrival | Ordered by when you must act, with lead time already applied |
| Every row has the same actions (reply, archive, star) | Each row has one button, labelled with its verb and destination |
| Dates are when mail arrived | Dates are act-by and due; arrival is in the small print |
| A bill and its reminder and its receipt are three rows | They are one row that updates and then completes itself |
| Done means archived, which also hides the conversation | Done means done here; the email stays in its other modes and is offered to Archive |
| Opening the email is the only way to learn what to do | The row says it; the email is one key away for proof |
| Snooze hides the email until a time | Schedule sets a When date while the deadline stays visible, and news in the thread brings it back early |

## 9. Open questions and risks

- Phishing. A "Pay on ..." button is the most dangerous control in an
  email app. Proposal: the one-click button appears only when DMARC passes
  for the sender's domain (`auth_results`) and the link's registrable
  domain matches the sender's or one seen in earlier mail from them.
  Otherwise the row shows "Open email to pay" and the raw domain. Needs a
  decision and a test fixture set.
- Precision of the rule detector on real mail is unknown. Phase 1's
  check (BK reviews counts) is the right gate; I'd add a target: under one
  false to-do a week on BK's mail before To do gets a rail badge.
- Direct debit detection decides whether most utility bills appear at
  all. Wrong in one direction, To do floods; wrong in the other, a failed
  direct debit ("we couldn't collect your payment") is missed. Failed
  collection must always be a to-do.
- Auto-complete trust. Is auto-ticking on a strong receipt match right,
  or should every completion be one key? The proposal auto-completes only
  RSVP (exact) and strong pay matches, with undo for a day.
- Return windows need the retailer's own window, which is often only on
  a web page. Default to 14 days and say "assumed 14 days".
- Numeric dates ("09/10/2026") aren't parsed by `natural_time` and are
  ambiguous between UK and US order. Use schema.org first, then the
  account's locale, then ask.
- Travel-validity rule for passports needs a trip date mxr may not
  have. Without one, surface at expiry minus 6 months minus 10 weeks, and
  say why.
- Weekly money total ("£554 out this week") may read as financial
  advice or cause worry. Dogfood before shipping.
- Lead-time defaults are mostly UK sources; US and other users need
  their own (ACH, USPS, state notice periods).
- Promises first on ties is judgement; BK should confirm.

## 10. Sources

- Bellotti, Ducheneaut, Howard, Smith. "Taking email to task: the design
  and evaluation of a task management centered email tool." CHI 2003.
  http://www.chi2003.org/docs/takingemail.pdf
- Sarrafzadeh et al. "Characterizing and Predicting Email Deferral
  Behavior." WSDM 2019. https://arxiv.org/abs/1901.04375
- Things Support, Scheduling To-Dos. https://culturedcode.com/things/support/articles/2803579/
- Things Support, Today, Upcoming, Anytime, Someday. https://culturedcode.com/things/support/articles/4001304/
- Todoist, Add deadlines to tasks (Jan 2025). https://www.todoist.com/help/todoist/product-updates/add-deadlines-to-tasks-jan-7-2025-WFbv37eEw
- Todoist, GTD tips (overdue guilt, reschedule). https://www.todoist.com/inspiration/gtd-tips
- Todoist on Smart Schedule's removal. https://x.com/todoist/status/1260030543556603904
- MacStories, Smart Schedule launch. https://www.macstories.net/news/todoist-launches-smart-schedule-an-ai-based-feature-to-reschedule-overdue-tasks/
- OmniFocus manual, dates. https://support.omnigroup.com/documentation/omnifocus/ios/2.22/en/using-dates-and-times/
- OmniFocus 4 perspectives (Forecast). https://support.omnigroup.com/documentation/omnifocus/universal/4.3.3/en/perspectives/
- Linear, Triage. https://linear.app/docs/triage
- Sunsama, Daily planning. https://www.sunsama.com/daily-planning ; walkthrough https://calmevo.com/how-to-use-sunsama/
- HEY features. https://www.hey.com/features/
- Microsoft, Using Microsoft To Do with flagged email. https://support.microsoft.com/en-us/office/using-microsoft-to-do-with-flagged-email-from-outlook-f90c37b0-4453-4756-a6d5-e2ef8d33b395
- Microsoft, Cortana in the Viva Briefing email. https://support.microsoft.com/en-gb/topic/how-cortana-helps-you-in-briefing-email-from-microsoft-viva-478c90f6-a9b4-3c24-6a63-ff85426611a5
- Office 365 IT Pros, Viva Briefing pause. https://office365itpros.com/2022/12/23/viva-briefing-pause/
- Pocket-lint, Google Now bill cards. https://www.pocket-lint.com/apps/news/google/128940-google-now-adds-bill-reminder-cards-pulled-from-gmail-so-you-can-pay-bills-on-time/
- Search Engine Land, Google Now bill reminders. https://searchengineland.com/google-now-reminds-pay-bills-205379
- Monzo, Salary Sorter and Bills Pots. https://monzo.com/blog/2019/09/26/introducing-salary-sorter-and-bills-pots ; community thread https://community.monzo.com/t/pay-day-salary-sorter-pots-direct-debits/135249
- Zoho community, link back to email. https://help.zoho.com/portal/zh/community/topic/gmail-extension-really-needs-a-link-back-to-the-original-email
- GQueues group, no link to email. https://groups.google.com/g/gqueues/c/Cc4Tm8mflOY
- David Allen, two-minute rule. https://gettingthingsdone.com/2011/06/when-to-use-gtds-two-minute-rule/ ; https://gettingthingsdone.com/2020/05/the-two-minute-rule-2/
- Next actions, physical and visible. http://www.43folders.com/2004/09/17/next-actions-both-physical-and-visible (site now parked; summary at https://super-productivity.com/blog/gtd-next-actions-guide/)
- Alfred, AI task extraction (vendor). https://get-alfred.ai/blog/ai-that-extracts-tasks-from-emails-automatically
- Bacs timing. https://www.accesspaysuite.com/blog/how-long-do-bacs-payments-take/ ; https://www.iris.co.uk/blog/payroll/whats-the-difference-between-bacs-and-faster-payments/
- UK passport timing. https://www.lonelyplanet.com/articles/when-to-renew-your-uk-passport-as-wait-times-increase ; https://blog.wego.com/uk-passport-renewal-delays/
- US passport processing times. https://travel.state.gov/content/travel/en/passports/how-apply/processing-times.html
- Tenancy notice (England). https://www.nrla.org.uk/resources/renters-rights/ending-a-periodic-assured-tenancy ; https://www.citizensadvice.org.uk/housing/ending-a-private-tenancy/ending-your-tenancy/
- Consumer Contracts Regulations guidance. https://assets.publishing.service.gov.uk/media/5a817b92ed915d74e33fe73a/bis-13-1368-consumer-contracts-information-cancellation-and-additional-payments-regulations-guidance.pdf
