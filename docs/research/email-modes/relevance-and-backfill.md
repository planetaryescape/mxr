# Relevance and backfill: stale items and the first run are one problem

Research for "Items have a relevancy window, and the first run uses it" in
[22-email-modes.md](../../blueprint/22-email-modes.md). Code references are
at `3da0c119` (`origin/main`, v0.6.47), which is the merge base of
`docs/site-modes`. Mailbox counts come from BK's real store
(`~/Library/Application Support/mxr/mxr.db`), opened read-only on
2026-10-02; only counts were read, no subjects, senders or bodies.

Each section keeps evidence and inference apart. "Evidence" means a cited
source or a measured count. "Inference" means this note's reasoning, which
needs dogfooding to confirm.

BK raised two problems on 2026-10-02:

1. "when we do the first classification I imagine there'll be tonnes of
   incoming todos and all that of old things. we need to think about how
   we'll handle that"
2. "one thing that annoys me about apple notifications is that they don't
   dismiss even after the thing is clearly stale. For example you can't
   still be showing me a notification for a calendar event that has
   already passed. Notifications have a relevancy period"

The hypothesis under test was that both are one mechanism: every mode
item carries a relevancy window derived from its content, expired items
leave the attention surfaces on their own, and the first classification
over years of mail mostly resolves itself because most old items are
already past their window. The evidence supports the first two parts and
qualifies the third. Windows clear everything with a date. They do not
clear the large undated residue, and on BK's mail that residue is the
bulk of the flood.

## 1. BK's own store already shows both problems, and the undated residue is the bigger one

