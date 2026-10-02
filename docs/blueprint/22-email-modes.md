# 22. Email is five apps, and each one shows an email its own way

Email is five apps sharing one inbox: Messages, To do, Updates, Reading and
Archive. Sorting mail into five lists would give five good categorisations
that still look like a mail client. An email is data, so each mode decides
what unit it shows, what it pulls out of the message, which actions sit up
front, when the user is finished, and even which part of the email it
indexes for search. Messages shows people, To do shows instructions on a
runway, Updates shows a briefing by source, Reading shows readable items,
and Archive shows records. None of them shows a list of subject lines.

Code references are at `3da0c119` (v0.6.47) unless marked. Settled choices
are D107 to D114 in [15-decision-log.md](15-decision-log.md); D112 amends
D110's digest cadence. Rubric v3 in `docs/web-app-experience-rubric.md`
grades the work.

## Research

Each mode's shape comes from a research note with sources, mobile and TUI
mockups and open risks:
[messages.md](../research/email-modes/messages.md),
[todo.md](../research/email-modes/todo.md),
[updates.md](../research/email-modes/updates.md),
[reading.md](../research/email-modes/reading.md),
[archive.md](../research/email-modes/archive.md),
[now-and-handoff.md](../research/email-modes/now-and-handoff.md) (Now, one
email in many modes, handoff, navigation, archive on last done, Screener)
and [chatgpt-sign-in.md](../research/chatgpt-sign-in.md) (the cloud
credential). Where a note and this plan disagree, this plan wins and says
why below.

## Each mode has its own unit, verb, rhythm and end state

| Mode | Unit on screen | Verb up front | Rhythm | End state |
|---|---|---|---|---|
| Now | At most ten items in four fixed sections | Act in the item's mode | Opened first, each session | "Clear. The next to-do surfaces Mon 09:00." |
| Messages | A person, with their topics (threads) inside | Reply, got it, done here | On arrival, two or three visits a day | "Nobody is waiting on you." |
| To do | An instruction (verb + object) on a runway to its act-by date | Do it (one labelled button), tick off | Once a morning, a weekly look ahead on Mondays | "Nothing needs you. Next: renew car insurance shows up Mon 19 Oct." |
| Updates | A source line in a briefing; a tracker for things with state | Let go of the digest | Two fixed cuts a day (08:00, 16:30) | "Nothing new since 08:00. Next digest at 16:30." |
| Reading | A readable item (an issue, or each link in a digest) | Read, later, let go | When the user chooses; no notifications | "Nothing new since Tuesday. Later has 4 things saved." |
| Archive | A record (receipt, order, booking, invoice, trip) | Ask, copy, open the document | On demand, plus "coming up" when a record's date nears | None: records stay |

Inbox stays as the everything view in arrival order. It is a lens, not a
mode, and holds nothing the modes don't. The split matches what field
studies found email doing: Whittaker and Sidner (CHI 1996) named task
management, personal archiving and asynchronous communication as the jobs
one list does badly (now-and-handoff.md, first section).

## One email lives in several modes, each showing its own aspect

The landlord writes "please sign and send it back by 15 Oct. Are you
around Thursday?" with the lease attached. Messages shows Sam Okafor with
"are you around Thursday?" highlighted as the ask. To do shows "Sign lease
renewal, act by Mon 13 · due Wed 15" with Sam as context. Archive shows
"Lease renewal 2026-27, PDF" once it's signed.

Every view keeps an identity anchor (the sender's avatar and the thread
subject in a muted line), and an "Also in" line names the other modes
holding the item: "Also in To do: sign by Wed 15 Oct", with `g` plus the
mode's letter as the jump. This is OOUX's rule that an object keeps its
core properties wherever it appears (now-and-handoff.md §B). Nothing in the
literature tests one email in several views directly, so it is a design
bet (see "Design bets").

## Handoff names where the item went

Each mode keeps its own done state on the `desk_dismissals` watermark
(`051_desk_dismissals.sql`), so done in one mode never clears another and a
new message brings the thread back to that mode only (now-and-handoff.md
§C).

| From | Key | What happens | Toast |
|---|---|---|---|
| Any mode | `e` | Done here; provider archive only when no other mode holds the thread | "Done in Messages. Still in To do (due Wed)." or "Done. Archived in Gmail." |
| Messages, Updates, Reading, Archive | `t` | A to-do prefilled from the ask and due words, in a one-line inline editor | "Added to To do: Sign lease renewal, act by Mon 13. g x" |
| To do | `e` | Tick off; the source's record is filed in Archive with no prompt | "Ticked off. Filed in Archive." |
| Updates | on arrival | A `needs_you` signal (new sign-in, failed payment, delivery exception) goes to To do at once and stays in the next digest marked "already in To do" | none |
| Any mode | `T` | Pass to another mode, from a menu | names the destination |

The destination's rail count ticks up (120 ms) and the item carries "Just
now, from Messages" until the user leaves that mode. NN/g's guidance to
prefer undo over confirmation for routine actions is why none of these
asks first; `u` undoes each.

The provider archive happens when the last mode lets go, by default. The
toast says which happened every time, and Inbox shows a "held by To do"
chip on mail a mode still holds. `modes.archive_on_last_done` (default
`true`) turns it off, after which archive is the palette's "Archive in
Gmail". This is the Superhuman and Shortwave "done is archive" model
applied per mode; Gmail categories never archive, and HEY doesn't need to
because it owns the mailbox (now-and-handoff.md §F).

## Classification is rules first, then the user's model, and every item says why

1. **Sender rules (exist today).** `mail_kind::classify`
   (`crates/daemon/src/handler/mail_kind.rs`) places a sender as Person,
   List or Automated and returns the rule as a reason; a screener
   disposition wins over every rule. Person is Messages, List is Reading,
   Automated is Updates (and Archive when the message is a record), Deny
   is no mode.
2. **Message rules (new).** Deterministic detectors in the pattern of
   `mxr_deliveries::detect`: admin verbs with an object, due phrases,
   schema.org `Invoice`, `Order` and `*Reservation`, receipts, unanswered
   invites, and a thread shape rule (one-to-one, group, copied). They add
   aspects and never remove the base mode.
3. **The user's model (new).** Shortlisted messages go to the configured
   model for what rules can't tell. The model copies due words and
   numbers; the daemon checks they appear verbatim and resolves dates with
   `mxr_core::natural_time`, as `handler/promises.rs` does. Answers are
   cached by prompt version and content hash, as `triage_cache` does. With
   no model, layers 1 and 2 still place everything.

