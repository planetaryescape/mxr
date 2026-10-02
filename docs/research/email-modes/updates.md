# Updates: a briefing of facts by source, read twice a day and let go in one key

Research for the Updates mode in [22-email-modes.md](../../blueprint/22-email-modes.md).
Code references are to `docs/email-modes` at `bdf997c4`. Evidence and
opinion are marked: "the study found" means a cited source; "we propose"
means judgement.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## 1. The job is staying aware without being interrupted, and success is a short read that ends

Updates holds mail that tells you something happened and asks nothing:
a sign-up, your week on Strava, a new sign-in, a build that passed, an
order that shipped, a price drop, a status report. The verb is "glance and
let go". Nobody wants to process these one at a time, and nobody wants to
miss the one that matters.

People keep notifications on for awareness, not to act. Iqbal and Horvitz
logged 2 weeks of real work and found users "view notifications as a
mechanism to provide passive awareness rather than a trigger to switch
tasks", and that some users who lost notifications self-interrupted more
to check for themselves ([Iqbal and Horvitz, CSCW 2010][iqbal]). So the
mode must give awareness, or people will go and look anyway.

Success feels like reading a morning briefing: one screen, the two or
three things that changed at the top, the routine stuff reduced to a line,
then a single "let go" and it's over. You know your parcel is out for
delivery, your build is green, nobody new signed in, and you never opened
an email to find out. If something needs you, it has already left Updates
for To do.

## 2. The best tools batch on a predictable schedule, show state not history, and rank a few items above the rest

### Batching three times a day at fixed times helped; hourly did nothing and none caused anxiety

The strongest evidence on rhythm is a randomised field experiment
(n = 237, two weeks) that delivered phone notifications as usual, batched
hourly, batched three times a day (9:00, 15:00, 21:00), or never. The
three-a-day group "felt more attentive, productive, in a better mood, and
in greater control of their phones" and reported lower stress. The hourly
group "did not differ from the control" except on feeling interrupted.
The never group "reaped few of those benefits, but experienced higher
levels of anxiety and fear of missing out" ([Fitz, Kushlev et al.,
Computers in Human Behavior 2019][fitz]). Participants could still open
any app at any time; only delivery was batched.

Two cautions from the email literature. Mark et al. tracked 40 workers for
12 days and found batching email linked to higher rated productivity but
"no evidence that batching email leads to lower stress" ([Mark, Iqbal,
Czerwinski et al., CHI 2016][mark2016]). Cutting email entirely for 5 days
lowered stress as measured by heart rate variability and lengthened task
focus ([Mark, Voida, Cardello, CHI 2012][mark2012]). Read together: fewer,
predictable check-ins help focus; the stress win comes from less volume,
which for Updates means aggregation and muting, not just a schedule.

### iOS Scheduled Summary puts a ranked few on top and lets the rest wait

iOS 15's Notification Summary delivers non-urgent notifications "at
scheduled times in the day as a summary" ([WWDC21 session 10091 notes][wwdc]).
It is ordered by on-device relevance, "the most important notifications
appear above the less important" ([TechCrunch, June 2021][tc-ios15]), and
two notifications are featured at the top, helped by an app-supplied
`relevanceScore` and media ([Batch on WWDC21][batch]). Calls, direct
messages and time-sensitive notifications skip the summary and arrive at
once, and "Show Next Summary" lets you pull the next one early ([Apple
Support][apple-summary]). Apps declare an interruption level: passive,
active, time-sensitive or critical ([WWDC21 notes][wwdc]).

What mxr should take: user-chosen fixed times, a featured top, a peek
before the scheduled time, and a breakthrough path for the few
time-sensitive items. What it should not take: the feature ships off by
default with no apps included ([Batch][batch]), and NN/g notes "most users
don't bother customizing their systems" ([NN/g, push notifications][nng]).
A digest that needs setup won't be used.

### Live Activities and watch complications show the current state of a thing, not a stream of events

Apple's Live Activities are for tasks "with a clear start and end point"
(a delivery, a ride, a score): they update in place instead of sending a
new notification per change, end when the task completes, and alert only
on important changes ([Apple HIG, Live Activities][hig-live]). On the
watch, Glances (a swipe-through stack of app summaries) were removed in
watchOS 3 because they were slow and cumbersome, and replaced by
complications and a Dock showing "recently refreshed" snapshots ([Macworld][macworld-glances]).
The lesson: for anything with state, one line that says where it is now
beats a history of how it got there.

mxr already models this for parcels. `crates/deliveries` collapses many
emails into one `deliveries` row whose `status` "advances monotonically"
from ordered to delivered (`crates/store/migrations/042_deliveries.sql`).

### One row per source, with batch actions, is what HEY, Shortwave, SaneBox and Android converge on