Evidence (counts on BK's real store, 2026-10-02):

| What | Count |
|---|---:|
| Messages in the store | 110,285 |
| Received in the last 30 days | 1,518 (1.4%) |
| Older than one year | 94,806 (86%) |
| Deliveries listed as active (not delivered, not dismissed) | 83 |
| of those, no event for more than 30 days | 63 |
| Calendar invites asking for an RSVP and still unanswered | 50 |
| of those, with the event already over | 50 |
| Open promises the user made (`contact_commitments`, `direction = 'yours'`) | 1,842 |
| of those, with no date (`by_when IS NULL`) | 1,705 (93%) |
| of those, evidence message older than 30 days | 1,447 |
| of those, evidence message in the last 14 days | 90 |

The stale items exist because nothing in the code ends them by time.
The active deliveries list is `WHERE dismissed_at IS NULL AND
delivered_at IS NULL` with no age bound
(`crates/store/src/deliveries.rs`, the `DeliveryListFilter::Active`
query), so a parcel whose "delivered" email never came stays "in transit"
forever. Promises expire only after 180 days and only when nobody wrote in
the thread since (`expire_stale_contact_commitments`, called from
`crates/relationship/src/commitments.rs` with `Duration::days(180)`). The
desk is the one surface that already uses a window: `DESK_WINDOW_DAYS =
30`, `RECENT_DAYS = 7` and `SCREENER_NEW_SENDER_DAYS = 14` in
`crates/daemon/src/handler/desk_lanes.rs`. That window is one number for
everything.

Inference: blueprint 22 has `ListTodos` merge promises into To do. Run
as specified on this store, To do would open with 1,842 promises and 50
RSVPs for events that already happened. Relevancy windows remove the 50
RSVPs and the 63 quiet parcels outright, because each has a date. They
remove almost none of the promises, because 93% have no date. The first
run therefore needs two mechanisms: windows for dated items, and a
bounded catch-up for undated ones. Even the catch-up is too big on this
mail: 90 undated promises from the last 14 days is more than anyone will
review, which says promise extraction precision is the first run's main
risk, not the window logic.

## 2. Platforms that let content expire tie the expiry to the thing, not to the notification

### Android cancels a notification after a duration the app sets

Evidence: `Notification.Builder.setTimeoutAfter(long durationMs)`, added
in API level 26, "Specifies a duration in milliseconds after which this
notification should be canceled, if it is not already canceled"
([Android reference](https://developer.android.com/reference/android/app/Notification.Builder)).

### iOS has no expiry on a notification; the app must remove it

Evidence: `UNNotificationContent` exposes title, body, attachments,
badge, sound, `interruptionLevel`, `relevanceScore`, `filterCriteria`
and grouping fields, and no expiry or end date (topic list of
[UNNotificationContent](https://developer.apple.com/documentation/usernotifications/unnotificationcontent),
read through Apple's documentation JSON). Removal is the app's job:
`removeDeliveredNotifications(withIdentifiers:)` "Removes your app's
notifications from Notification Center that match the specified
identifiers"
([Apple](https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/removedeliverednotifications(withidentifiers:))).
`relevanceScore` is "a value between 0 and 1" used "to sort the
notifications from your app"; "the highest score gets featured in the
notification summary"
([Apple](https://developer.apple.com/documentation/usernotifications/unnotificationcontent/relevancescore)).
It ranks; it does not expire. `timeSensitive` notifications "can break
through system controls such as Notification Summary and Focus"
([Apple](https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel/timesensitive)).
The APNs `apns-expiration` header governs how long Apple stores an
undelivered push ("30 days or less"), not how long a delivered one stays
on screen ([Apple](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns)).

Inference: this explains BK's complaint precisely. A stale calendar
notification on iPhone stays because iOS gives notifications no end time
and leaves cleanup to each app; an app that doesn't call
`removeDeliveredNotifications` leaves it there. Android's API puts the
duration on the notification itself. mxr owns both ends, so it can do
better than either: derive the end from the content and enforce it
centrally.

### Live Activities have a stale date and a bounded dismissal

Evidence: `ActivityContent.staleDate` is "The date when the system
considers the Live Activity to be out of date"; at that time "the
ActivityState of the Live Activity changes to stale"
([Apple](https://developer.apple.com/documentation/activitykit/activitycontent/staledate)).
With the default dismissal policy, "the system keeps a Live Activity that
ended on the Lock Screen for up to four hours after it ends"
([Apple](https://developer.apple.com/documentation/activitykit/activityuidismissalpolicy/default)).
"A Live Activity can be active for up to eight hours"; after that the
system ends it and "a Live Activity remains on the Lock Screen for a
maximum of 12 hours"
([Apple, Displaying live data](https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities)).

Inference: Apple separates three moments mxr also needs: stale (the data
is old but the thing may still be live), ended (the thing is over) and
dismissed (gone from view). A parcel with no carrier email for a week is
stale; delivered is ended; filed to Archive is dismissed.

### Widgets and Wallet passes declare when they are relevant, and expired passes hide themselves

Evidence: on watchOS, `RelevantContext.date(from:to:)` "Tells the system a
widget is relevant between two dates"
([Apple](https://developer.apple.com/documentation/relevancekit/relevantcontext/date(from:to:))),
and RelevanceKit "is only available in watchOS"
([Apple](https://developer.apple.com/documentation/relevancekit/relevantcontext)).
WidgetKit's `TimelineEntryRelevance` carries "a score and a duration";
"for the duration you specify, WidgetKit may rotate your widget to the top
of the stack"
([Apple](https://developer.apple.com/documentation/widgetkit/timelineentryrelevance)).
A Wallet pass has `expirationDate` ("The date and time the pass
expires"), `relevantDates` ("date intervals that the system uses to show
a relevant pass", with a `startDate` and `endDate`) and `voided` ("the
pass is void, such as a redeemed, one-time-use coupon")
([Apple, Pass](https://developer.apple.com/documentation/walletpasses/pass)).
On the device, "Some limited or single-use passes, such as boarding
passes or event tickets, are automatically hidden after they expire.
These passes are moved to a separate list, where you can view, unhide, or
delete them permanently", and "You can turn off the Hide Expired Passes
feature"
([Apple Support, March 2026](https://support.apple.com/en-us/102544)).

Inference: Wallet is the closest product model for mxr. The expiry comes
from the issuer's data, expired items leave the main view without the
user acting, nothing is deleted, a separate list holds them with unhide,
and the whole behaviour can be switched off.

### Gmail already reads deal expiry from email and uses it to resurface and to retire offers

Evidence: Gmail's Promotions annotations define `DiscountOffer` with
`discountCode`, `availabilityStarts` and `availabilityEnds` ("The end
date and time of the promotion")
([Google, reference](https://developers.google.com/workspace/gmail/promotab/reference)).
The best-practices page says an expiration date gives an email "two
opportunities to preview at the top in a bundle: once when you first send
it and again within three days of the offer expiring", and "Don't leave an
expired date in the annotation, because Gmail treats it as an old offer
and doesn't populate the email into a bundle"
([Google, best practices](https://developers.google.com/workspace/gmail/promotab/best-practices)).

Inference: the biggest mail provider already runs the exact mechanism for
one kind of email: a window from the content, a resurfacing moment near
its end, and exclusion after it.

### Schema.org gives a machine-readable end for most kinds of mail mxr cares about

Evidence: `Event.endDate`, "The end date and time of the item"
([schema.org/Event](https://schema.org/Event)); `Offer.validThrough`,
"The date after when the item is not valid. For example the end of an
offer" ([schema.org/Offer](https://schema.org/Offer));
`ParcelDelivery.expectedArrivalUntil`, "The latest date the package may
arrive" ([schema.org/ParcelDelivery](https://schema.org/ParcelDelivery));
`Invoice.paymentDueDate`, "The date that payment is due", and
`paymentStatus` ([schema.org/Invoice](https://schema.org/Invoice)).
mxr already parses schema.org for deliveries
(`crates/deliveries/src/schema_org.rs`, which reads `eta_until`) and
stores ICS `starts_at` and `ends_at` for invites
(`crates/store/src/calendar.rs`).

### Codes and links carry their own short lifetimes

Evidence: NIST SP 800-63B: "Confirmation codes sent by means other than
physical mail SHALL be valid for a maximum of 10 minutes", and
out-of-band authentication is invalid "if not completed within 10
minutes" ([NIST SP 800-63B](https://pages.nist.gov/800-63-3/sp800-63b.html)).
Framework defaults for emailed links vary widely: Django's
`PASSWORD_RESET_TIMEOUT` defaults to "259200 (3 days, in seconds)"
([Django settings](https://docs.djangoproject.com/en/stable/ref/settings/));
Devise defaults `reset_password_within` to 6 hours and `confirm_within`
to unlimited (`lib/devise.rb` in
[heartcombo/devise](https://github.com/heartcombo/devise/blob/main/lib/devise.rb)).

Inference: a one-time code needs a minutes-scale window, and the stated
expiry ("expires in 10 minutes") should win over any default. A verify
link's default is a guess between hours and days, so the stated expiry
matters more there.

### Google Now's card expiry could not be confirmed

Evidence: Wikipedia confirms only that Now cards showed context-dependent
information and were replaced by the Feed in October 2016
([Wikipedia](https://en.wikipedia.org/wiki/Google_Now)). Pocket-lint's
2014 report on bill cards lists biller, amount and due date but says
nothing about when cards left
([Pocket-lint](https://www.pocket-lint.com/apps/news/google/128940-google-now-adds-bill-reminder-cards-pulled-from-gmail-so-you-can-pay-bills-on-time/)).
The claim that Now cards disappeared once stale stays unverified and is
not used below.

## 3. Stale reminders are a known annoyance, but the evidence is thin and mostly anecdotal

Evidence:

- BK's own report (above).
- A 2018 Hacker News comment about GitLab to-dos: when a merge request
  "is merged in, then I don't want it to show up on my todos... It is no
  longer a todo but a done. Unfortunately todos doesn't work this way and
  become useless for senior members"
  ([HN 17675447](https://news.ycombinator.com/item?id=17675447)).
- A 2021 Hacker News comment naming as the one thing they'd miss from
  apps that "apps can dismiss notifications that are no longer relevant"
  ([HN 25764726](https://news.ycombinator.com/item?id=25764726)).
- Pielot, Vradi and Park analysed 794,525 notifications from 278 users;
  non-messaging notifications "rarely lead to a conversion (rates between
  ca. 15 and 25%)"
  ([MobileHCI 2018](https://www.interruptions.net/literature/Pielot-MobileHCI18.pdf)).
  Most notifications are never acted on, so most sit until dismissed.

No study measuring annoyance at stale notifications specifically was
found. Search was limited (no working web search API; HN Algolia and
direct fetches only), so absence here is weak evidence of absence.

Inference: the case for windows rests mainly on platform design (Android,
ActivityKit, Wallet, Gmail all built expiry) and on BK's stated
preference, not on a user study.

## 4. Products facing a backlog either refuse to import it, let the user bulk-clear it, or age it out

### HEY doesn't import old mail at all

Evidence: "HEY does not import your existing email from Gmail, Outlook,
iCloud. HEY is a fresh start moving forward. Your old email will remain
wherever it was"
([HEY FAQ](https://www.hey.com/faqs/)). When a sender is screened back in,
"we'll show you any email they sent you within the last 90 days"
([HEY Screener](https://www.hey.com/features/the-screener/)). HEY's Cover
Art slides "a cover... over your previously seen emails"
([HEY features](https://www.hey.com/features/)). No feature named "Imbox
clean start" was found on HEY's features page or FAQ; the clean start is
the no-import policy itself.

### Superhuman makes clearing history one command with a seven-day undo

Evidence: "Getting to Inbox Zero the first time is the hardest because
most of us have thousands of old email sitting in our inbox. ... Hit Cmd+K
or Ctrl+K → Get Me To Zero to archive old emails". The user picks "a time frame",
can keep Unread or Starred messages, and "You can undo Get Me To Zero at
any time within seven days". The guide suggests running it again "to
clean up all of your Unread messages older than a month"
([Superhuman, Achieve Inbox Zero](https://help.superhuman.com/hc/en-us/articles/46005833597709-Achieve-Inbox-Zero);
[Mass Archive](https://help.superhuman.com/hc/en-us/articles/46005611576589-Mass-Archive),
both read through Superhuman's Zendesk search API, updated August and
September 2026).

### SaneBox learns from history but sorts new mail

Evidence: "Our robots analyze your past interactions with your email to
figure out what's important to you. As email comes into your Inbox, we
determine its importance"; SaneLater is "where all of your new
unimportant email can be found"; it reads headers only
([SaneBox FAQ](https://www.sanebox.com/faq)). Its Email Cleanup Tool
"lists recent emails from your Inbox and active training folders" to
train on important senders
([SaneBox help 140](https://www.sanebox.com/help/140)). How SaneBox
treats mail already in the inbox on day one was not documented in any
page fetched.

### Linear lets the importer choose active-only and skips what it already imported

Evidence: "Some organizations choose to use Linear as a 'clean break'
from their legacy tool and import only where absolutely necessary"; the
importer lets you "Choose which issues to import, including active-only
or broader sets such as stale, completed, or all issues"; and
"Reimporting from the same external source to the same Linear team
without deleting the initial import first will skip any already-imported
issues" ([Linear docs](https://linear.app/docs/import-issues)).

### Sunsama ages stale tasks out with a visible count

Evidence: incomplete tasks roll over each day, and "The auto-archive
feature automatically captures incomplete tasks that have rolled over
multiple consecutive days"; "A message appears at the bottom of your task
list reading 'N Tasks moved to archive'"; the threshold is adjustable and
"If important tasks keep getting archived, expand the threshold from four
days or disable auto-archive entirely". Edited recurring instances are
"exempt from rollover removal"
([Sunsama, rollover basics](https://help.sunsama.com/docs/getting-started/basics/task-rollover-and-recurring-tasks-the-basics);
[Sunsama, archive](https://help.sunsama.com/docs/usage-guides/archive)).
The backlog sorts someday tasks into time buckets from "Next week or two"
to "Never" ([Sunsama, backlog](https://help.sunsama.com/docs/backlog)).

### Todoist offers one button for all overdue tasks, because a pile of overdue tasks drives people away

Evidence: "Overdue tasks: Click Reschedule to the right of the Overdue
section"
([Todoist help](https://www.todoist.com/help/todoist/features/schedule-a-date-and-time-for-your-todoist-tasks-q7VobO)).
Todoist's own blog: "At around 50 overdue tasks, I close out of Todoist
on my Mac to avoid seeing the number of things I'm not doing", and "we let
one overdue task become 5, then 10, then 100+. We abandon the system right
when we need it the most"
([Todoist, GTD tips](https://www.todoist.com/inspiration/gtd-tips)).

### Email bankruptcy is the manual version of a window plus a catch-up

Evidence: "Email bankruptcy is deleting or ignoring all emails older than
a certain date"; "a message is usually sent to all senders explaining...
that if their message still requires a response they should resend";
the term is usually attributed to Lawrence Lessig in 2004
([Wikipedia](https://en.wikipedia.org/wiki/Email_bankruptcy), citing the
Washington Post, 25 May 2007, which returned 403 to the fetch; Lessig's
original post now 404s). Fred Wilson in May 2010 had over 1,200 unread,
cleared it to 800, then found his roughly 30 most important contacts,
searched for their mail to make sure he'd answered it, and archived the
rest: "If it is important that I see your email, please email me again"
([AVC, 11 May 2010](https://avc.com/2010/05/email-bankruptcy/)).

### Things imports were not researched successfully

No Things support page on importing or on overdue handling was found by
direct fetch. Not used below.

Inference across this section: every product that handles a backlog
well does three things. It bounds the window (HEY: nothing; HEY Screener:
90 days; Superhuman: a time frame the user picks; Sunsama: four days;
Wilson: the five weeks he'd fallen behind), it saves what matters by a
cheap signal first (HEY: screened-in senders; Superhuman: unread and
starred; Wilson: top 30 people; Linear: active only), and it makes the
bulk action reversible
(Superhuman: seven days; Sunsama: the archive list; Wallet: unhide). None
asks the user to triage history one item at a time. Linear's skip on
re-import is the precedent for re-runs never re-flooding.

## 5. Classifiers start cold from a global model and learn the user fast; mxr can start warm from history

Evidence: Gmail Priority Inbox combines a global model with a per-user
model: "A glut of data exists for learning a global model, but a dearth of
data exists for learning personalized user models. We use a simple form of
transfer learning where the final prediction is the sum of the global
model and the user model log odds." Corrections and new users learn
faster: "a manual correction by a user is given a higher value of C. User
models also have higher C than the global model, and new user models have
higher values still to promote initial learning." Thresholds are per user
because "Our experience showed a huge variation between user preferences
for volume of important mail"; "When a user marks messages in a
consistent direction, we perform a real-time increment to their
threshold." Error on user-marked mail fell from 45% (global) to 38% (user
models) to 31% (user models and thresholds), and Priority Inbox users
"spent 6% less time reading mail overall, and 13% less time reading
unimportant mail"
([Aberdeen, Pacovsky and Slater, 2010](https://research.google/pubs/pub36955/),
PDF read in full).

Label propagation is the standard semi-supervised method where "a
(generally small) subset of the data points have labels" and "these labels
are propagated to the unlabeled points"
([Wikipedia](https://en.wikipedia.org/wiki/Label_propagation_algorithm);
Zhu and Ghahramani's 2002 report was fetched but its PDF text could not be
extracted).

Reply timing decays fast. Kooti et al., 16 billion emails from more than 2
million users: "more than 90% happen within a day of receiving the
message, and the most likely reply time is just two minutes. Also, half of
the replies are within 47 minutes"
([Kooti et al., WWW 2015, arXiv 1504.00704](https://arxiv.org/abs/1504.00704)).

Inference:

- mxr's base mode is already a per-sender label (`mail_kind::classify`
  plus screener dispositions). One sender decision placing all that
  sender's mail is label propagation along the sender edge, and it is why
  history is cheap: thousands of messages share a few hundred senders.
- Unlike Gmail's new users, mxr starts with the whole history locally:
  reply pairs, contacts, cadence. That is the "free signals" positioning in
  practice. The first run can learn who matters from history (as SaneBox
  does) without surfacing history.
- Classify newest first so the user's first corrections land on mail they
  remember, and so corrections (which Priority Inbox weights higher)
  propagate backwards through the sender before history is processed.
- An owed reply older than a few weeks is past the period in which almost
  all replies happen. Messages should not expire, but "your turn" should
  decay.

## 6. Sync order today works against "newest first"

Evidence: IMAP initial sync runs `UID SEARCH ALL`, then
`uids.sort_unstable()` and fetches in ascending UID order, oldest first,
collecting the whole folder before returning
(`crates/provider-imap/src/lib.rs`, around line 578). Gmail pages with a
`page_token` backfill cursor (`crates/provider-gmail/src/cursor.rs`) over
`messages?maxResults=` with no ordering parameter
(`crates/provider-gmail/src/client.rs`); Google does not document an
order guarantee. `post_sync_fanout` (`crates/daemon/src/loops.rs`) runs
rules and the delivery scan on every page, including backfill pages, and
defers only relationship indexing and the voice profile while
`initial_backfill_in_progress`.

Inference: "Now is useful in minutes" can't rely on sync order. The
classification queue must order by message date itself, and the IMAP
adapter should page newest first if the first IMAP sync is to show
anything before the whole folder is fetched. Detectors that create rows
(deliveries today, to-dos later) must apply windows on backfill pages
too, or the backfill recreates exactly the 63 quiet parcels counted
above.

## 7. Proposed windows, by kind

Inference throughout, grounded where marked. The window is code: dates
come from schema.org, ICS and `natural_time` over quoted words, never
from a model, matching D111. A model may only pick among dates code
found.

| Kind | Relevant from | Relevant until | Where the end comes from | After the window |
|---|---|---|---|---|
| Event or invite | arrival | event end | ICS `DTEND` (`calendar_invites.ends_at`), `Event.endDate`; no end: start plus 1 hour | RSVP to-do expires; the event is a record only if accepted |
| One-time code | arrival | stated lifetime, else arrival plus 10 minutes | quoted words ("expires in 10 minutes"); NIST's 10-minute maximum as default | let go; never a record |
| Verify or confirm link | arrival | stated expiry, else arrival plus 3 days | quoted words; Django's 3-day default | let go; never a record |
| Sign-in or security alert | arrival | arrival plus 2 days | judgement: long enough to see it twice | let go; stays searchable in Inbox |
| Delivery | first email | delivered plus 1 day; quiet when no event for 7 days past `expectedArrivalUntil`, or 14 days with no ETA | carrier status, `ParcelDelivery.expectedArrivalUntil` | delivered: order record in Archive; quiet: "went quiet" in Updates once, then let go |
| Bill | `surface_at` | due plus 14 days unless the user touched it | `Invoice.paymentDueDate`, due words | expires; a reminder or final notice in the thread reopens it by dedup key; paid: record |
| Renewal | `surface_at` | renewal date plus 3 days | due words, schema.org | expires (renewed or lapsed); confirmation becomes a record |
| RSVP | arrival | reply-by date, else event start | due words, ICS | expires; Archive keeps the event only if accepted |
| Sale or offer | arrival or `validFrom` | `Offer.validThrough`, `availabilityEnds`, quoted end; else 7 days | schema.org, Gmail annotations, words | let go; never a record |
| Newsletter issue | arrival | Reading's per-source fade (twice the median interval, 2 to 14 days) | existing D112 rule | `mode_done` for Reading; Later never expires silently |
| Promise you made | evidence date | dated: date plus 7 days; undated: no end, decays like owed | `by_when`, due words | dated: expires with "was due"; undated: catch-up rules |
| Person message | arrival | none | messages don't expire | "your turn" decays to Quiet after max(3 times their cadence, 7 days), capped at 30 days |
| Record | none | none | records don't expire | a record's "coming up" moment has its own window (return window close, trip end) |

Grace periods (bill plus 14 days, renewal plus 3) are judgement, not
sourced. The 14-day bill grace exists because a missed bill keeps
mattering until the biller says otherwise, and the biller's next email
reopens it.

## 8. What expiry should do

Inference:

- **Expiry is a state, not a deletion.** To-dos gain `state = 'expired'`
  with `expired_at`; update facts gain `relevant_until`; Reading already
  writes `mode_done`. Nothing is deleted (Wallet, Sunsama, Superhuman all
  keep what they clear).
- **Expiry leaves To do, Updates and Now without user action.** Archive
  receives what was record-worthy (an accepted event, a delivered order, a
  paid bill), as it already does on tick-off.
- **Expiry never touches what the user made or changed.** A to-do with
  `origin` of `manual` or `handoff`, any field the user edited
  (`field_sources` value `user`), or a `scheduled_for` date stays until
  the user acts, showing "was due". Sunsama exempts edited instances from
  rollover removal for the same reason.
- **Expiry is local.** It writes mode state only and never archives at the
  provider. Provider archive stays tied to a user action (done, let go, or
  the catch-up's let-go-all with its preview), so a background job can
  never move thousands of messages in Gmail unseen (D098).
- **Visibility is one quiet line, not a badge.** To do and Updates show
  "3 expired since you last looked" when the count is above zero, opening
  an Expired list with restore (`u` or Enter), as Sunsama's "N Tasks moved
  to archive" and Wallet's Expired list do. Now shows nothing about expiry;
  badges count work only.

## 9. The first run

Inference, using the evidence above:

1. **Classify newest first.** A dated queue, independent of sync order.
   The first slice (the last 14 days; 917 messages on BK's store)
   goes through rules and the fast tier before anything else, so Now has
   its People, Due soon and Updates within minutes of the sync reaching
   that slice.
2. **History runs in the background with progress.** Newest to oldest,
   resumable, behind the first slice: "Sorting your history: 2023, 61%".
3. **Windows apply during classification.** An item whose window already
   ended is written as expired at birth: `expired_at` set, `surfaced_at`
   never set, not counted in "expired since you last looked". One summary
   line after the run says what the window cleared ("50 past invites and
   63 quiet parcels were already over").
4. **The undated residue gets one bounded catch-up.** Open-ended items
   (undated to-dos and promises, owed replies) from the last 14 days only
   form one reviewable batch, shown once: "Catch up: 12 things from the
   last two weeks might still need you". Each row keeps or lets go with
   one key, and "let go of all" has a dry-run preview that equals the
   commit, with undo. At most 25 rows, ranked by act-by then closeness;
   the rest join the expired list with a count. Older open-ended items are
   expired at birth with the reason "older than your catch-up window".
   Fourteen days rests on Kooti et al.'s reply decay, sits inside
   Superhuman's one-month suggestion and the five weeks of mail Wilson
   declared bankrupt, and well inside HEY's 90; 25 sits under the roughly
   50 overdue items at which Todoist's own writer gave up.
5. **Hard rule: nothing past its window enters Now, ever.** Not on the
   first run, not after it. For open-ended items, nothing older than the
   catch-up window enters Now on the first run.
6. **History beyond 90 days is rules only by default.** Windows make model
   output on old to-dos and updates worthless, since almost all of it would
   be expired at birth. Rules and schema.org still run over everything,
   because Archive records never expire and sender labels propagate. The
   fast tier runs over the last 90 days (4,605 messages on BK's
   store). Arithmetic, not measured: at an assumed 600 input tokens per
   message, the full history of 110,285 messages is about 66 million
   tokens and the 90-day slice about 2.8 million. Smart-tier record fields
   for older mail run lazily, when Archive opens or answers about that
   record. A user can extend the model horizon explicitly.
7. **Re-runs never re-flood.** Every item keeps a stable claim: to-dos
   by `dedup_key` with `surfaced_at` and `expired_at`, update facts with a
   breakthrough claim. A new rule or recipe version re-evaluates fields but
   never clears `surfaced_at`, `expired_at` or a user decision, and new
   items it finds in old mail go through the same window and catch-up cap
   as the first run (Linear's skip on re-import).

## 10. Tests and the user-visible check

Inference:

- Window table tests per kind: ICS `DTEND`, `Event.endDate`,
  `Offer.validThrough`, Gmail `availabilityEnds`, "code expires in 10
  minutes", the 10-minute default, `expectedArrivalUntil`,
  `paymentDueDate` plus grace, and user-touched rows never expiring.
- A first-run handler test with two years of fixture mail on a moved
  clock: Now is populated after the newest slice; zero surfaced items have
  `relevant_until < now`; the catch-up holds at most 25; preview equals
  commit; a re-run with a bumped rule version surfaces nothing already
  expired or surfaced.
- The delivery fix: an in-transit parcel with no event for 14 days and no
  ETA is "went quiet", not active.
- On BK's real mailbox, counts only: the first run puts at most 10 to-dos
  in To do's Now band and at most 25 in the catch-up; zero items past
  their window appear in Now, To do's Now band or an Updates cut; the run
  records expired-at-birth counts per kind against today's baseline (83
  active parcels with 63 quiet, 50 past RSVPs, 1,842 open promises with 90
  in the last 14 days).

## 11. Evidence strength

- Strong, primary and read in full: the platform APIs (Android, iOS,
  ActivityKit, RelevanceKit, WidgetKit, Wallet), Apple's expired-pass
  behaviour, Gmail's deal annotations, schema.org fields, NIST, Priority
  Inbox, Superhuman, HEY, Linear, Sunsama and Todoist help pages, and the
  counts on BK's store.
- Moderate: Kooti et al. (large, but Yahoo mail in 2015, and reply timing
  is a proxy for owed decay); Pielot et al. (phone notifications, not
  email items).
- Weak or missing: studies of stale-notification annoyance (anecdotes
  only); Google Now's expiry (unverified); SaneBox's day-one treatment of
  existing mail (undocumented in what was fetched); Things imports (not
  found).
- Untested judgement: every default in the window table without a source
  (sign-in 2 days, offer 7 days, bill grace 14 days, renewal 3 days), the
  14-day catch-up window, the cap of 25, and the 90-day model horizon.

## Open questions for BK

- Should a detected promise ("I'll send it Friday") expire like a
  detected bill, or is anything you said to a person exempt like a to-do
  you created?
- Is 14 days the right catch-up window for you, or should the first run
  ask you to pick, as Get Me To Zero does?
- Should "expired since you last looked" exist at all, or should expiry be
  silent with the Expired list one key away?
- Should a bill past due plus 14 days expire, or stay as "was due" until
  you act, accepting the pile risk?

## Sources

Fetched and read for this note unless marked. Superhuman's help pages
returned 403 to direct fetch and were read through the Zendesk help
center search API. Pages marked (proxy) were read through r.jina.ai.

Platforms:

- Android. Notification.Builder, setTimeoutAfter. https://developer.android.com/reference/android/app/Notification.Builder
- Apple. UNNotificationContent. https://developer.apple.com/documentation/usernotifications/unnotificationcontent
- Apple. relevanceScore. https://developer.apple.com/documentation/usernotifications/unnotificationcontent/relevancescore
- Apple. UNNotificationInterruptionLevel.timeSensitive. https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel/timesensitive
- Apple. removeDeliveredNotifications(withIdentifiers:). https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/removedeliverednotifications(withidentifiers:)
- Apple. Sending notification requests to APNs. https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns
- Apple. ActivityContent.staleDate. https://developer.apple.com/documentation/activitykit/activitycontent/staledate
- Apple. ActivityUIDismissalPolicy.default. https://developer.apple.com/documentation/activitykit/activityuidismissalpolicy/default
- Apple. Displaying live data with Live Activities. https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities
- Apple. RelevanceKit and RelevantContext.date(from:to:). https://developer.apple.com/documentation/relevancekit/relevantcontext/date(from:to:)
- Apple. TimelineEntryRelevance. https://developer.apple.com/documentation/widgetkit/timelineentryrelevance
- Apple. Wallet Pass. https://developer.apple.com/documentation/walletpasses/pass
- Apple Support. Remove or hide expired passes in Wallet (March 2026). https://support.apple.com/en-us/102544 (proxy)
- Google. Gmail promotions annotation reference. https://developers.google.com/workspace/gmail/promotab/reference
- Google. Gmail promotions best practices. https://developers.google.com/workspace/gmail/promotab/best-practices
- schema.org. Event, Offer, ParcelDelivery, Invoice. https://schema.org/Event https://schema.org/Offer https://schema.org/ParcelDelivery https://schema.org/Invoice
- NIST SP 800-63B. https://pages.nist.gov/800-63-3/sp800-63b.html
- Django settings, PASSWORD_RESET_TIMEOUT. https://docs.djangoproject.com/en/stable/ref/settings/
- Devise, lib/devise.rb. https://github.com/heartcombo/devise/blob/main/lib/devise.rb
- Wikipedia. Google Now. https://en.wikipedia.org/wiki/Google_Now
- Pocket-lint. Google Now bill reminder cards (2014). https://www.pocket-lint.com/apps/news/google/128940-google-now-adds-bill-reminder-cards-pulled-from-gmail-so-you-can-pay-bills-on-time/

Products:

- HEY FAQ. https://www.hey.com/faqs/ (proxy)
- HEY Screener. https://www.hey.com/features/the-screener/ (proxy)
- HEY features. https://www.hey.com/features/ (proxy)
- Superhuman. Achieve Inbox Zero. https://help.superhuman.com/hc/en-us/articles/46005833597709-Achieve-Inbox-Zero
- Superhuman. Mass Archive. https://help.superhuman.com/hc/en-us/articles/46005611576589-Mass-Archive
- Superhuman. Mark Done. https://help.superhuman.com/hc/en-us/articles/47439134613773-Mark-Done
- SaneBox FAQ. https://www.sanebox.com/faq
- SaneBox. How do I train SaneBox? https://www.sanebox.com/help/140
- Linear. Importing guidance. https://linear.app/docs/import-issues (proxy)
- Sunsama. Task rollover basics. https://help.sunsama.com/docs/getting-started/basics/task-rollover-and-recurring-tasks-the-basics (proxy)
- Sunsama. Archive. https://help.sunsama.com/docs/usage-guides/archive (proxy)
- Sunsama. Backlog. https://help.sunsama.com/docs/backlog (proxy)
- Todoist. Schedule a date and time. https://www.todoist.com/help/todoist/features/schedule-a-date-and-time-for-your-todoist-tasks-q7VobO (proxy)
- Todoist. GTD tips. https://www.todoist.com/inspiration/gtd-tips (proxy)

Email bankruptcy:

- Wikipedia. Email bankruptcy. https://en.wikipedia.org/wiki/Email_bankruptcy
- Musgrove, M. "E-Mail Reply to All: 'Leave Me Alone'". Washington Post, 25 May 2007. https://www.washingtonpost.com/wp-dyn/content/article/2007/05/24/AR2007052402258.html (second-hand: cited by Wikipedia, 403 to fetch)
- Wilson, F. "Declaring Email Bankruptcy". AVC, 11 May 2010. https://avc.com/2010/05/email-bankruptcy/

Research:

- Aberdeen, D., Pacovsky, O. and Slater, A. (2010). The learning behind Gmail Priority Inbox. https://research.google/pubs/pub36955/
- Kooti, F., Aiello, L. M., Grbovic, M., Lerman, K. and Mantrach, A. (2015). Evolution of conversations in the age of email overload. WWW 2015. https://arxiv.org/abs/1504.00704
- Pielot, M., Vradi, A. and Park, S. (2018). Dismissed! A detailed exploration of how mobile phone users handle push notifications. MobileHCI 2018. https://www.interruptions.net/literature/Pielot-MobileHCI18.pdf
- Wikipedia. Label propagation algorithm. https://en.wikipedia.org/wiki/Label_propagation_algorithm
- Zhu, X. and Ghahramani, Z. (2002). Learning from labeled and unlabeled data with label propagation. CMU-CALD-02-107. https://mlg.eng.cam.ac.uk/zoubin/papers/CMU-CALD-02-107.pdf (fetched; text not extractable)

Anecdotes:

- Hacker News 17675447 (GitLab to-dos after merge, 2018). https://news.ycombinator.com/item?id=17675447
- Hacker News 25764726 (apps dismissing irrelevant notifications, 2021). https://news.ycombinator.com/item?id=25764726