Every item carries a reason ("Here because: asks you to renew, due 14 Oct
(local model)"), and `mxr why <message>` lists every mode with its reason
and source. Corrections are one key: `X` for "doesn't belong here" (this
email) and `K` for the sender's mode and per-sender settings, stored so a
re-run can't undo them.

### Email content goes only to the user's configured model

The daemon's privacy rules carry over: AI is off by default with a local
Ollama default (`LlmConfig::default`); each request pins one provider
(`GistPolicy::pin`); locality comes from the pinned endpoint
(`llm_endpoint_is_local`, backed by `mxr_llm::is_loopback_endpoint`); mail
beyond the message at hand goes to a cloud endpoint only with
`llm.allow_cloud_relationship_data`; mail is wrapped as untrusted data
(`wrap_untrusted_mail`). Background classification adds one stricter
rule, because it reads every incoming message: it runs only against a
loopback endpoint unless the user points the fast tier (below) at a cloud
endpoint and sets `allow_cloud_background_classification = true`. A cloud
model configured for drafting never silently starts classifying all mail.

BK decided on 2026-10-02 that background classification may use a cloud
model when the user chooses it, with the user's own credential; mxr ships
no key and runs no relay. The supported cloud path is the user's own API
key. Sign in with ChatGPT (launched at OpenAI DevDay on 2026-09-29) is
parked. OpenAI publishes no data terms or DPA for plan usage on Plus or
Pro, so it fails the rule that third parties' mail goes only to a processor
whose terms cover it. It also fits background work badly: it spends the
user's shared Codex allowance, rejects `max_output_tokens`, and its 429
gives no reset time ([chatgpt-sign-in.md](../research/chatgpt-sign-in.md),
[docs/issues/chatgpt-plan-usage-data-terms.md](../issues/chatgpt-plan-usage-data-terms.md)).
Revisit if OpenAI publishes API-equivalent terms, and then for on-demand
features first.

### Model work runs in two tiers, and only mail already in a mode reaches the smart one

BK decided this on 2026-10-02 (D114).

| Tier | Work | Endpoint | Volume |
|---|---|---|---|
| Fast | Mode classification, Updates facts for generic subjects, the baseline index extras | The local model by default | Every incoming message |
| Smart | To do fields (verb and object, payee, amount, deadline, act-by, the action link), Archive record fields, Messages' "what they asked" | The user's cloud model when they configured one with their own API key; otherwise local | Only mail already classified into that mode, a small, bounded share |

Without a cloud model the smart tier runs locally and marks its fields
unchecked. Config is two named tiers, `llm.tiers.fast` and
`llm.tiers.smart`, each an `LlmOverrideConfig` (the struct `llm.overrides`
uses today in `crates/config/src/types.rs`, inheriting unset fields from
`[llm]` through `LlmConfig::effective_override`), plus a fixed
feature-to-tier table in code. `llm.overrides` stays for the 14 features
it covers today, so no existing config breaks; precedence is per-feature
override, then tier, then base. The new mode features get no per-feature
override.

Each tier is pinned per request, as `GistPolicy::pin` does. A non-loopback
tier must name an API key (`api_key_env`), which keeps cloud work under API
data terms. Turning a cloud tier on names the tier and shows what it sends
("smart tier: the text of emails in To do and Archive"). Whichever tier
ran, the same guardrails hold: every amount and date appears verbatim in
the email, checked in code like the gist's quote check (`thread_gist.rs`);
dates are normalised by `natural_time`; the provenance chip names the
model; results are cached by message content hash, so each email is
extracted once. The default tier per task is settled by the local versus cloud
extraction eval (`mxr modes eval --extract`), which the user runs on
their own mail with their own key; the rubric records counts, not content.

Cloud for everything is a supported setup, not a workaround. BK decided
on 2026-10-02 that a user who wants no local model must be able to run
every tier in the cloud. Pointing `[llm]` at a cloud endpoint with an API
key and setting `allow_cloud_background_classification = true` sends both
tiers there, and nothing in the modes plan may require a local LLM server
to work. The opt-in screen says plainly what that means: the text of every
incoming message goes to that provider. Embeddings are separate: they run
in-process on the CPU through the bundled semantic models and need no
local LLM server, so a cloud-only user still gets semantic search without
installing anything. Cloud embeddings are not planned; they would send the
whole mailbox to a provider and re-embed it on every model change.

### Each model task gets the tool that fits it, which is often no model

The fit review in [model-fit.md](../research/model-fit.md) assigns every
model task in this plan a tool by fit, not by cost (BK, 2026-10-02: use
things for what they are suited for). Code alone handles the Got it
acknowledgement (a template from the user's own greeting and sign-off),
unsubscribe evidence, Messages ranking, conversation shape and quote
stripping. Where a model judges, code finds the candidates first and the
model selects among them, or answers "none": the amount and deadline among
values found by regex, schema.org and `natural_time`; the request sentence
for "what they asked"; whether a receipt confirms a to-do. Selection keeps
quotes verbatim by construction. Gists, to-do titles, Updates facts and
`mxr ask` generate text and stay on the tiers above; thread gists run on
the smart tier, and its "ask" replaces the existing gist ask rather than
duplicating it.

Auto-actions (completing a to-do on a receipt, an update breaking through
to To do, rendering a model-placed item as checked) ship as suggestions
the user confirms with one key until the eval shows a measured threshold
on the user's own mail. System One models (TypeSafe Jev and the local
models that speak its API) are a candidate in that eval, not a
dependency: Jev is cloud-only and its DPA covers only the customer and the
customer's users, not third parties in mail, so it does not meet the
processor rule as published; a second local daemon is never required. GPT-6
Luna is a reasonable cloud smart-tier model under API terms, but its
stated confidence is not used for thresholds. The OpenAI Decisions API is
in limited preview with no docs or terms; revisit when it has both.
`OpenAiCompatibleProvider` gains JSON-schema structured output regardless,
since selection tasks need it.

When the smart tier is not enough, the task steps up rather than every
call paying for the strongest model (BK, 2026-10-02). `llm.tiers.smart`
takes an optional `escalate` model on the same provider and key. A task
escalates when the smart model answers "none" where code found
candidates, when its pick fails the verbatim check, or when the user
marks a field wrong; the escalated answer is cached like any other and its
provenance chip names the stronger model. Model names are config defaults
the user can change, never hard-coded: on OpenAI today that is GPT-6 Luna
for the smart tier and GPT-6.1 Sol or GPT-6 Astra (the flagship) for
`escalate`, per the OpenAI models page on 2026-10-02.

## Every view follows the same trust rules

- **Model text is italic and names its model.** Apple paused notification
  summaries for news in iOS 18.3 after one put a false claim under the
  BBC's name, then italicised every summary (TechCrunch, Jan 2025;
  updates.md §3). In mxr any sentence a model wrote is italic, says
  "summary by <model>" on hover, and the verbatim text is one key away
  (`o`). `K` can set a source to "never summarise".
- **A model summarises one message at a time.** Counts, latest state and
  totals across messages are code. Apple's errors came from merging
  several notifications into one sentence.
- **Numbers are quoted, deltas are computed.** Every number shown appears
  verbatim in its message. A delta ("up 12% on last week") is computed by
  code against the previous message of the same template, only when both
  values were quoted and the units match (updates.md §4, §7).
- **Every extracted field shows its provenance.** Schema.org, rule, model
  or "you", on hover and in JSON. An unchecked money or date field carries
  an open dot. Expensify and Paperless both report wrong dates and totals
  from extraction (archive.md §3).
- **One-click money links are gated.** A "Pay on camden.gov.uk" button
  appears only when DMARC passes for the sender's domain (`auth_results`
  in `crates/mail-parse/src/lib.rs`) and the link's registrable domain
  matches the sender's or one seen in earlier authenticated mail from
  them. Otherwise the row says "Open email to pay" and shows the raw
  domain. The domain is always visible before Enter, and the TUI footer
  prints it (todo.md §5, §9). How strict the gate is stays BK's call.
- **Badges count work only:** Now, and To do's Now band.

## Now shows at most ten things in four fixed sections