HEY's Bundles: "No matter how many emails they send you, they'll only take
up a single row in your Imbox or Paper Trail" ([HEY features][hey]).
Shortwave bundles "condense multiple emails into just one compact line"
and you can "snooze, delete, or mark an entire bundle done" with one key,
with delivery schedules to defer when they arrive ([Shortwave bundles][shortwave]).
SaneBox's Daily Digest groups by folder and bundles same-sender mail:
"several shipping notifications ... may appear as a single group"
([SaneBox Digest guide][sanebox]). Android 16's Notification Organizer
bundles Promotions, News, Social and Suggested into the Silent section,
one row per category with app icons stacked ([9to5Google][organizer]), and
Android 8 channels let users silence one kind of notification from an app
while keeping another ([Android channels overview][android-channels]).

### GitHub and Linear make "done" a one-key verb and say why each item is there

GitHub's inbox: `e` marks done, `Shift+U` unsubscribes, `m` mutes a
thread, notifications can be grouped by repository, and each carries a
reason label (assigned, mentioned, review requested) ([GitHub Docs][gh-inbox]).
Linear: `Backspace` deletes, `Shift+Backspace` deletes all read, `H`
snoozes, `Shift+S` unsubscribes ([Linear Docs][linear]), and a snoozed
notification comes back early when there is new activity on the issue
([Linear changelog][linear-snooze]). Both put the source and the reason on
the row, so you can decide without opening.

### Ops teams learned that only actionable alerts may interrupt; the rest belongs on a dashboard

Google's SRE book: "Every page should be actionable", "If a page merely
merits a robotic response, it shouldn't be a page", and for subcritical
issues prefer "a dashboard that monitors all ongoing subcritical problems"
over email alerts, which "tend to easily become overrun with noise". It
warns that people "can only react with a sense of urgency a few times a
day before" fatigue ([Google SRE book][sre]). Updates is the dashboard;
To do is the page.

### Status pages lead with one sentence of overall state, then only the parts that aren't fine

GitHub's status page opens with "All Systems Operational", lists each
component with its state, shows 90 days of daily health as coloured bars,
and puts incidents below in reverse order with timestamped updates
([GitHub Status][statuspage]). The routine case costs one line; detail
appears only for what changed. That is the shape for a digest headline
("2 things changed, 23 routine") and for a source line like "Uptime
Robot: all monitors up, 1 blip at 02:14".

### A briefing leads with what's new and why it matters

Axios's Smart Brevity format is a tease, one strong first sentence, "why it
matters", and a link to go deeper; Axios claims updates come out about 40%
shorter with the same information ([Axios HQ][axios]). That is opinion
from a vendor, but the shape (fact first, context second, link last) is
the right shape for one Updates line.

## 3. Updates views fail when they are a second inbox, when AI rewrites facts, and when they need setup

### Gmail's Updates tab is a list of emails in a drawer, so people stop opening it

Gmail describes Updates as "automated confirmations, notifications,
statements, and reminders that may not need immediate attention"
([Gmail Help][gmail]). It shows them exactly like Primary: one row per
email, subject and snippet, an unread count. Nothing collapses, nothing
says what changed, and bills that do need attention sit beside build
notices that don't. There is no published open-rate data for the Updates
tab; the nearest number is a ZeroBounce survey in which 25% said they
never check the Promotions tab ([Mailbird summary][mailbird]). The
inference (ours) is that a tab that looks like work but holds noise
teaches people to ignore it, including the bill inside it.

### Apple's AI summaries invented facts, merged sources, and lost trust in the publisher's name

Apple Intelligence summarised a stack of BBC notifications as "Luigi
Mangione shoots himself; Syrian mother hopes Assad pays price; ...", when
the headline was about a court appearance ([Silicon Republic][siliconrepublic]).
The BBC complained, and in iOS 18.3 Apple paused summaries for news and
entertainment apps, italicised all summaries, added "Summaries may contain
errors" in Settings, and let users turn summaries off per app from the
Lock Screen ([TechCrunch, Jan 2025][tc-pause]; [iDownloadBlog][idb]).
Other reported errors include an ESPN summary inventing a postponement and
a breakup text reduced to "No longer in a relationship; wants belongings
from the apartment" ([TechRadar][techradar]).

Three failures stack here. The model summarised across several
notifications at once, so a fact from one item attached to another. The
false sentence appeared under the publisher's name, so it read as the
BBC's claim. And it looked identical to real notification text. Google's
Android 16 summaries are, by contrast, limited to messaging apps,
"avoiding auto-generated digests for categories like news or promotions"
([Parameter][parameter]).

### Every view that shows each event separately drowns the one that matters

Pielot et al. logged 794,525 notifications from 278 people: a median of
56 a day, and non-messaging notifications "rarely lead to a conversion
(rates between ca. 15 and 25%)" ([Pielot, Vradi, Park, MobileHCI 2018][pielot]).
Users "mostly dismiss (i.e., swipe away without clicking)" notifications
that aren't relevant ([Mehrotra PhD thesis, 2017][mehrotra]). The SRE book
describes the consequence: people "second-guess, skim, or even ignore
incoming alerts, sometimes even ignoring a 'real' page that's masked by
the noise" ([Google SRE book][sre]). mxr's own demo shows the mirror
failure: "Action required: unusual sign-in attempt" sat in You owe and
"Build failed on release branch" in New from people (blueprint 22).

### Today's Paper trail page is a list of emails with a reason line

The current page (`apps/web/src/routes/paper-trail.tsx`, judge screenshot
`08-paper-trail`) shows "3 messages from 3 senders", one row per sender
with the subject, a count and "Here because: automated sending domain",
plus pin and sweep. It is already better than Gmail (bundled by sender,
sweep with preview), but the row is still an email: the subject is shown
as written, there is no current state, no number, no "what changed", and
receipts (Archive) and alerts (Updates) share the page.

## 4. Each update becomes a source, a fact, an optional state and numbers; most of it is derivable without a model

| Field | What it is | How it's derived | mxr today |
|---|---|---|---|
| `source_key` | The stream this belongs to, e.g. `strava.com`, `github.com/acme/api` | Sender address normalised to the registrable domain, plus `List-Id` or a repo/project token when present | `mail_kind::classify` gives sender kind; `PlaceMessage` has `from_email`, `list_id` (`crates/store/src/places.rs`) |
| `source_name` | "Strava", "GitHub · acme/api" | Display name, else domain label | `from_name` |
| `template_key` | Which kind of update from that source ("weekly summary", "build result", "new sign-in") | Subject with numbers, IDs, dates, names and quoted strings masked, hashed with `source_key` | Needs building; pure function, no model |
| `fact` | One sentence: "Your order shipped", "New sign-in from Chrome on Windows, Lisbon" | Cleaned subject first (strip "Notification:", brand prefixes, "[acme/api]" tags); model only when the subject is generic ("Your weekly update") | Subject only. `thread_gist.rs` has the model pipeline with quote checking and caching to copy |
| `numbers` | `[{label, value, unit}]`: "21.3 km", "3 runs", "$12.40", "12 sign-ups" | Regex over subject and the first screenful of cleaned text (`mxr_reader::clean`); model may label, never invent, values; every value must appear verbatim | Needs building |
| `delta` | "up 12% on last week" | Code compares `numbers` with the previous message of the same `template_key`. Never the model | Needs building |
| `entity_key` and `state` | For things with a lifecycle: parcel, build, PR, incident, subscription renewal | Deliveries: tracking number and status (exists). Builds/incidents: subject words (passed, failed, resolved) keyed by branch or incident id | `deliveries.status`, `DeliveryStatus` enum in `crates/deliveries/src/lib.rs`, `schema_org.rs` extracts `ParcelDelivery` |
| `signal` | `routine`, `changed`, `new_source`, `anomaly`, `needs_you` | Rules: failure words (failed, declined, unusual, exception, suspended); a state that regressed (delivery `Exception`, `AttemptFail`, `Returned`); a number outside the source's usual range; first message from a source; a to-do aspect present | Partly: `triage.rs` has an FYI/Action verdict from the model; delivery status exists |
| `escalation` | The to-do it should become | The phase 1 to-do detector (blueprint 22) running on the same message | Planned in phase 1 (`todos` table) |
| `link` | The one place to go deeper ("View order", "Open build") | First prominent call-to-action URL in the HTML, or `tracking_url` | `deliveries.tracking_url`; link extraction exists for reader (`033_link_count.sql`) |
| `provenance` | `rule`, `schema`, `model:<name>` per field | Recorded with the fact | Pattern exists: `AiProvenanceData` in `thread_gist.rs` |
| `digest_id` | Which digest cut it fell into | Received time against the user's digest times | Needs building |

Two derived views sit on top. A **source line** aggregates every update
from one `source_key` in the digest window: count, latest fact per
template, current state for any tracked entity, and the strongest signal.
A **tracker** is one entity with a lifecycle (a parcel, a build branch, an
incident); it shows its latest state and disappears from Updates when
terminal (delivered, resolved) unless it ended badly.

### The model is optional and summarises one message at a time

The Apple failure was a model writing one sentence from several items. In
mxr the model, when enabled, sees one message and returns a fact sentence
and labels for numbers it can quote. Aggregation across messages (counts,
latest state, deltas) is code. The daemon checks each quoted number and
phrase appears in the text, as `thread_gist.rs` already does for asks.
Model text is shown in italics with "summary by <model>" on hover, the
verbatim subject is one keypress away, and a source can be set to "never
summarise". Background classification follows the loopback-only rule in
blueprint 22.