Now replaces the desk. Sections never reorder: People (Your turn, from
Messages), Due soon (To do's Now band, by act-by), the latest Updates cut
as one card, and one Reading pick from 17:00 (Things' This Evening). Each
shows at most three items, then "and 9 more in Messages". Empty sections
disappear; when all are empty the low tide scene (`LowTide.tsx`) plays
with the time the next thing arrives. A long You owe becomes one line:
"11 people are waiting. Three are close; start there." (Sunsama's workload
warning; Cowan's four-chunk working memory; now-and-handoff.md §A.)

```text
+----------+---------------------------------------------------------------+
| Now    3 |  Friday afternoon. 3 people, 2 things to act on.              |
| Messages |  PEOPLE                                     all 11 in Messages |
| To do  2 |  Maya Ortiz     "Can you send the launch checklist?"   22h    |
| Updates  |  Sam (landlord) "Are you around Thursday?"              3h    |
| Reading  |  Iris Chen      "Does the incident note read right?"    1d    |
| Archive  |  DUE SOON                                     all 4 in To do  |
|----------|  Pay council tax  £142.00    act by Wed 7 · due Fri 9        |
| Inbox    |  Sign lease renewal          act by Mon 13 · due Wed 15  Sam  |
| More  >  |  UPDATES  since 08:00 ----------------------------------------|
|          |  2 changed, 1 new sign-in. 23 routine from 9 sources.        |
|          |  ! New sign-in to Google, Chrome on Windows   (in To do)      |
|          |  [Open  g u]                     [Let go of this digest  A]  |
|          |  Not now: Reading 6 this week                                 |
+----------+---------------------------------------------------------------+
  r reply  e done here  t to do  Enter open in its mode  A let go of digest
```

`GetNow` returns the four sections with caps and "more" counts in one
response, so web, TUI and `mxr now --format json` agree; the cap lives in
the daemon, and People shares one owed rule with Messages.

## Messages shows people, with their conversations as topics inside

**Unit.** One row per person; each one-to-one thread is a topic inside
them. A group thread (two or more other humans who took part) is its own
row keyed by `thread_id`, never by participant set, because CC lists
change on almost every reply and Slack and iMessage split a group whenever
someone is added. A CC-only thread you never wrote in, or one with more
than about 10 recipients, goes to Updates (messages.md §4). Importance
attaches to people: reciprocity, recency and longevity predicted contact
importance (Whittaker, Jones and Terveen, CSCW 2002), and SNARF's
per-correspondent sorting improved triage in the field (Fisher et al.).

**Extracted** (messages.md §5): the person, merged across addresses; a
closeness band from `contacts`; whose turn (`owed_replies.rs`); their pace
from `reply_pairs` ("usually 47m"; Tyler and Tang, ECSCW 2003); what they
said with quotes and signatures removed; the ask quoted verbatim; due
words; attachments; incoming Gmail reactions as a mark on the message.
Quote stripping moves into `crates/reader` with thread-aware matching
against earlier messages, because a `>` prefix finds under 10% of reply
lines in business mail (Lampert, Dale and Paris, EMNLP 2009) and the HTML
quote markers live only in the web today (`htmlQuote.ts`).

```text
+-----------+------------------------------+----------------------------------------------+
| Messages  | YOUR TURN                    | Samir Patel            close · usually 47m   |
|           | Samir Patel           16h *  | You've written 48 times since 2023.          |
|           |   "Can you take a look and   |----------------------------------------------|
|           |    reply with the next..."   | TOPICS  Contract renewal     your turn · 16h |
|           | Jon Bell               8h *  |         Launch checklist     waiting · 1d    |
|           |   "Does the pricing copy..." |         with Ruth: Pricing   quiet · 3d      |
|           | Samir, Ruth            2d *  |----------------------------------------------|
|           |   Contract renewal           | Samir · Yesterday 18:02                      |
|           | PINNED  Maya · Ari           | Can you take a look and reply with           |
|           | RECENT                       | [the next concrete step?]   <- the ask       |
|           | Iris Chen        Yesterday   | Read all 6 paragraphs  runbook.pdf           |
|           |   You: "Thanks, on it."      |                     trimmed: quote, sig      |
|           | QUIET (12)              >    |               You · 18:40  Thanks, looking.  |
|           |                              | Reply to Samir · Contract renewal   [ ] all  |
+-----------+------------------------------+----------------------------------------------+
  Mod+Enter send, next   . got it   e done here   t to do   b reply later   c new topic
```

**View and actions.** Length decides shape, not the medium: three lines or
fewer render compactly like chat, longer ones as a letter block with "Read
all N paragraphs". No bubbles around letters, because Spike's are the
format most often called unprofessional for formal mail. Removed text is
marked ("trimmed: quote, sig") with the original on `o`. The composer is
always there, addressed. Got it (`.`) sends a short acknowledgement in
your voice after a visible countdown, instead of reactions, which degrade
into extra email on other clients. Archive, label and move are not on the
surface.

**Rhythm, end state, in and out.** On arrival from close people, two or
three short visits a day. Empty Your turn says "Nobody is waiting on you"
and lists up to three people whose usual cadence lapsed, as a fact, never a
nudge (Gmail's Nudges are mostly written about as something to switch
off). Enters by a Person base mode, a message you send, or a correction.
Leaves Your turn on reply or Got it, leaves Messages on done here until
they write again, and `t` leaves a "to-do: sign the form, due Fri" chip on
the topic.

**Not a list of emails.** The row is a person, ordered by band, then turn,
then time; the preview is their ask; the subject is a topic label; the
state is the turn, not read or unread.

## To do shows instructions on a runway, not emails with flags

**Unit.** One row per thing to do, titled verb plus object ("Pay council
tax"), never the sender's subject. Bellotti et al.'s Taskmaster (CHI 2003)
found that a marker saying "something to do" without saying what "did not
help much in planning", and its warning bar was the best-rated feature
(4.4 of 5). Promises (`contact_commitments`) and requests from people share
the list with a `kind`, showing the person ("you promised", "Priya
asked"); `ListTodos` merges both tables (todo.md §7).

**Extracted** (todo.md §4): verb, object, counterparty, amount and
currency (schema.org `Invoice.totalPaymentDue` first), `due_at` with its
verbatim due words, `act_by` and `surface_at` from the lead-time table, one
action link with its registrable domain, trust (DMARC plus prior mail),
reason, origin, and the completion signal. Three dates stay apart, as in
Things, Todoist (deadlines as a separate field since January 2025) and
OmniFocus: when it's due, when you must act, and when you chose to do it.

```text
+--------+-------------------------------------------------------------------------------+
| To do 3|  3 things need you this week. Council tax first, act by Wed.  [ Do this week ]|
|        |  NOW                                                                          |
|        |  Pay council tax             Camden Council   £142.00                         |
|        |    [######----]  act by Wed 7 · due Fri 9        [ Pay on camden.gov.uk  ↵ ]  |
|        |    from Camden Council, 28 Sep · Here because: "payment due 9 October" (rule) |
|        |  Send the signed engagement form   Priya Shah (you promised)                  |
|        |    [########--]  act by Thu 8 · due Fri 9        [ Reply to Priya        ↵ ]  |
|        |  Verify your new sign-in email   Octopus Energy                               |
|        |    link expires today 18:00                      [ Verify on octopus...  ↵ ]  |
|        |  COMING UP                                                                    |
|        |  wk of 12 Oct  RSVP Sam's leaving drinks            shows up Mon 12 · Thu 15  |
|        |  wk of 19 Oct  Renew car insurance  Admiral £412    shows up Mon 19 · due 26  |
|        |  WHENEVER  2 >                      DONE THIS WEEK  4 >                       |
|        |  ↵ do it   e done   Z schedule   , edit   X not a to-do   o open email        |
+--------+-------------------------------------------------------------------------------+
```

**View and actions.** Four bands: Now (`surface_at <= now`, by act-by),
Coming up (30 days by week, dimmed, "shows up Mon 12"), Whenever
(collapsed) and one "Done this week" line. The runway bar fills from
surface date to deadline in the accent colour, never red; past the deadline
it reads "was due Fri" (D101). Selecting a row opens a side panel of
editable field chips, each with its source sentence. Enter does the one
thing: opens the gated link, starts the reply, or answers the RSVP.
"Do this week" (`g F`) steps through Now like Focus & reply. Returning to
the window within 10 minutes of Enter on a link asks "Done?" with `e`. A
bill collected by direct debit or card on file is an Update, as Monzo
shows scheduled payments, but a failed collection is always a to-do.

**Rhythm, end state, in and out.** To-dos surface once, at the start of
working hours on their surface day (the `surfaced_at` claim); only expiring
verify links surface at once; Mondays the headline looks at the week. The
empty state names the next thing and when it shows up. Enters by rule
detection in `post_sync_fanout`, the model (phase 7), handoff, or a
promise. A new message in the thread updates the row by dedup key and
brings a scheduled row back early, as Linear's snooze does. Leaves by `e`,
`X`, or the world: a matching confirmation makes the row "Looks done:
payment received 3 Oct". RSVP completes when
`calendar_invites.current_partstat` is set; a strong pay match (same
domain, same amount or reference, a receipt word) completes with a day of
undo; the rest only offer.

**Not a list of emails.** The row is the task, ordered by when you must
act; a bill, its reminder and its receipt are one row that updates and then
completes; each row has one button labelled with its verb and destination.

## Updates is a briefing by source, read twice a day and let go in one key

**Unit.** A source line: everything one source sent since the last cut,
folded to its latest fact per template, its numbers and its strongest
signal. Things with a lifecycle (parcel, build, incident) are trackers
showing where they are now, as Live Activities do. HEY Bundles, Shortwave
bundles, SaneBox digests and Android's Notification Organizer all converge
on one row per source (updates.md §2).

**Extracted** (updates.md §4): `source_key`, `template_key` (subject with
numbers, IDs and names masked, no model), a one-sentence `fact` (cleaned
subject first, model only for generic subjects), quoted `numbers`, a
code-computed `delta`, `state` for trackers (`deliveries` already does
this), `signal` (`routine`, `changed`, `new_source`, `anomaly`,
`needs_you`), one link, and provenance per field.

```text
+ Updates ----------------------------------------------------------------------------+
| This morning's digest · 08:00 · 31 updates from 12 sources    [Let go of all  A]    |
| NEEDS A LOOK                                                                        |
| ! Google        New sign-in from Chrome on Windows, Lisbon, 06:12   already in To do |
| ! Stripe        Payout of R 4,210.00 failed: bank declined          [This needs me t]|
| CHANGED                                                                             |
| Bookshop        Parcel  ordered - shipped - (*) out for delivery - delivered        |
|                 Arriving today by 18:00 · DHL · 3 emails                  [Track  L]|
| GitHub acme/api Build failing on main since 11:02 · 3 runs · was green 2 days       |
| Strava          Your week: 3 runs, 21.3 km  up 12% on last week                     |
| ROUTINE                                                                             |
|   Vercel        7 deploys succeeded · latest 07:41 acme-web                 7       |
|   Uptime Robot  All monitors up · 1 blip 02:14 (40s)                        6       |
|   + 4 quieter sources                                                       8       |
| - arriving for 16:30 ------------------------------------------------- 4 so far -  |
+-------------------------------------------------------------------------------------+
  A let go of digest  e let go of source  t this needs me  K tune source  L link  o email
```

**View and actions.** Three sections by signal, not date: Needs a look,
Changed, Routine. No timestamps unless the time is the fact, no unread
state, badge or sound. Google's SRE book is the model: only actionable
alerts may interrupt, the rest belongs on a dashboard that shows state. `A`
lets go of the cut with a dry-run preview ("Let go of 31 updates from 12
sources; 2 also in To do stay there"), `e` lets go of one source, `t` hands
to To do, `K` tunes a source (mute, changes only, breakthrough, never
summarise).

**Rhythm.** Two fixed cuts, 08:00 and 16:30 in the user's zone,
configurable from one to four, plus a quiet "since 08:00" strip. Fitz,
Kushlev et al. (Computers in Human Behavior 2019, n = 237) found three
fixed batches a day improved attention and mood, hourly batching did
nothing, and no delivery raised anxiety; two is the extrapolation for
slower email. Mark et al. (CHI 2016) found batching helped rated
productivity but not stress, so mxr markets this as attention, not calm.
The cut is a stable set, so let go acts on exactly what the preview listed.
Leftovers fold into the next cut instead of stacking.

**In and out.** Enters by an Automated base mode or a copied thread;
`needs_you` breaks through to To do on arrival. Leaves by let go. Trackers
that end well leave on their own (a delivered parcel goes to Archive); bad
endings move to Needs a look. After eight digests let go without opening
a source, mxr asks once whether to mute it.

**Not a list of emails.** Thirty-one emails from twelve senders are twelve
lines, mostly folded; four parcel emails are one track; the text is the
fact; numbers come compared; replying isn't offered.

## Reading is a front page you visit, not a pile you owe

**Unit.** A readable item: a single-essay issue, each link item in a
digest, or the article a teaser points to. Readwise Reader keeps pushed
content (Feed: Unseen, Seen) apart from chosen content (Library); here the
edition is the feed and Later is the shelf (reading.md §2).

**Extracted** (reading.md §4), rule-based in `post_sync_fanout` with no
model and no network: source, cleaned headline, standfirst, shape
(`single`, `digest`, `teaser`, `notice`), main link and digest items with
unwrapped URLs, minutes (238 wpm, adjusted from finished items), cadence,
and local engagement (opened, finished).

```text
+------+---------------------------------------------------------------------+
| Read |  Reading                                   Later 4   Sources        |
|      |  SINCE YOU WERE LAST HERE                                           |
|      |  +---------------------------------------------------------------+  |
|      |  | The quiet death of the three-pane layout                      |  |
|      |  | Long Reads Weekly  ·  14 min  ·  you read 9 of 10             |  |
|      |  | Why every RSS reader since 2002 shipped the same window...    |  |
|      |  |                          [↵] read   [b] later   [e] let go   |  |
|      |  +---------------------------------------------------------------+  |
|      |  SQLite Notes · weekly digest · 6 links                    4 min    |
|      |    > Local-first mail is having a moment        demo.mxr.local      |
|      |    > SQLite 3.51 release notes                  sqlite.org          |
|      |    + 4 more                                                         |
|      |  ------------------- you left off here --------------------------   |
|      |  EARLIER THIS WEEK                                                  |
|      |  Platform Weekly · Shipping a sync engine in 2026         9 min     |
|      |  FADING  (goes on Sunday, unless you keep it)                       |
|      |  Growth Digest · you opened 0 of the last 11       [D] unsubscribe  |
+------+---------------------------------------------------------------------+
```

**View and actions.** Three time bands ranked inside by how much you read
each source; a new source gets one lead slot for its first three issues.
No counts except Later, as Reeder dropped unread counts for a synced
position. NN/g found users fully read 19% of newsletters and spent 51
seconds on one, so the edition is a scan surface (headline, source,
minutes, standfirst) and the rare full read gets a 66-character reader
column (Butterick: 45 to 90). `R` shows the sender's layout, remembered per
source. Enter reads, `L` fetches the linked article (only on that key,
naming the domain it contacts), `b` puts it on Later, `e` lets go, `D`
unsubscribes with evidence ("You opened 0 of the last 11 issues") through
the existing `UnsubscribePurge { dry_run }`. No reply, forward or labels.

**Rhythm, end state, in and out.** Pull only. Each source's items fade
after twice its median interval, clamped to 2 to 14 days, through a
visible Fading band (Current's half-life; Feedly marks items read after 31
days). Expiry writes `mode_done`, nothing is deleted, and Later never
expires silently; items over 30 days ask once, "Still want it?". The empty
state says you are current.

**Not a list of emails.** A digest is many items and a teaser is its
article; the title is the headline; order is by your reading; opening gives
a book page, not a 600px template; unsubscribe is a key with evidence.

## Archive is a filing cabinet you ask questions of

**Unit.** A record, built from one or more emails: an order's
confirmation, dispatch and delivery are one row, a trip's bookings are one
trip, a year of bills is a series, as TripIt and Gmail's Purchases view
show (archive.md §2).

**Extracted** (archive.md §4): kind, issuer, issued date (the
transaction's, not arrival), amount and currency, reference, title, span,
good-until, documents, source messages, group, provenance per field, and a
`checked` flag that is true only when every money and date field came from
schema.org or the user. Schema.org first, then per-issuer templates learned
from confirmed records (most business-to-consumer mail is templated: Sheng
et al., KDD 2018, on Google's Juicer), then labelled-line rules, then a
model that may only choose among strings that appear verbatim.

```text
+----------+------------------------------------------------------------------+
| Archive  |  / What are you looking for?   "lisbon booking"                  |
|          |  ANSWER                                                          |
|          |  Booking ref  K7QX2M                                [y] copy     |
|          |  TAP Air Portugal · LHR -> LIS · Thu 12 Jun 2025 07:40           |
|          |  Part of trip "Lisbon, June 2025" (flight, hotel, 2 tickets)     |
|          |  from schema.org markup · checked     [↵] e-ticket.pdf  [o] email|
|          |  Kind: All  Receipts  Orders  Trips  Bills  Documents   [g f]    |
|          |  2025 · MARCH                                      3 · £1,412.40 |
|          |  03 Mar  Dell           XPS 14 laptop     £1,249.00  402-118  PDF|
|          |          ordered · shipped · delivered 7 Mar · warranty to 2027  |
|          |  11 Mar  Octopus Energy Bill, Feb         £  128.40  A-99312  PDF|
|          |  28 Mar  Apple          iCloud+ 200GB     £    2.99  MSXK21   -  |
|          |                                    amount unchecked (model) o    |
+----------+------------------------------------------------------------------+
  / ask  ↵ document  y copy ref  Y copy amount  o email  p issuer  [ ] year  E export
```

**View and actions.** The answer box is the default focus. A query that
matches a record field returns the field, with no model; only then does it
fall back to `mxr ask` (citation-checked today) and to mail search, saying
so. Below, a ledger by month with counts and totals, because date was by
far the most common sort in Stuff I've Seen (Dumais et al., SIGIR 2003);
the issuer page (`p`) is the orienteering step people take even when they
know what they want (Teevan et al., CHI 2004). `E` exports CSV plus PDFs
with a dry-run preview, `,` fixes a field or marks the card checked, `X`
says not a record, `t` hands to To do ("claim warranty").

**Rhythm, end state, in and out.** On demand, no badge. A record with a
moment (a trip in 72 hours, a ticket today, a return window closing in 3
days) shows in a "Coming up" strip and one line on Now, as Wallet passes
surface by date. Records enter from the detector, a delivered parcel, a
ticked-off to-do, `T` from Messages (a dry-run card), or "always file this
sender"; they never leave, only get dismissed. The user never files one
message at a time: Whittaker et al. (CHI 2011, 345 users, 85,000
refinding actions) found folder access took 58.8 seconds against 17.2 for
search and did not raise success.

**Not a list of emails.** The row shows the fields you came for; the date
is the transaction's; the primary action is copy or open the document; a
query returns an answer card.

## Each mode indexes the part of the email it cares about

Semantic search has one recipe for every message today. `build_chunks`
(`crates/semantic/src/lib.rs`, line 1655 at `bdf997c4`) makes a header
chunk (subject, sender, recipients, snippet), body chunks from
`mxr_reader::clean` output in 120-word windows with 30-word overlap
(`chunk_text`), and for each attachment a filename and type chunk plus
text windows. Chunks are keyed by `(message_id, source_kind, ordinal)` and
embeddings by `(chunk_id, profile_id)` (`004_semantic_search.sql`), so the
same text in two messages embeds twice, and the only filter is
`allowed_source_kinds`.

Each mode should index what it shows. Content units are pulled out once
per message: new text (quotes and signatures removed by the thread-aware
matcher), article or link sections, extracted record fields, attachment
text, and the gist when one exists. A per-mode recipe then picks units,
windowing and a context prefix. A chunk is tagged with every mode it
serves (many-to-many), and embeddings are keyed by a hash of the chunk
text, so identical text embeds once even when the email is in several
modes.

| Mode | Recipe | Why |
|---|---|---|
| Messages | Each message's new text, prefixed with the person and topic ("Samir Patel · Contract renewal"), plus the gist. Quoted history is not re-indexed | Contextual retrieval: the prefix carries who and what, which a 120-word window loses |
| To do | One instruction chunk ("Pay council tax, Camden Council, £142, due 9 Oct") plus the body | Amounts and dates are answered by SQL over `todos`, not vectors |
| Updates | One fact chunk per message, deduplicated by `template_key` | Fifty "build passed" mails are one meaning |
| Reading | Larger, section-aware chunks, one per digest link item, plus fetched article text; embedded lazily, when read or for sources the user reads | Most issues are never read (NN/g), so eager embedding wastes the most work here |
| Archive | One field chunk plus PDF text | Exact identifiers (references, order numbers) stay on BM25 in hybrid search |

A baseline (header plus new text) is indexed at sync, so search works at
once; the recipe enriches it after classification. Each message is stamped
with its recipe version and classification version, and only stale
messages reindex, through the existing resumable `SemanticIndexJob`. There
stays one embedding model and one ANN (`hnsw_rs`) index per profile;
search gains a mode filter beside `allowed_source_kinds`. Recipes start as
a typed, versioned table in `crates/semantic`, not user config, so lifting
them into config later stays possible. Before switching, a local retrieval
eval on BK's real mail reports top-5 hit rate (counts only) for today's
chunking and for the recipes. Each mode's recipe ships in that mode's
phase (D113).

## One key map across Now and the modes

Checked against `docs/reference/tui-keymap.json` and the web registry
(`apps/web/src/lib/actions/`, `features/mail-actions/verbActions.ts`,
`features/places/actions.ts`, `features/focus/actions.ts`) at `bdf997c4`.
Modes get their own scope in both clients, as `place` has today, and
`keymapParity.test.ts` holds web and TUI to the same table.

| Key | Meaning wherever it applies | Now | Msgs | To do | Upd | Read | Arch | Meaning today |
|---|---|:-:|:-:|:-:|:-:|:-:|:-:|---|
| `Enter` | The mode's primary verb: open person, do it, expand source, read, open document | yes | yes | yes | yes | yes | yes | Open |
| `e` | Done here (tick off in To do, let go in Updates and Reading) | yes | yes | yes | yes | yes | | Archive; Done on desk and in focus |
| `A` | Let go of everything in view, with dry-run preview | card | | | yes | yes | | Sweep place; attachments (list) |
| `t` | Make a to-do from this | yes | yes | | yes | yes | yes | unbound |
| `T` | Pass to another mode (menu) | yes | yes | yes | yes | yes | yes | unbound |
| `X` | Not this mode, for this email | yes | yes | yes | yes | yes | yes | web reader expand all |
| `K` | This sender here: mode, never/always, lead time, mute, changes only, never summarise | yes | yes | yes | yes | yes | yes | Sender kind menu (place) |
| `o` | Open the email itself, as sent | yes | yes | yes | yes | yes | yes | Open (list alias) |
| `r` / `a` | Reply / reply all | yes | yes | | | | | same |
| `.` | Got it: a short acknowledgement after a countdown | | yes | | | | | unbound |
| `b` | Later: reply later, or the Later shelf | | yes | | | yes | | Reply later |
| `Z` | Not now: snooze, or schedule in To do | yes | yes | yes | | | | Snooze |
| `,` | Edit extracted fields; mark a record checked | | | yes | | | yes | unbound |
| `L` | Open the item's link: tracker page, linked article | | | | yes | yes | | Links |
| `D` | Unsubscribe, with evidence and preview | | | | | yes | | Unsubscribe |
| `R` | Original layout or cleaned | | | | | yes | | Reader mode (TUI) |
| `y` / `Y` | Copy reference / amount | | | | | | yes | Summarize (TUI) |
| `p` | This person's or issuer's page | | yes | | | | yes | Sender view; pin (place) |
| `s` | Pin a person (star) | | yes | | | | | Star |
| `c` | Compose; in Messages a new topic with this person | yes | yes | | | | | Compose |
| `>` | Quote the selection into the reply | | yes | | | | | More senders (place) |
| `[` / `]` | Previous / next group: topic, year | | yes | | | | yes | Collapse / expand (sidebar) |
| `Mod+Enter` | Send, then the next person whose turn it is | | yes | | | | | Focus send |
| `g F` | Step through the Now band | | yes | yes | | | | Focus & reply |
| `/`, `g f`, `x`, `u`, `?` | Search (Archive's ask box), filter (facets), select, undo, help | yes | yes | yes | yes | yes | yes | same |
| `g h` `g m` `g x` `g u` `g r` `g e` `g i` | Now, Messages, To do, Updates, Reading, Archive, Inbox | | | | | | | `g u` was Subscriptions |

Where the research notes disagreed, the reason for the choice:

- `e` is done here everywhere, including tick-off: it is already Done on
  the desk (`deskDone.ts`) and in focus (`focus.done`).
- Let go of all is `A`, not `L` (updates.md): `A` is Sweep all with a
  dry-run preview in the place keymap, and Paper trail becomes Updates;
  `L` opens links in list, place and reader in both clients.
- `x` stays select everywhere, because batch done needs it. todo.md read
  it as "screened out", which is only the letter inside the `K` menu
  (`placeCopy.ts`); updates.md used it for "let go of source". Dismissal
  moved to `X`, let go of a source to `e`.
- Later is `b` (FlagReplyLater today), not `l` (Apply label, reading.md)
  or `Z` (messages.md); `Z` stays snooze.
- Unsubscribe is `D` in both clients, not `U` (Mark unread, reading.md).
- Archive's research keys collided with global ones: `c` (compose) became
  `,`, `f` (forward) became `g f`, `x` (select) became `E` (export), `v`
  (move) folded into `,`, `g i` (Inbox) became `p`, `d` (screener deny)
  became `X`, and `e` (done) became `o`.
- Updates' `m` (mark read and archive) and `c` (compose) moved into the
  `K` menu, whose option letters become the `g` letters (`m`, `x`, `u`,
  `r`, `e`, and `d` to screen out).
- No "why" key: `?` is help in both clients, every row shows its reason
  line, and the TUI footer shows the selected row's reason and link domain.
- `v` (move) is not reused for "view original" (messages.md, updates.md,
  reading.md); `o` opens the email as sent everywhere.
- `g u` moves from Subscriptions to Updates, with the old binding in
  `retiredAliases` as `g R` and `g P` were; `g p` redirects to Updates;
  `g q`, `g w` and `g o` open Messages filters. `g m`, `g x` and `g e` are
  unbound in both keymaps.

## Conflicts the research left, and what this plan chose

| Topic | The notes said | Chosen | Why |
|---|---|---|---|
| Updates cadence | Old draft: once a day. updates.md: two cuts. now-and-handoff.md: one card on Now | Two cuts (08:00, 16:30), 1 to 4 configurable; Now shows the latest cut as one card | Fitz et al.: a few fixed batches helped; a parcel out for delivery at 10:00 is stale by morning. Amends D110 (D112) |
| Ticked-off to-do and Archive | todo.md, archive.md: offer "File in Archive". now-and-handoff.md: no prompt | No prompt; toast with undo | Whittaker 2011: filing effort is wasted; Archive files records itself anyway |
| Due soon order on Now | now-and-handoff.md: by `due_at`. todo.md: by act-by | Act-by first, due second, everywhere | A passport due in January needs action in November |
| Mobile navigation | reading.md: six tabs. now-and-handoff.md: five | Now, Messages, To do, Reading, Find (Archive + search + Inbox); Updates through Now's card | Apple HIG and Material 3 cap a tab bar at five; Updates is visited twice a day |
| Lead times | Old draft: renew 7 days, documents 30 | todo.md's table (below) | HMPO advises up to 10 weeks; renewals need time to compare quotes |
| Group threads | Old draft: groups as their own rows | Keyed by `thread_id`; CC-only goes to Updates | Slack and iMessage split groups when members change |
| Screener | Old draft: off the rail | Off the rail; a first-time sender's row carries one inline question; `/screener` under More as history | HEY's queue is heavy early and light later; mxr never withholds mail, so a gate duplicates the classifier |
| Promises and admin | Old draft asked one list or two | One list with a `kind`; promises first on equal act-by | Two deadline lists make the user merge them; Taskmaster and Viva show the person matters |

## Data the modes need

To-dos get their own table: a to-do outlives its email, can be made by
hand, and gets scheduled, ticked off and corrected. `deliveries` and
`contact_commitments` set the pattern (a detected row, provenance back to
the message, non-destructive resolve and dismiss).

```sql
CREATE TABLE todos (
    id                TEXT PRIMARY KEY,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id         TEXT NOT NULL,
    source_message_id TEXT NOT NULL,
    title             TEXT NOT NULL,          -- verb + object: "Pay council tax"
    verb              TEXT NOT NULL,          -- pay | renew | verify | sign | book | rsvp | return | other
    doc_type          TEXT,                   -- bill_link | bill_bank | renewal | passport | lease | ...
    counterparty      TEXT,
    amount_minor      INTEGER,
    currency          TEXT,
    due_at            INTEGER,                -- the outside deadline; NULL when none
    due_words         TEXT,                   -- verbatim phrase from the message
    act_by_at         INTEGER,                -- due minus notice or processing time
    surface_at        INTEGER,                -- act_by minus lead time, at the start of working hours
    scheduled_for     INTEGER,                -- the user's When date
    action_url        TEXT,
    action_domain     TEXT,                   -- registrable domain, shown before Enter
    action_trusted    INTEGER NOT NULL DEFAULT 0, -- DMARC pass and domain match: gates the button
    state             TEXT NOT NULL CHECK (state IN ('open', 'done', 'dismissed')),
    done_message_id   TEXT,                   -- the confirmation that completed it, if any
    origin            TEXT NOT NULL CHECK (origin IN ('rule', 'schema', 'model', 'handoff', 'manual')),
    reason            TEXT NOT NULL,
    field_sources     TEXT NOT NULL,          -- JSON: field -> schema | rule | model | user
    model             TEXT,
    surfaced_at       INTEGER,                -- claim guard: announced once
    dedup_key         TEXT NOT NULL,          -- account + thread + verb + normalised object
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    done_at           INTEGER,
    UNIQUE (account_id, dedup_key)
);
```

Upserts use `ON CONFLICT(account_id, dedup_key)`, never `INSERT OR
REPLACE`. The wake loop sets `surfaced_at` in the same UPDATE that finds
the row, as `reply_later_returned_at` does (`056_reply_later_due.sql`), so
a to-do is announced once across restarts.

Mode membership is computed, not stored, as places are today (D097):

```text
modes(message) = base_mode(sender)          -- mail_kind + screener
               + aspects(message)           -- message rules + cached model answers
               + open todo rows for the thread
               - corrections(message)
               - modes marked done (through the thread's watermark)
```

Shared stores: `mode_aspects` (model cache by prompt version and content
hash), `mode_corrections (account_id, scope, key, mode, verdict,
decided_at)` with `scope` in `message | sender`, and `mode_done
(account_id, thread_id, mode, through_seq, through_count, done_at)` on the
`DeskDismissal::covers` watermark. Per-mode stores are specified in the
research notes: `update_facts` and `update_sources` (updates.md §4),
`reading_items`, `reading_state` and `reading_articles` (reading.md §4),
`records` with its field, message and group tables (archive.md §4), and a
small `people` identity layer (messages.md §5). Activity records carry ids
and counts only, never titles, amounts or due words, and `MXR_ACTIVITY=off`
disables them.

Deleting an email deletes everything derived from it. Every new store
above (to-dos, aspects, facts, reading items, records, index chunks) is
removed when its source message leaves the store, through a foreign-key
cascade or the delete path in sync, and a store that merges several
emails (a record, a tracker) drops the deleted message's fields and
removes itself when no source is left. Today's gaps, such as gists,
summaries, promises, decisions and opened attachment files that outlive
their message, are tracked separately and listed on the site's Security &
Privacy page.

## Lead times are a fixed table by kind of action

The lead time is how long the outside world takes. One default fails
because the spread runs from hours (a verify link) to ten weeks (a
passport); a learned one fails as Todoist's Smart Schedule did, withdrawn
because it "was not accurate enough to be helpful" (todo.md §6).

| Kind | Act-by | Surfaces | Basis |
|---|---|---|---|
| Bill, pay link on the biller's site | due date | 3 days before act-by | Card payment is immediate; 3 days is slack |
| Bill, bank details only | due minus 3 working days | 3 days before act-by | Bacs takes three working days |
| Bill by direct debit or card on file | not a to-do | Updates | Scheduled payments; a failed collection is always a to-do |
| Subscription or insurance renewal | due date | 14 days before | Time to compare quotes |
| Passport, visa | expiry minus 6 months, or the trip date if known | 10 weeks before act-by | HMPO advises up to 10 weeks; US routine is 4 to 6 weeks plus post |
| Driving licence, ID card | expiry | 8 weeks before | Judgement; no source yet |
| Lease or tenancy | end minus notice (2 months default in England) | 4 weeks before act-by | Tenant notice period |
| Return window | delivered + stated window ("assumed 14 days") | 4 days before close | Consumer Contracts Regulations; only on request |
| RSVP | the reply-by date, else 2 days before the event | 2 days before | No standard |
| Verify, confirm | link expiry | at once | Links expire in hours |
| Promise you made | the date you named | 1 working day before | You chose the date |
| Other | due date | 2 days before | Default |

`surface_at` moves to the start of the user's working hours and is never
later than now; a date already past surfaces at once as "was due Tue". Numeric
dates ("09/10/2026") are not parsed by `natural_time` and are ambiguous, so
schema.org dates come first, then the account's locale, then the row asks.
Lead times are config per kind and per sender through `K` ("Remind me 2
weeks earlier for Admiral"). The sources are mostly UK; other defaults are
open.

## How the modes map onto what exists today

| Exists today | Becomes |
|---|---|
| Desk lanes (`desk_lanes.rs`, `desk.rs`) | Now |
| Reply queue, Owed, Waiting on | Messages filters, sharing one owed rule with Now |
| Focus & reply (`routes/focus.tsx`) | `g F` inside Messages and To do |
| Snoozed (`snooze.rs`) | A state each mode shows; the cross-mode list under More |
| Paper trail (`places.rs`) | Split: notifications to Updates, records to Archive; `/paper-trail` redirects |
| Reading place (`ReadingRoute.tsx`, full renders 12 at a time) | Reading's edition, reader and Later |
| Screener (`screener.rs`) | An inline per-row question; the page under More as history |
| Promises (`024_contact_commitments.sql`) | To do rows with `kind = promise` |
| Deliveries (`042_deliveries.sql`) | Updates trackers in transit, Archive orders once delivered |
| Invites (`038_calendar_invites.sql`) | To do while unanswered, Archive after |
| Subscriptions (`subscriptions.tsx`) | Reading's Sources view |
| `mxr ask` (`archive_ask.rs`) | Archive's fallback answer |
| `mxr triage` (`triage.rs`) | Superseded by mode aspects in phase 7 |

## Phases

Each phase ships one mode's real view in its researched shape, behind no
flag: daemon IPC and CLI JSON first, then TUI and web, then MCP, then that
mode's index recipe. Mutations and batches go through the preview path
(D098).

### Docs land with the feature they describe

The docs site documents what ships, so the modes reach the docs in two
steps. Phase 0 changes the philosophy and positioning now and says
plainly which parts are planned. Each later phase updates the pages its
Docs line names, in the same release as the feature, and a phase is not
done until they are (rubric v3, X11). Every phase also:

- updates the status table on `guides/email-modes.md` and drops
  "(planned)" from the glossary terms it ships;
- regenerates the CLI reference (`npm run generate` in `site/`) and the
  OpenAPI dump for the commands and routes it adds;
- keeps an old URL working with a redirect when it replaces a page;
- checks every new claim against the code at the release commit and runs
  `npm run build` in `site/`.

[docs/site-modes-inventory.md](../site-modes-inventory.md) lists every
site page and repo doc with the phase that changes it, and the sidebar to
move to once phase 2 ships.

### Phase 0: say the new philosophy

Rewrite only what states the philosophy, and describe nothing unbuilt as
if it ships. Site: a new `guides/email-modes.md` ("Email is five apps at
once") with the model and a status table per mode pointing at the shipped
features that serve it; `index.mdx`, `guides/why-mxr.md`,
`guides/glossary.md` (mode terms marked planned), `guides/llm-features.md`
and `guides/security-and-privacy.md` (what each request sends today, the
planned tiers and cloud-only setup, what deletion removes today); notes on
`guides/your-day.md` and `guides/desk.md`; the sidebar entry. Repo:
`README.md`, `docs/vision.md`, `docs/README.md`, `AGENTS.md` and the
product description in both agent skills. Plan: this workstream, a Docs
line on every phase, the roadmap checklist and rubric X11.

- **Check:** `npm run build` in `site/` passes with no broken internal
  links; every claim about shipped behaviour names the code it was checked
  against; `docs/site-modes-inventory.md` is committed.

### Phase 1: To do as a runway, with the data it needs

The `todos` table; the rule detector in `post_sync_fanout` (admin verbs,
due phrases, direct debit, failed collection); `schema_org.rs` extended to
`Invoice` (`totalPaymentDue`, `paymentDueDate`, `provider`) and
`*Reservation`; the lead-time table with act-by in working days; the action
link picker with the DMARC and domain gate; confirmation matching; promises
merged in `ListTodos`; `CreateTodo`, `UpdateTodo`, `SetTodoDone {
dry_run }`; `mxr todo` (`list`, `add --from MESSAGE --due PHRASE`, `done`,
`schedule`, `edit`, `undo`, all `--format json`); the web view with bands,
runway bars, one labelled button and the field panel; the TUI lens with the
link domain in the footer; a To do rail entry; the desk's Due lane on
act-by; `t` from a conversation. Models: `llm.tiers.fast` and
`llm.tiers.smart` with the feature-to-tier table; smart-tier extraction of
To do fields for mail the rules placed in To do, with the verbatim
amount and date checks, the content-hash cache and the model named on each
chip; `mxr modes eval --extract` comparing local and cloud on the user's
mail. Index: the recipe table, chunk mode tags, text-hash embedding keys,
the To do instruction chunk, and the retrieval eval's first run on today's
chunking.

- **Check:** in the demo, Camden's council tax email (schema.org
  `Invoice`) reads "Pay council tax, Camden Council, £142.00, act by Wed 7
  · due Fri 9" with "Pay on camden.gov.uk"; a lookalike-domain copy shows
  "Open email to pay" and the raw domain; the receipt turns it into "Looks
  done"; a bill due in five days appears on the desk three days before,
  once, across a restart. On BK's real mail `mxr todo --format json` lists
  his bills and renewals, and he records counts of right, wrong and missed,
  plus the extraction eval's local and cloud field counts.
- **Docs:** site: new `guides/todo.md` and its sidebar entry;
  `guides/email-modes.md`, `guides/glossary.md`, `guides/forgotten-work.md`
  (promises become To do rows), `guides/desk.md` (the Due lane on act-by),
  `guides/calendar-invites.md`, `guides/llm-features.md` and
  `guides/security-and-privacy.md` (`llm.tiers`, the smart tier's cloud
  opt-in), `guides/semantic-search.md` (recipe table, text-hash keys),
  `guides/web-app.md`, `guides/activity-log.md`,
  `guides/automation-contract.md`, `guides/for-agents.md`,
  `guides/agent-skill.md`, `reference/config.md`, `reference/json-output.md`,
  `reference/keybindings.md` (`t`), `reference/tui.md`, `reference/bridge.md`,
  `reference/mcp.md`, `reference/time-phrases.md`. Repo: `README.md`,
  `.agents/skills/mxr/SKILL.md`, `docs/web-app.md`, `docs/activity-log.md`,
  `docs/reference/ai-email.md`, `docs/security-audit-rubric.md`,
  `PRIVACY.md`, `docs/calendar-email/`, blueprint 02, 05, 09 and 12.
- **Tests:** `todos` store tests (dedup upsert, claim guard);
  `llm_tiers` tests (precedence override, tier, base; a cloud tier without
  `api_key_env` is refused; only To do mail reaches the smart tier; with no
  cloud tier, fields come back unchecked);
  `todo_detect` fixtures per verb, due phrase and doc type; `schema_org`
  Invoice fixtures; `action_link` gate fixtures (DMARC fail, lookalike,
  redirect wrapper, prior-mail domain); `lead_time` tests per kind;
  `todo_complete` matcher tests; `handler/tests/todo.rs` with a moved
  clock; CLI JSON snapshot; `e2e/todo.spec.ts`; TUI lens test;
  `index_recipe` tests (one embedding per distinct chunk text).

### Phase 2: Now, the rail, membership, per-mode done and the eval harness

`modes()` with rules only, `mode_corrections`, `mode_done`,
`ListModeItems`, `GetModeMembership`, `SetModeCorrection`, `SetModeDone {
dry_run }`, `GetNow`; `mxr modes`, `mxr why`, `mxr now`; the desktop rail,
mobile tabs and TUI sidebar with the new `g` keys and `K` menu; Now's four
capped sections; the identity anchor and "Also in"; archive on last done
with its toasts and setting; the Screener as an inline question; `mxr
modes eval` in rules-only mode, so accuracy is measured before any model.
Index: the baseline at sync, version stamps, stale-only reindex and the
mode filter on search.

- **Check:** in the demo, "Action required: unusual sign-in attempt" is
  in To do or Updates, says why, and is never in People. The landlord's
  email is in Messages and To do; `e` in Messages toasts "Done in
  Messages. Still in To do" and Gmail keeps it; ticking off the to-do
  toasts "Archived in Gmail". Now never shows more than ten items. `mxr
  modes eval --sample 200` prints rules-only counts on BK's mail.
- **Docs:** site: `guides/now.md` replaces `guides/desk.md` and
  `reference/now-and-modes.md` replaces `reference/desk-and-places.md`, both
  with redirects; the sidebar reorganised by mode (inventory);
  `guides/your-day.md`, `guides/email-modes.md`, `guides/glossary.md`,
  `guides/triage-flow.md` (the Screener as an inline question),
  `guides/mailbox.md`, `guides/search.md` (mode filter),
  `guides/semantic-search.md` (baseline at sync, stale-only reindex),
  `guides/sound-hints-and-touch.md`, `guides/web-app.md`,
  `guides/architecture.md`, `guides/no-native-desktop-app.md`,
  `guides/recipes.md`, `examples.md`, `getting-started/quick-start.md`,
  `getting-started/first-sync.md`, `getting-started/gmail-setup.md`,
  `index.mdx`, `reference/keybindings.md` (`g` keys, `K`, `X`, `T`, `e`),
  `reference/tui.md`, `reference/cli/concepts.md`, `reference/json-output.md`,
  `reference/config.md` (`modes.archive_on_last_done`), `reference/mcp.md`,
  `reference/bridge.md`. Repo: `README.md`, `.agents/skills/mxr/SKILL.md`,
  `docs/web-app.md`, `docs/web-app-controls.md`, `ARCHITECTURE.md`,
  blueprint 00 and 08, a superseded note on 21, and the regenerated
  `docs/reference/tui-keymap.json`.
- **Tests:** `mode_membership` unit tests; `handler/tests/modes.rs`
  (preview equals commit, archive only when the last mode lets go, setting
  off); `handler/tests/now.rs` (caps, more counts); `e2e/now.spec.ts`;
  `e2e/modes.spec.ts`; `keymapParity.test.ts` with the mode scopes.

### Phase 3: Messages as people with topics

`conversation_shape`, the `people` identity layer with manual merge,
closeness bands, thread-aware quote and signature stripping in
`crates/reader` with `trimmed` flags, the ask (smart tier, quoted
verbatim), `GetPerson`, `mxr messages`
(`--turn`, `show <person>`, `ack <thread> --dry-run` printing the exact
text), the web list and person page with the permanent composer and Got
it, and the TUI two-pane lens. Index: the Messages recipe (new text with a
person and topic prefix, plus the gist).

- **Check:** Samir is one row however many threads he's in; the Samir and
  Ruth thread is its own row; a CC-only thread is in Updates. BK's top ten
  are people he would name (agree count recorded).
- **Docs:** site: new `guides/messages.md` and its sidebar group;
  `guides/focus-and-reply.md` (`g F` inside Messages),
  `guides/forgotten-work.md` (owed replies become Your turn),
  `guides/sender-view.md` (the person page), `guides/automated-followups.md`
  (Waiting on as a Messages filter), `guides/briefings-and-loop-in.md`,
  `guides/semantic-search.md` (the Messages recipe), `guides/email-modes.md`,
  `guides/glossary.md`, `guides/web-app.md`, `reference/keybindings.md`
  (`.`, `>`, `Mod+Enter`), `reference/tui.md`, `reference/json-output.md`,
  `reference/mcp.md`, `reference/bridge.md`. Repo:
  `.agents/skills/mxr/SKILL.md`, `docs/web-app.md`.
- **Tests:** `conversation_shape` fixtures (CC churn keeps one group row);
  quote-matching fixtures from Gmail, Apple, Outlook and plain text;
  `relationship_strength` tests; ack dry-run equals sent text;
  `e2e/messages.spec.ts`; retrieval eval rerun for Messages.

### Phase 4: Updates as a twice-daily briefing

`update_facts` (source, template, fact, numbers, code deltas, signal),
`update_sources` tuning, digest cuts with leftovers folding, `needs_you`
breakthrough, deliveries as trackers, `GetUpdatesDigest { cut }`, `mxr
updates` (`--cut`, `let-go --dry-run`, `tune`), the web briefing, Now's
card and the TUI lens. Index: one fact chunk per message, deduplicated by
template.

- **Check:** a day of demo notifications is two briefings of source lines;
  four parcel emails are one track; let go of a cut acts on exactly the
  previewed set and leaves what To do holds.
- **Docs:** site: new `guides/updates.md` and its sidebar group;
  `guides/reading-and-paper-trail.md` (notifications leave Paper trail),
  `guides/deliveries.md` (trackers), `guides/now.md` (the Updates card),
  `guides/your-day.md`, `guides/email-modes.md`, `guides/glossary.md`,
  `guides/semantic-search.md` (the fact chunk), `guides/web-app.md`,
  `reference/config.md` (digest cuts), `reference/keybindings.md` (`g u`
  moves from Subscriptions, `A`), `reference/tui.md`,
  `reference/json-output.md`, `reference/mcp.md`, `reference/bridge.md`.
  Repo: `.agents/skills/mxr/SKILL.md`, `docs/web-app.md`.
- **Tests:** `template_key` fixtures from real automated senders (split and
  merge cases); delta tests (none across units or templates);
  `handler/tests/updates.rs` (cut boundaries, fold, preview equals commit,
  badge stays zero); `e2e/updates.spec.ts`.

### Phase 5: Reading as an edition, a reader and a shelf

`reading_items` extraction, `reading_state`, bands with affinity and
expiry, unsubscribe evidence, `ListReadingItems`, `SetReadingState {
dry_run }`, `ListLater`, `mxr reading` (`--format json`, `later`, `sources
--rank`, `let-go --expired --dry-run`), the web edition and 66-character
reader, and the TUI lens. Article fetch (`FetchArticle`, a Rust Readability
port chosen by a spike on 30 real articles) comes last, on an explicit key
only. Index: section-aware chunks and link items, embedded lazily.

- **Check:** `mxr reading --format json` shows shape and minutes for every
  issue, and BK agrees with the shape on 30 sampled issues (counts only);
  "later" on a digest link, then offline, reads the article in full.
- **Docs:** site: new `guides/reading.md` (from the Reading half of
  `guides/reading-and-paper-trail.md`) and its sidebar group;
  `guides/unsubscribe.md` (`D` with evidence), `guides/now.md` (the evening
  pick), `guides/email-modes.md`, `guides/glossary.md`,
  `guides/semantic-search.md` (lazy embedding), `guides/security-and-privacy.md`
  (`FetchArticle` contacts the article's domain), `guides/web-app.md`,
  `reference/keybindings.md` (`b`, `L`, `R`), `reference/tui.md`,
  `reference/json-output.md`, `reference/mcp.md`, `reference/bridge.md`.
  Repo: `.agents/skills/mxr/SKILL.md`, `docs/web-app.md`.
- **Tests:** extractor fixtures from real newsletter HTML (low confidence
  falls back to the subject); expiry clamp tests; `e2e/reading.spec.ts`.

### Phase 6: Archive as records with an answer box

`records` and its field, message and group tables, the record detector,
PDF prefetch for record emails within a cap, record search fields,
`ListRecords`, `AnswerRecordQuery`, `SetRecordField`, `DismissRecord`,
`FileAsRecord { dry_run }`, `ExportRecords { dry_run }`, `mxr records`
(`list`, `show`, `find`, `fix`, `export`), the web answer box, ledger and
record card, and the TUI lens. Models: record fields on the smart tier for
mail the detector placed in Archive, with the same verbatim checks, cache
and provenance as phase 1, and `mxr modes eval --extract` extended to
record fields. Index: one field chunk plus PDF text, with identifiers left
to BM25.

- **Check:** "lisbon booking" returns the reference on an answer card with
  its provenance; the Dell order's three emails are one row; phase 1's
  ticked-off council tax is a record; the export preview states the
  unchecked rows.
- **Docs:** site: new `guides/archive.md` and its sidebar group;
  `guides/reading-and-paper-trail.md` becomes a redirect;
  `guides/archive-intelligence.md` (`mxr ask` as the fallback),
  `guides/search.md`, `guides/deliveries.md` (delivered orders as records),
  `guides/calendar-invites.md`, `guides/your-day.md`, `guides/email-modes.md`,
  `guides/glossary.md`, `guides/semantic-search.md`, `guides/llm-features.md`
  (record fields on the smart tier), `guides/security-and-privacy.md` and
  `index.mdx` (PDF prefetch ends "downloads an attachment when you open
  it"), `guides/web-app.md`, `reference/keybindings.md` (`y`, `Y`, `E`, `,`),
  `reference/tui.md`, `reference/json-output.md`, `reference/mcp.md`,
  `reference/bridge.md`. Repo: `README.md` (the attachment line),
  `.agents/skills/mxr/SKILL.md`, `docs/web-app.md`, `PRIVACY.md`.
- **Tests:** detector and field fixtures per source;
  `handler/tests/records.rs` (correction wins forever, preview equals
  file); `e2e/archive.spec.ts`; per-field precision in `mxr modes eval`.

### Phase 7: Fast-tier classification for what rules can't tell, measured

The `ModeAspects` feature on the fast tier, the loopback-only rule with the
user's explicit cloud opt-in and API key, the `mode_aspects` cache,
shortlist prefilter, quote and due-word checks, and italics plus model
provenance on every model-placed item.

- **Check:** `mxr modes eval --sample 200` with BK's local model reports
  per-mode precision and recall, deadline accuracy and field precision,
  recorded in the rubric; a cloud endpoint configured only for drafts
  classifies nothing.
- **Docs:** site: `guides/llm-features.md` and
  `guides/security-and-privacy.md` (the fast tier, the loopback rule and
  `allow_cloud_background_classification`), `guides/email-modes.md`,
  `guides/glossary.md`, `guides/rules.md`, `guides/triage-flow.md` and
  `guides/recipes.md` (`mxr triage` superseded), `guides/for-agents.md`
  ("What stays local"), `reference/config.md`. Repo: `README.md` ("What is
  local"), `PRIVACY.md`, `docs/security-audit-rubric.md`,
  `docs/reference/ai-email.md`, `.agents/skills/mxr/SKILL.md`.
- **Tests:** prompt tests (untrusted wrapping, quote check, dates from the
  parser); privacy tests in the style of `draft_compose`; cache
  invalidation by content hash.

## Design bets, and how we'll learn

- **One email in several modes has no direct evidence.** Taskmaster's
  thrasks are the nearest study, and it was small. From phase 2, log
  (counts only) how often "Also in" is followed, and BK notes any moment
  an email read as a duplicate. If it confuses, fall back to one primary
  mode with the others as chips.
- **Person rows with topics** rest on evidence that importance attaches to
  people, not on a test of this layout in email. Learn from BK's top-ten
  agree count and time to clear Your turn.
- **Two digest cuts** extrapolate from a phone study at three a day. Learn
  from a week of counts per cut in `docs/dogfooding-log.md`.
- **Length decides shape** must keep contracts readable where Spike failed.
  BK reads real long threads in it before phase 3 ships.
- **Auto-filing and auto-complete** depend on field precision and receipt
  matching that are unknown until phase 6's eval and a month of undo
  counts.
- **Per-mode index recipes** may not beat today's chunking. The retrieval
  eval decides per mode before switching.
- **Classifier errors are what users see first**, and Now's cap of three
  makes each one a third of a section. That is why the eval harness lands
  in phase 2.

## Unresolved questions

Research settled what the earlier draft asked (rows per person, archive on
last done, Screener, one To do list, lead times, digest cadence). What is
left is a values call, or needs BK's real mail.

- How strict is the pay-link gate: DMARC plus domain match, also require
  earlier mail from that domain, or never show a one-click money button?
- Should To do's Monday headline show a weekly money total ("£554 out this
  week"), or does it read as financial advice?
- Archive on last done is on by default on product precedent (Superhuman,
  Shortwave), not a study. Keep it on for you?
- Where do Reading highlights go: Archive search, a Markdown export, or
  Obsidian?
- What false-positive bar must To do meet before it gets a rail badge?
  todo.md suggests under one false to-do a week on your mail.
- Should person merge across addresses be automatic on exact name plus
  "you've written to both", or manual only?
- Is one key (`.`) right for Got it, which sends mail after a countdown, or
  should it be a chord?
- Reading's engagement tracking (dwell, scroll) is local and powers ranking
  and unsubscribe evidence. On by default, and separate from
  `MXR_ACTIVITY`?
- Which records count for your taxes, and should Archive ever suggest
  deleting old statements?