### Storage follows the existing patterns

- `update_facts (message_id, template_key, source_key, fact, numbers_json,
  signal, provenance_json, prompt_version, content_hash)`: a cache keyed
  like `triage_cache` (`044_triage_cache.sql`), recomputable.
- `update_sources (account_id, source_key, tuning, decided_at)` with
  `tuning` in `every_digest | changes_only | muted | breakthrough`. Muting
  could reuse the screener's per-sender decision store
  (`crates/store/src/screener.rs`) as a fifth disposition; a separate table
  keeps screener semantics clean. We lean to the separate table.
- Let go reuses `mode_done` from blueprint 22 with the `desk_dismissals`
  watermark (`051_desk_dismissals.sql`): through-rowid and count per
  thread, so a new message brings only that thread back.
- Trackers for parcels stay in `deliveries`; builds and incidents get a
  small `update_trackers` table only if real mail shows enough of them.
  Start with deliveries only.

## 5. The proposed view is a briefing: what needs a look, what changed, and the routine folded into one line per source

### The digest arrives at two fixed times you choose, and Updates is always current in between

We recommend fixed, predictable cuts, two a day by default (08:00 and
16:30 in the user's zone), configurable from one to four, plus an
always-available view of what has arrived since the last cut.

The evidence for this choice:

- Predictable batching at a few fixed times is the only schedule with a
  measured benefit; hourly had none and no delivery raised anxiety
  ([Fitz et al.][fitz]). Three a day was tested; two is our extrapolation
  for email, whose updates are slower than phone pings and where Mark
  found batching helps productivity more than stress ([Mark 2016][mark2016]).
- Users choose the times in iOS, Shortwave and SaneBox ([Apple Support][apple-summary];
  [Shortwave][shortwave]; [SaneBox][sanebox]). Fixed defaults matter more
  than choice, because most users don't customise ([NN/g][nng]).
- People who lose the stream go looking ([Iqbal and Horvitz][iqbal]; FoMO
  in [Fitz][fitz]). So the page never hides what has arrived: a
  "since 08:00" section sits below the digest, quiet and uncounted, the
  equivalent of iOS's "Show Next Summary".

Once a day (blueprint 22's draft) is defensible for heavy users of
Reading and To do, but a parcel out for delivery at 10:00 is stale by the
next morning. Two cuts cover "what happened overnight" and "what happened
today" without becoming a stream.

The cut also does an engineering job: it is a stable set. "Let go of this
digest" acts on exactly the messages in the cut, so the dry-run preview
and the commit match (AGENTS.md: "The preview selection path must match
the real mutation path"), and nothing that arrived a second ago is let go
unseen.

On usage data: we found no published adoption or usage numbers for iOS
Scheduled Summary. Apple ships it off by default with no apps included
([Batch][batch]), and the public record is how-to articles, not data. We
should not assume people want to configure this; ship it on, with
defaults, and measure our own use (counts only) in the dogfooding log.

### The digest takes three forms: a sentence on the desk, a briefing page, and a source line per sender

On the desk (Now), one card per cut. A headline sentence written by code
from the signals, then at most three lines that need a look or changed,
then a count:

```
UPDATES  since 08:00                                             let go  L
  2 things changed, 1 new sign-in. 23 routine from 9 sources.
  ! New sign-in to Google from Chrome on Windows, Lisbon          → to do  t
  ◆ Parcel from Bookshop: out for delivery, arriving by 18:00
  ◆ acme/api build: failing on main since 11:02 (3 runs)
    + 23 routine updates                                           open   g u
```

On the Updates page, a briefing in three sections. The unit is the
source, not the email.

Desktop web:

```
┌ Updates ───────────────────────────────────────────────────────────────────────────┐
│ This morning's digest · 08:00 · 31 updates from 12 sources      [Let go of all  L] │
│                                                                                    │
│ NEEDS A LOOK                                                                       │
│ ! Google        New sign-in from Chrome on Windows, Lisbon, 06:12                  │
│                 Not you? First sign-in from this device.   [This needs me  t]  [Fine  x] │
│ ! Stripe        Payout of R 4,210.00 failed: bank declined        [This needs me  t]│
│                                                                                    │
│ CHANGED                                                                            │
│ ◆ Bookshop      Parcel  ordered ─ shipped ─ ● out for delivery ─ delivered         │
│                 Arriving today by 18:00 · DHL · 3 emails                 [Track ↗] │
│ ◆ GitHub acme/api   Build failing on main since 11:02 · 3 runs · was green 2 days  │
│ ◆ Strava        Your week: 3 runs, 21.3 km  ▲12% on last week                      │
│                                                                                    │
│ ROUTINE                                                                            │
│   Vercel        7 deploys succeeded · latest 07:41 acme-web               7        │
│   Linear        5 issue updates in Q4 Launch                              5        │
│   Uptime Robot  All monitors up · 1 blip 02:14 (40s)                      6        │
│   Plausible     Weekly report: 1,204 visitors  ▼3%                        1        │
│   + 4 quieter sources                                                     8        │
│                                                                                    │
│ ─ arriving for 16:30 ─────────────────────────────────────────── 4 so far ─────── │
│   Notion, GitHub (2), Bookshop                                         [peek  .]   │
└────────────────────────────────────────────────────────────────────────────────────┘
  j/k move · enter expand source · L let go of digest · x let go of source
  t this needs me · m mute source · c changes only · o open link · v verbatim · ? why
```

Mobile (single column, 16px gutter, thumb actions at the bottom):

```
┌─────────────────────────────┐
│ Updates          08:00 ▾    │
│ 31 from 12 sources          │
│                             │
│ NEEDS A LOOK                │
│ ! Google                    │
│   New sign-in, Chrome on    │
│   Windows, Lisbon 06:12     │
│   [Needs me]   [Fine]       │
│ ! Stripe                    │
│   Payout R 4,210 failed     │
│   [Needs me]   [Fine]       │
│                             │
│ CHANGED                     │
│ ◆ Bookshop parcel           │
│   ●●●○ out for delivery     │
│   by 18:00                  │
│ ◆ acme/api build failing    │
│ ◆ Strava 21.3 km ▲12%       │
│                             │
│ ROUTINE · 27 from 8         │
│   Vercel 7 · Linear 5 ·     │
│   Uptime 6 · +3 more    ›   │
│                             │
│ ┌─────────────────────────┐ │
│ │    Let go of all 31     │ │
│ └─────────────────────────┘ │
└─────────────────────────────┘
```

Swipe left on a source: let go of that source. Swipe right: this needs
me. Long-press: mute, changes only, never summarise.

TUI (a lens beside `crates/tui/src/ui/desk_lens.rs` and `place_lens.rs`):

```
 Updates · digest 08:00 · 31 from 12 sources                       L let go all
 ─────────────────────────────────────────────────────────────────────────────
 needs a look
 ! Google         New sign-in, Chrome on Windows, Lisbon 06:12        t  x
 ! Stripe         Payout R 4,210.00 failed: bank declined             t  x
 changed
 ◆ Bookshop       parcel [##### ] out for delivery · by 18:00 · DHL
 ◆ acme/api       build failing on main since 11:02 · 3 runs
 ◆ Strava         week: 3 runs · 21.3 km · +12%
 routine
   Vercel         7 deploys ok · latest 07:41                          7
   Linear         5 issue updates · Q4 Launch                          5
   Uptime Robot   all up · 1 blip 02:14                                6
 > 4 quieter sources                                                   8
 ── since 08:00: 4 (Notion, GitHub 2, Bookshop) ─────────────────── . peek
 j/k  enter expand  x let go  L let go all  t needs me  m mute  c changes  v verbatim
```

### Five keys cover the job, and they match web and TUI

| Key | Action | Notes |
|---|---|---|
| `L` | Let go of the whole digest | Opens the preview ("Let go of 31 updates from 12 sources; 2 also in To do stay there"), `Enter` confirms. Same selection for preview and commit |
| `x` | Let go of this source (or this item in Needs a look) | No preview for a single source; undo toast |
| `t` | This needs me | Creates a to-do through `CreateTodo { from_message }` prefilled ("Check new sign-in", "Fix payout bank details"), and lets go of the update |
| `m` / `c` | Mute source / changes only | Effect shows at once ("Strava: only when something changes"), undo |
| `enter` / `o` / `v` | Expand a source / open its link / show the verbatim email | Opening the email is the third option, not the first |

`u` stays global undo. `?` on any line shows why it's there and where
each field came from ("fact from subject; 21.3 km quoted from body; +12%
computed against 26 Sep").

### Items enter by rules, leave by let go, and escalate to To do by one key or a rule

Entering: a message enters Updates when its base mode is Updates
(automated sender, blueprint 22 table) and no correction says otherwise.
A message with a to-do aspect is in both; Updates shows the fact, To do
shows the task.

Breaking through: an update with signal `needs_you` (security sign-in,
failed payment, delivery exception, a failing build on a branch you
pushed to) does not wait for the cut. It goes to To do at once with
"Here because: new sign-in alert (rule)" and still appears in the next
digest's Needs a look, marked "already in To do". This is iOS's
time-sensitive level and the SRE "page only when actionable" rule. A
source the user set to `breakthrough` goes to the desk on arrival.

Leaving: let go writes `mode_done` for Updates. The provider archive
happens only if no other mode holds the thread (blueprint 22). Trackers
leave on their own when terminal and good (parcel delivered moves to
Archive per blueprint 22); a bad terminal state (returned, failed) moves
to Needs a look instead. Anything left in a digest after the next cut
folds into it ("2 from this morning") rather than stacking a second
digest, so the page never grows a backlog.

Tuning suggests itself from behaviour, with the rule shown: "You've let
go of Strava 8 digests running without opening it. Mute, or changes
only?" Mehrotra's notification work mined rules from past interactions
and made them "transparent to users so that they can check their
appropriateness" ([Mehrotra thesis][mehrotra]). Ask at most once per
source per month.

### The empty state says when the next digest comes and what's quiet

```
Nothing new since 08:00.
Next digest at 16:30. 12 sources, 3 muted.        [review muted sources]
```

No illustration, no "inbox zero" celebration. After let go of all, the
same line with "Let go of 31 updates · Undo".

### Motion is a fold, and there is no sound

Updates never makes a sound or a badge; badges count work only (blueprint
22). On let go, a source's rows fold up into its header line in about
150 ms and the header fades; on let go of all, sections fold in order top
to bottom and the page settles on the empty state with the undo line. A
tracker's state dot moves along its track when the state changes while the
page is open. `t` slides the line toward the To do rail item and leaves a
"→ To do" ghost for two seconds. All motion respects
`prefers-reduced-motion`.

## 6. This is not a list of emails: the unit, the text, the time and the action all change

- The row is a source, not a message. Thirty-one emails from twelve
  senders are twelve lines, and most are folded into "routine".
- The text is the fact, not the subject. "Your weekly update is here"
  becomes "Your week: 3 runs, 21.3 km, up 12%"; the subject is behind `v`.
- Things with state show where they are now. Four parcel emails are one
  track with a dot on "out for delivery", not four rows.
- Numbers are compared for you. The delta comes from the previous message
  of the same kind, computed by code.
- Order is by signal (needs a look, changed, routine), not by date. Rows
  have no timestamps unless the time is the fact ("06:12 sign-in").
- There is no unread state and no count badge. The page is read or it
  isn't; the digest is the unit of done.
- The primary action is let go of the batch. Opening an email is the last
  resort, and replying isn't offered at all.
- Escalation is a handoff to another mode, not a flag on the email.

## 7. Open questions and risks

- Is two cuts a day right for BK, or one? The evidence supports a few
  fixed times, not a specific number for email. Decide after a week of
  dogfooding with counts recorded in `docs/dogfooding-log.md`.
- `template_key` by masking subjects will split one kind of update into
  several when senders vary wording, and merge different kinds when
  subjects are generic ("You have a new notification"). Needs a fixture
  set from BK's real automated senders before it ships; the fallback is
  grouping by source only.
- Number extraction and deltas are where trust breaks. A wrong "+12%" is
  the BBC failure in miniature. Show deltas only when both values were
  quoted from the same template and units match; otherwise show the raw
  number.
- Escalation rules will have false positives ("new sign-in" from your own
  laptop). The `Fine` action must be one key, and a sender-level "these
  are always fine" correction is needed. Should breakthrough to To do
  ever happen without a model, or only on very specific rules?
- Should GitHub notification mail about PRs you're reviewing be Updates
  at all? Review requests are to-dos and mentions are Messages-like. The
  per-message aspects in blueprint 22 should handle it, but GitHub is the
  likely heaviest source and deserves its own fixtures.
- Mute vs unsubscribe: muting hides from Updates but keeps mail flowing
  to Archive and search. Offer unsubscribe (exists, `Request::Unsubscribe`)
  on the mute prompt when the sender has `List-Unsubscribe`?
- Where do receipts land: Archive only, or a line in Updates ("Paid
  Vercel $20") on the day? We lean to Archive only, with the purchase in
  Updates only when it's a surprise (new merchant, unusual amount).
- Weekly cut: some sources (Strava, Plausible) are weekly by nature. A
  Sunday "this week" view could aggregate them, but that is a feature to
  validate, not ship now.

## 8. Sources

Sources marked "second-hand" were read through a search engine's summary,
or the page did not render for the fetch tool; their quotes should be
checked against the page before anything here is published. Every other
source was fetched and read.

Research and data:

- [fitz]: Fitz, Kushlev et al., "Batching smartphone notifications can improve well-being", Computers in Human Behavior, 2019. https://static1.squarespace.com/static/57a40c19414fb54f51f8095f/t/5d2e7f8efcb18b0001bd506d/1563328398940/2019+Fitz-Kushlev-etal+2019.pdf
- [iqbal]: Iqbal and Horvitz, "Notifications and awareness: a field study of alert usage and preferences", CSCW 2010. https://www.semanticscholar.org/paper/Notifications-and-awareness:-a-field-study-of-alert-Iqbal-Horvitz/700768a496da0595b0bc999e2f126f5a29a13a2b (second-hand: search summary only, page not fetched or not rendered)
- [mark2016]: Mark, Iqbal, Czerwinski, Johns, Sano, "Email Duration, Batching and Self-interruption", CHI 2016. https://www.microsoft.com/en-us/research/wp-content/uploads/2016/06/Email20Duration20Camera20Ready20submission3-1.pdf
- [mark2012]: Mark, Voida, Cardello, "A pace not dictated by electrons", CHI 2012. https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%202012.pdf (second-hand: search summary only, page not fetched or not rendered)
- [pielot]: Pielot, Vradi, Park, "Dismissed! A Detailed Exploration of How Mobile Phone Users Handle Push Notifications", MobileHCI 2018. https://www.interruptions.net/literature/Pielot-MobileHCI18.pdf
- [mehrotra]: Mehrotra, "A Framework for Intelligent Mobile Notifications", PhD thesis, University of Birmingham, 2017. https://etheses.bham.ac.uk/id/eprint/7440/1/Mehrotra17PhD.pdf
- Pielot and Rello, "Productive, Anxious, Lonely: 24 Hours Without Push Notifications" (half of participants kept their changes two years later). https://arxiv.org/abs/1612.02314
- [sre]: Google, Site Reliability Engineering, "Monitoring Distributed Systems". https://sre.google/sre-book/monitoring-distributed-systems/
- [nng]: Nielsen Norman Group, push notification guidelines. https://www.nngroup.com/articles/push-notification/

Product documentation and reporting:

- [wwdc]: WWDC21 10091 notes, "Send communication and Time Sensitive notifications". https://wwdcnotes.com/documentation/wwdc21-10091-send-communication-and-time-sensitive-notifications/
- [batch]: Batch, WWDC 2021 push notification announcements. https://batch.com/blog/posts/wwdc-2021-major-push-notifications-announcements (second-hand: search summary only, page not fetched or not rendered)
- [tc-ios15]: TechCrunch, "Apple refines iOS 15 notifications with Focus, Summary features". https://techcrunch.com/2021/06/07/apple-refines-ios-15-notifications-with-focus-summary-features/
- [apple-summary]: Apple Support, "View and respond to notifications on iPhone". https://support.apple.com/en-by/guide/iphone/iph6534c01bc/ios (second-hand: search summary only, page not fetched or not rendered)
- [hig-live]: Apple Human Interface Guidelines, Live Activities. https://developer.apple.com/design/human-interface-guidelines/live-activities (second-hand: search summary only, page not fetched or not rendered)
- [macworld-glances]: Macworld, "watchOS 3 FAQ: Glances are going away". https://www.macworld.com/article/228206/watchos-3-faq-glances-are-going-away-as-watch-apps-speed-up-and-move-to-a-dock.html
- [tc-pause]: TechCrunch, "Apple pauses AI notification summaries for news after generating false alerts". https://techcrunch.com/2025/01/16/apple-pauses-ai-notification-summaries-for-news-after-generating-false-alerts
- [siliconrepublic]: Silicon Republic, "Apple calls a halt to AI notification summaries amid backlash". https://www.siliconrepublic.com/business/apple-halt-ai-notification-summaries-backlash-features (second-hand: search summary only, page not fetched or not rendered)
- [idb]: iDownloadBlog, "Apple to fix incorrect Apple Intelligence notification summaries". https://www.idownloadblog.com/2025/01/06/apple-promises-fix-incorrect-apple-intelligence-notification-summaries/ (second-hand: search summary only, page not fetched or not rendered)
- [techradar]: TechRadar on the breakup-text summary. https://www.techradar.com/phones/iphone/whats-worse-than-a-break-up-by-text-this-savage-apple-intelligence-dumping (second-hand: search summary only, page not fetched or not rendered)
- [parameter]: Parameter, "Android 16 Updates Include AI Summaries and Notification Organizer". https://parameter.io/android-16-ai-notification-summaries/ (second-hand: search summary only, page not fetched or not rendered)
- [organizer]: 9to5Google, "Google rolling out Notification Organizer to Pixel". https://9to5google.com/2025/12/09/google-pixel-notification-organizer/
- [android-channels]: Pushwoosh, Android push notifications and channels. https://www.pushwoosh.com/blog/android-push-notifications/ (second-hand: search summary only, page not fetched or not rendered)
- [gh-inbox]: GitHub Docs, "Managing notifications from your inbox". https://docs.github.com/en/subscriptions-and-notifications/how-tos/viewing-and-triaging-notifications/managing-notifications-from-your-inbox
- [linear]: Linear Docs, Inbox. https://linear.app/docs/inbox
- [linear-snooze]: Linear changelog, inbox snooze. https://linear.app/changelog/2021-06-17-inbox-snooze-and-easier-issue-merge (second-hand: search summary only, page not fetched or not rendered)
- [hey]: HEY features. https://www.hey.com/features/
- [shortwave]: Shortwave Docs, Bundles. https://www.shortwave.com/docs/guides/bundles/
- [sanebox]: SaneBox Help, Daily Digest guide. https://www.sanebox.com/help/170-daily-digest-how-to-guide
- [gmail]: Gmail Help, inbox categories. https://support.google.com/mail/answer/3094499
- [mailbird]: Mailbird, citing the ZeroBounce Promotions tab survey. https://www.getmailbird.com/gmail-inbox-tabs-stop-important-emails-going-to-promotions/ (second-hand: search summary only, page not fetched or not rendered)
- [axios]: Axios HQ, Smart Brevity 101. https://axioshq.com/hubfs/smart-brevity-101.pdf (second-hand: search summary only, page not fetched or not rendered)

- [statuspage]: GitHub Status, a Statuspage-style page. https://www.githubstatus.com/

Not covered (WebSearch budget spent; Exa and Brave keys rejected):
Feedly and Inoreader mark-all-read behaviour, Slack's activity view,
Google Now cards, Datadog dashboards, and Stripe or Amplitude digest
emails. The proposal does not depend on them.

[fitz]: https://static1.squarespace.com/static/57a40c19414fb54f51f8095f/t/5d2e7f8efcb18b0001bd506d/1563328398940/2019+Fitz-Kushlev-etal+2019.pdf
[iqbal]: https://www.semanticscholar.org/paper/Notifications-and-awareness:-a-field-study-of-alert-Iqbal-Horvitz/700768a496da0595b0bc999e2f126f5a29a13a2b
[mark2016]: https://www.microsoft.com/en-us/research/wp-content/uploads/2016/06/Email20Duration20Camera20Ready20submission3-1.pdf
[mark2012]: https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%202012.pdf
[pielot]: https://www.interruptions.net/literature/Pielot-MobileHCI18.pdf
[mehrotra]: https://etheses.bham.ac.uk/id/eprint/7440/1/Mehrotra17PhD.pdf
[sre]: https://sre.google/sre-book/monitoring-distributed-systems/
[nng]: https://www.nngroup.com/articles/push-notification/
[wwdc]: https://wwdcnotes.com/documentation/wwdc21-10091-send-communication-and-time-sensitive-notifications/
[batch]: https://batch.com/blog/posts/wwdc-2021-major-push-notifications-announcements
[tc-ios15]: https://techcrunch.com/2021/06/07/apple-refines-ios-15-notifications-with-focus-summary-features/
[apple-summary]: https://support.apple.com/en-by/guide/iphone/iph6534c01bc/ios
[hig-live]: https://developer.apple.com/design/human-interface-guidelines/live-activities
[macworld-glances]: https://www.macworld.com/article/228206/watchos-3-faq-glances-are-going-away-as-watch-apps-speed-up-and-move-to-a-dock.html
[tc-pause]: https://techcrunch.com/2025/01/16/apple-pauses-ai-notification-summaries-for-news-after-generating-false-alerts
[siliconrepublic]: https://www.siliconrepublic.com/business/apple-halt-ai-notification-summaries-backlash-features
[idb]: https://www.idownloadblog.com/2025/01/06/apple-promises-fix-incorrect-apple-intelligence-notification-summaries/
[techradar]: https://www.techradar.com/phones/iphone/whats-worse-than-a-break-up-by-text-this-savage-apple-intelligence-dumping
[parameter]: https://parameter.io/android-16-ai-notification-summaries/
[organizer]: https://9to5google.com/2025/12/09/google-pixel-notification-organizer/
[android-channels]: https://www.pushwoosh.com/blog/android-push-notifications/
[gh-inbox]: https://docs.github.com/en/subscriptions-and-notifications/how-tos/viewing-and-triaging-notifications/managing-notifications-from-your-inbox
[linear]: https://linear.app/docs/inbox
[linear-snooze]: https://linear.app/changelog/2021-06-17-inbox-snooze-and-easier-issue-merge
[hey]: https://www.hey.com/features/
[shortwave]: https://www.shortwave.com/docs/guides/bundles/
[sanebox]: https://www.sanebox.com/help/170-daily-digest-how-to-guide
[gmail]: https://support.google.com/mail/answer/3094499
[mailbird]: https://www.getmailbird.com/gmail-inbox-tabs-stop-important-emails-going-to-promotions/
[axios]: https://axioshq.com/hubfs/smart-brevity-101.pdf
[statuspage]: https://www.githubstatus.com/
