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
are D107 to D119 in [15-decision-log.md](15-decision-log.md); D112 amends
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
email in many modes, handoff, navigation, archive on last done, Screener),
[relevance-and-backfill.md](../research/email-modes/relevance-and-backfill.md)
(relevancy windows and the first run),
[teaching-in-place.md](../research/email-modes/teaching-in-place.md)
(teaching the model without a tour),
[trust-and-completeness.md](../research/email-modes/trust-and-completeness.md)
(showing that sorting hid nothing)
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
for the smart tier and GPT-6.1 Sol (GPT-6 Astra available in config) for
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
- **No one-click money links.** A to-do's action opens the source email in
  mxr with the link it is about highlighted ("Open email to pay"), and
  shows that link's domain. mxr never opens a pay, verify or sign link
  from the row. Two review rounds found a way past every gate tried
  (forged authentication results, primed or backdated sender history,
  subdomains, lookalikes, redirects and click trackers), so the one-click
  link is deferred until a design survives review:
  [one-click-pay-link.md](../issues/one-click-pay-link.md).
- **Badges count work only:** Now, and To do's Now band.

## Sorting shows its work, so nothing feels hidden

BK, 2026-10-07: "since it reorders emails, I'm always thinking: am I
missing some important email? So from time to time I find myself jumping
to the inbox tab just to see emails in order of arrival." In Inbox he
reads nothing: he checks that the newest message arrived a few minutes
ago. The research is
[trust-and-completeness.md](../research/email-modes/trust-and-completeness.md);
settled as D119.

Every sorting inbox drew this fear. After one miss in Gmail's Promotions
tab a user checks it "regularly and meticulously"; Priority Inbox users
kept sweeping "Everything else"; people check spam folders where Gmail
puts wanted mail under 0.05% of the time. What helped was showing where
mail went and letting people fix it there (Outlook's notice of mail sent
to Other, SaneBox's digest), and duplicating rather than hiding the mail
that matters (Apple Mail copies time-sensitive mail into Primary). Lee
and See (2004) call this designing for appropriate trust: show the
automation's purpose, process and past performance.

BK's check is a reasonable one, and mxr should answer it where the user
already is. Three signals, in this order, each one step deeper than the
last:

1. **Mail is arriving.** The status bar on every page: "Latest mail 5m ago
   · synced 30s ago", a warning when sync fails, and a popover of the last
   five arrivals with the mode each went to (`GetFreshness`, being built
   on `feat/freshness-indicator`). This is the check BK makes in Inbox
   today.
2. **Every arrival is accounted for.** One line on Now whose counts sum to
   what arrived, each count opening those emails in arrival order.
3. **Some mail is never sorted away.** Four rules, stated in the app, that
   no other rule or model overrides.

Inbox mode chips, the "Not sure" line and the track record support these
three and are not separate places to look.

### The arrivals line on Now sums to what arrived

```text
Since 08:12, 50 emails arrived: 8 Messages, 10 Updates, 31 Reading,
1 in Spam. 2 also in To do, 1 in Archive.
```

- **Window.** Since the user last opened Now (`mode_views`), with the
  start of the day as the floor and 24 hours as the ceiling. The time
  shown is a clock time, never "since your last visit".
- **Unit.** Emails, not threads or people. On BK's mail the last seven
  days had 435 live arrivals in 423 threads and no thread split across
  two base modes, so counting emails loses nothing and matches the
  freshness popover.
- **Each count is a link** to Inbox filtered to that mode and window,
  newest first, with exactly that many rows. The proof that nothing is
  hidden is one click from the claim, and it is the arrival-order view
  BK goes to Inbox for.
- **Placement is stored when mail arrives.** Membership stays computed
  (D097), but it reads today's inbox, so archived mail falls out of every
  mode: 242 of BK's 435 arrivals in seven days had left the inbox. Sync
  writes one `arrivals` row per inbound message (account, message, first
  seen at, mode, rule, `now_mode` after a correction). The ledger, the
  freshness popover and the track record read it. Arrivals use first-seen
  time, never the `Date:` header (five messages in BK's store are dated
  more than a day ahead).
- **Quiet.** One muted line under Now's headline, no badge, no colour, no
  animation. A count of zero is left out. When the window has had no
  mail, the line reads "Nothing new since 08:12. Latest mail 2h ago."

### The arrivals reconciliation rule

For a window W, every inbound message first seen in W is counted exactly
once:

```text
arrived(W) = messages + updates + reading + screened_out + spam + sorting
also(W)    = to_do + archive        -- shown, never added
```

- **Primary mode** comes from the sender rules and the thread shape:
  Person to Messages, Person in a copied or crowd thread to Updates, List
  to Reading, Automated to Updates. A never-bury rule (below) can raise
  it, never lower it.
- **No base mode.** Delivery mail counts in Updates, where trackers
  live, with Archive as an "also" once a record is filed; an invite counts in To do when it made an RSVP row, else
  Updates; a screened-out sender counts as "screened out"; provider spam
  as "in Spam". Measured on BK's mail: 21 delivery emails in seven days,
  each also with an Archive record. Today's membership code puts them in
  no mode.
- **Snoozed** mail counts in its mode. Mail a provider filter archived on
  arrival counts in its mode too.
- **Sorting.** An email the rules haven't reached yet is "2 still
  sorting". When this stays above zero for more than one sync, the
  freshness line shows it, because it means the pipeline is stuck.
- **A failure is visible, not smoothed.** If the parts ever fail to sum,
  the line shows the difference ("1 not placed, open it") and `mxr modes
  arrivals --format json` reports it, rather than rounding to a tidy
  total.
- To do and Archive are aspects: "2 also in To do" names them without
  double counting.

Measured, counts only: the last 24 hours had 50 arrivals: 1 in Spam, 8
Messages, 10 Updates, 31 Reading; 2 also in To do and 1 in Archive. Seven days
had 461: 26 in Spam, 54 Messages, 236 Updates (21 of them deliveries),
145 Reading, 22 also in To do and 37 also in Archive. Both sum.

### Four kinds of mail are never sorted away

The rules are shown under the arrivals line's `?` and in `mxr modes
explain`, in these words:

> People you've written to always reach Messages. Sign-in alerts and
> failed payments always reach Now. Anything with a due date reaches To
> do.

| Rule | What it overrides | Where it goes | Measured, BK, 7 days |
|---|---|---|---:|
| N1. From an address you've written to, with you in To or Bcc | List-Unsubscribe, List-Id, list-sender and no-reply rules | Messages | 21 arrivals; 3 had gone to Reading |
| N2. A security alert: new sign-in, password or two-step change, account recovery | Updates' batching | Now's Updates card, as a line that stays until opened | 12 by subject (an upper bound); all in Updates, none in To do |
| N3. A failed payment | Updates | To do, surfacing at once (`payment_failed`, `crates/todo/src/detect.rs`) | 2 by subject or to-do; both already to-dos |
| N4. A dated deadline | Reading and Updates | To do on its runway | 13 by subject or due date; 6 already to-dos |

- Only the user's own sender decision (`K`) overrides N1. A first-time
  sender is not covered and keeps the new-sender question.
- N1 does not cover threads you were only copied on. Those go to Updates
  and onto the "Not sure" line (4 in seven days).
- N2 to N4 still obey relevancy windows (D115): a sign-in alert leaves
  Now after two days, and a one-time code or verify link is never an N2
  line. Reach is guaranteed; staleness rules still apply.
- Now's cap of three per section still holds. An N-rule item beyond the
  cap is named in the "and 2 more" line, never dropped.
- N2 needs a detector that doesn't exist yet. Until it ships, the "?"
  copy names only the rules that are true in code.

### Inbox shows where each email went

Inbox stays the everything view in arrival order. Each row gains the mode
name as quiet text beside the time ("Reading", "Updates"), and focus or
hover adds the reason: "Reading · has List-Unsubscribe", "Updates ·
no-reply sender". `K` changes the sender's mode from the row, and `X`
moves just this email. The chip uses the stored arrival placement, so an
archived email still says where it went. A row in no mode says so in
words: "In Spam" or "Screened out".

The rules behind BK's last seven days, as reasons: no-reply sender 187,
has List-Unsubscribe 134, from a person 58, automated sender 23, delivery
21, newsletter domain 11, automated domain 1.

### Uncertain placements are rule conflicts, and Now asks about at most three

No placement carries a confidence score: every rule is deterministic, and
the model tier (phase 7) is not built. A placement is uncertain when two
signals disagree:

| Conflict | Example | Now shows it as |
|---|---|---|
| U1. Someone you've written to, only copied | A colleague copied you on a thread you never wrote in | "Not sure" |
| U2. The fast-tier model disagrees with the rule (phase 7) | Rules say Reading, the model says it asks you something | "Not sure" |
| U3. First-time person | Already the new-sender question (D104) | not duplicated |

High-volume rules are not uncertain by definition. No-reply without list
headers (187 a week) and List-Unsubscribe alone (134) are ordinary mail,
and asking about them would make the line noise. A model's stated
confidence is not used as a threshold (see "Each model task gets the tool
that fits it"); disagreement is.

```text
Not sure: Maya Ortiz copied you on "Q4 plan". Updates for now.  m Messages · e fine
```

At most three a day on Now; the rest stay in their mode with a "not sure"
mark. When one conflict type produces more than one a day for a week, the
rule is changed rather than asking more.

### The track record waits for corrections to be counted

"Last week mxr sorted 435. You moved 2, both to Messages." This reassures
only if the second number can be non-zero. Today it can't: there is no
per-email correction, `screener_decisions` overwrites without the earlier
value, To do and mode requests are not in the activity log, and BK's
store has 0 sender decisions and 0 dismissed to-dos. It ships after `X`
and the `arrivals.now_mode` column, as one line in the arrivals line's
drill-down and on Monday's Now, never as a badge. It counts moves, not
misses the user never noticed, and says so in `?`: "Counts the emails you
moved. Mail you never opened isn't checked."

### Clear means accounted for, not seen

When Now is clear, the line merges into the end state: "Clear. All 50
emails since 08:12 are accounted for." It never says "you've seen
everything": the 31 Reading emails were sorted, not read.

### Copy

| Surface | Copy |
|---|---|
| Arrivals line | "Since 08:12, 50 emails arrived: 8 Messages, 10 Updates, 31 Reading, 1 in Spam. 2 also in To do, 1 in Archive." |
| Nothing new | "Nothing new since 08:12. Latest mail 2h ago." |
| Still sorting | "2 still sorting." |
| Not summing | "1 not placed. Open it." |
| Clear | "Clear. All 50 emails since 08:12 are accounted for." |
| Never-bury, in `?` | "People you've written to always reach Messages. Sign-in alerts and failed payments always reach Now. Anything with a due date reaches To do." |
| Inbox chip | "Reading · has List-Unsubscribe" |
| Not sure | "Not sure: Maya Ortiz copied you on "Q4 plan". Updates for now." |
| Track record | "Last week mxr sorted 435. You moved 2, both to Messages." |
| Track record, in `?` | "Counts the emails you moved. Mail you never opened isn't checked." |

### Rubric v3 X14

**Sorting shows its work.** Now's arrivals line sums to every inbound
email first seen in its window, each count opens exactly that many emails
in arrival order, and To do and Archive are shown as "also", never added.
Every Inbox row names its mode, including archived mail. The four
never-bury rules hold, and the "?" copy names only rules that exist in
code. "Not sure" shows at most three a day. Checked by a property test
over generated arrivals (the sum, and each count against its list), N-rule
tests in `handler/tests/modes.rs`, and on BK's mail, counts only: the
24-hour and 7-day sums, N-rule hits, and "Not sure" per day for a week,
recorded in the rubric. The outcome check is BK's: in `docs/dogfooding-log.md`
he notes each time he opens Inbox to check for missing mail, and that
count falls over two weeks.

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
than about 10 recipients, goes to Updates (messages.md §4). One exception
keeps the Your turn signal: a crowd thread stays in Messages while a person
has replied to you in it, with you in To, after your last message; once
you answer, it goes to Updates. Updates' early view lists these copied
threads by sender, with the reason "copied" (phase 3). Importance
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
|        |    [######----]  act by Wed 7 · due Fri 9        [ Open email to pay     ↵ ]  |
|        |    from Camden Council, 28 Sep · Here because: "payment due 9 October" (rule) |
|        |  Send the signed engagement form   Priya Shah (you promised)                  |
|        |    [########--]  act by Thu 8 · due Fri 9        [ Reply to Priya        ↵ ]  |
|        |  Verify your new sign-in email   Octopus Energy                               |
|        |    link expires today 18:00                      [ Open email to verify  ↵ ]  |
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
thing: opens the email with its link highlighted, starts the reply, or
answers the RSVP.
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

## The app teaches itself in place, with no tour

The modes ask people to drop habits that work in every other mail client,
so mxr has to explain itself a lot. It does that where each thing is used,
in the words of that mode, and never in a tour (D118). The research is
[teaching-in-place.md](../research/email-modes/teaching-in-place.md). In
short: NN/g found tutorials interrupt, don't improve task performance and
are quickly forgotten, and recommends help that arrives when the user
needs it; Carroll's minimal manual and its replication (Lazonder and van
der Meij, 1993) found short task-attached instruction teaches faster than
complete instruction; and explaining each of a classifier's decisions
raised users' understanding of it by 52% (Kulesza et al., IUI 2015). Fu
and Gray (2004) found experienced users keep generic habits that work, so
the moment to teach is when a Gmail habit meets a mode rule.

### Every mode ships six teaching surfaces

Nothing teaches at the top of a page, and there is no tour. A block of
explanation above the content has the same problem as a tour: it front-loads
information before the user needs it, or has the context to know where it
applies. Teaching sits next to the element it explains and shows at the
moment that element is first needed (D118, amended 2026-10-07).

1. **A header line** under the mode's name, always visible: what the mode
   is for in under 12 words, including its verb.
2. **Two empty states.** "Never had any" teaches the job, what lands here
   and how something gets here (including `t`). "Clear for now" states the
   fact and when the next thing arrives, as the end states in the first
   table say.
3. **Hints at their element.** A hint is one sentence that names its key,
   attached to one element: under the first row's why line, under the
   first runway bar, under a person's topic list. It shows the first time
   that element is needed, which means after the user has pressed a key or
   clicked on the page (never on arrival, unless the element is the only
   thing there), and never again once it is dismissed by `Esc`, its close
   button, or acting on the element, because a tip about something already
   used is noise (Apple HIG). At most one hint shows at a time. When one
   leaves, the next waits for its own element's next need, so dismissing a
   hint never reveals another: no chains. The web app shows a hint as an
   inline note under its element (`features/hints`); the TUI shows it in
   the status line while the cursor is on the element (`app/hints.rs`).
   The daemon stores the seen state per hint id and profile, so a hint
   dismissed in the web app never shows in the TUI.
4. **A why line on every item**, as "Every item says why" requires, plus a
   what-next fragment where the mode has a rhythm ("In the 16:30 digest",
   "Fades Sunday unless you keep it"). It names its evidence and its
   source (rule, you, or the model by name) and is never vague, because a
   low-soundness explanation costs trust (Kulesza et al., VL/HCC 2013).
5. **`?` leads with the mode.** The help that `?` opens today (web
   `HelpDialog.tsx`, TUI `Help`) starts with the mode's header, how it
   works (the guide's `about`), one line on what lands here, and links to
   the glossary and the mode's guide page, then the keys. `?` is the full
   explanation on demand, and how a dismissed hint is found again; there
   is no separate tips page.
6. **Keys with their verbs at the point of use**: the footer, tooltips and
   the palette show "e done here", never a bare key. Tooltips start with a
   verb and stay under 75 characters (Apple HIG). Superhuman and Raycast
   teach keys this way: the shortcut sits beside the action.

Across modes, handoff toasts name the destination and the undo (see
"Handoff names where the item went"). Archive the mode and archive the
Gmail action share a word, so toasts keep them apart every time: "Filed in
Archive" for a record, "Archived in Gmail" for the provider action, with
the user's own provider named.

The copy lives in one typed table in the daemon, served by `GetModeGuide`
and printed by `mxr modes explain [MODE] --format json`, so web, TUI, CLI
and agents use the same words and one test checks them. Each mode's guide
carries its hints with their seen state. Empty `mxr todo`, `mxr updates`
and the other mode commands print the empty-state line. `SetHintSeen
{ hint }` (`mxr modes hint ID`, `POST /api/v1/mail/hints/{hint}`) records a
dismissed hint; `--show` or `seen: false` brings it back. Acting through
the daemon dismisses too: sending Got it dismisses its hint from any
client.

### The hints

Each names its key, and that key does that verb in that mode (the table
test checks it). A hint shared by two modes has one id and one seen state.

| Id | Mode | Anchored to | Hint |
|---|---|---|---|
| `now.from_mode` | Now | The first row's why line ("From To do: …") | "Each row comes from a mode; Enter opens it there." |
| `done_here` | Now, Messages | The toast after the first `e` on a conversation (TUI: the status line) | "Done here (e) only clears this mode; it stays in To do until done there." |
| `updates.let_go` | Now (Updates once it ships) | The first "Let go of this digest" button; in the TUI, the Updates card row | "A lets go of this digest only; new mail arrives in the next one." |
| `todo.runway` | To do | The first runway bar | "The bar fills from when this showed up to when it's due; Enter does what the button says." |
| `todo.catchup` | To do | The catch-up line (TUI: the first row, under it) | "These came in before mxr sorted your mail. C goes through them: keep or let go of each." |
| `messages.topics` | Messages | A person's topic list, when it has more than one topic | "Every conversation with this person, yours to answer first; ] and [ step through them." |
| `messages.got_it` | Messages | Got it, the first time it has focus or the pointer (TUI: a row whose turn is yours) | "Got it (.) sends a short note that you've seen it and takes them off Your turn." |
| `archive.record` | Archive | The first record row | "Each row is one order, trip or bill, not an email; o opens the email it came from." |
| `archive.answer` | Archive | The first answer to a question | "y copies what this answer found; Enter opens the document." |
| `reading.fading` | Reading | The first Fading band (TUI: its first item) | "These go within a day; b keeps one on Later, which never fades." |
| `reading.link` | Reading | The first link under a digest | "Each link is its own item; L fetches its article, and only then does mxr contact that site." |

Updates adds its own hints in the phase that ships it, at the analogous
first-use point (a digest's let go).

The topics hint says "yours to answer first" because the daemon orders a
person's topics by state, your turn first, then by recency
(`messages_view::sort_topics`), not newest first.

### Copy for each mode

Drafts in BK's voice: plain, specific, no hype, and no "AI". The examples
use the demo mailbox. Dates and counts are filled in by code. Each "Card"
below was first drafted as a page-top card; since the 2026-10-07 amendment
it is the mode's `about` text, shown only by `?` and `mxr modes explain`,
and its key line is no longer shown on its own.

**Now**

- Header: "The few things that need you now, from every mode."
- Never had any: "Now fills in as mxr sorts your mail, newest first.
  People waiting on you, things due soon and the latest updates show
  here."
- Clear for now: "Clear. The next to-do surfaces Mon 09:00."
- Card: "Now shows at most ten things: people waiting on you, things due
  soon, the latest updates and, after 17:00, one thing to read. Acting on
  a row here does it in that row's own mode."
  Keys: `Enter` open in its mode · `e` done here · `?` what is this
- Why line: names the mode and the reason. "From To do: act by Wed 7 ·
  due Fri 9." "From Messages: Maya asked you something 22h ago."

**Messages**

- Header: "People you talk with, one row each. Reply or mark done."
- Never had any: "When someone writes to you and you've written to them,
  they show up here, one row per person, with what they asked you."
- Clear for now: "Nobody is waiting on you." Up to three people whose usual
  pace lapsed follow as facts: "Ari usually writes every week. Last: 19
  days ago."
- Card: "Each row is a person, not an email, with your conversations
  inside as topics. The quoted line is what they asked; the row leaves
  Your turn when you reply or press got it (.)." (Reworded so the key is
  plain text: clients show card text as typed.)
  Keys: `r` reply · `.` got it · `e` done here · `t` make it a to-do
- Why line: "Here because: Samir asked you a question, and you write to
  him often (rule)."

**To do**

- Header: "Things email asked you to do, ordered by when to act."
- Never had any: "When an email asks you to pay, sign, reply by a date or
  confirm something, it shows up here as one line: what to do and when to
  act. Press `t` on any email to add one yourself."
- Clear for now: "Nothing needs you. Next: renew car insurance shows up Mon
  19 Oct."
- Card: "Each row is one thing to do, written as what to do, not the
  email's subject. Act by the first date; the bar fills from when it
  showed up to when it's due, and Enter does what the button says."
  Keys: `Enter` do it · `e` tick off · `Z` schedule · `X` not a to-do
- Why line: "Here because: \"payment due 9 October\" (rule). Once you pay,
  the receipt files itself in Archive."

**Updates**

- Header: "Notifications gathered twice a day. Read the digest, then let go."
- Never had any: "Notifications from services and apps land here and are
  gathered into a digest at 08:00 and 16:30. Anything that needs you, like
  a failed payment, goes straight to To do."
- Clear for now: "Nothing new since 08:00. Next digest at 16:30."
- Card: "Updates gathers notifications into a digest at 08:00 and 16:30,
  one line per source, like your bank or GitHub, with what changed first. Anything that needs you
  goes to To do at once, so you can read this and let it go."
  Keys: `A` let go of digest · `e` let go of this source · `t` this needs
  me · `K` tune a source
- Why line: "Here because: automated sender, not a person (rule). In the
  16:30 digest."

**Reading**

- Header: "Newsletters you chose, as an edition. Read when you like."
- Never had any: "Newsletters and posts you subscribed to land here, with
  the ones you read most first. Nothing here is owed: there's no unread
  count, and items fade unless you keep them."
- Clear for now: "Nothing new since Tuesday. Later has 4 things saved."
- Card: "Reading is an edition of the newsletters you chose, with the
  sources you read most first. Nothing here is owed: items fade after a
  while unless you press `b` to keep them for later."
  Keys: `Enter` read · `b` later · `e` let go · `D` unsubscribe
- Why line: "Here because: you subscribed, and it has an unsubscribe link
  (rule). Fades Sunday unless you keep it."

**Archive**

- Header: "Receipts, orders, bookings and documents. Ask for what you need."
- Never had any: "Receipts, orders, bookings, bills and documents are filed
  here as records, one card per thing, not per email. Type what you
  remember, like \"lisbon booking\", and it answers with the field."
- Clear for now: none. Records stay. A query with no record match says so
  and falls back: "No record matches \"lisbon booking\". Searching all
  mail instead."
- Card: "Archive keeps records built from your mail: one card per order,
  trip or bill, and it answers in the field you asked for, like a booking
  reference. Archiving an email in Gmail is a different thing, and the
  toast always says which one happened."
  Keys: `/` ask · `y` copy reference · `Enter` open document · `o` the
  email
- Why line: "Here because: order confirmation with schema.org markup
  (checked). Return window closes Fri 12."
- Shipped with two revisions (phase 6): the clear state is "Records stay.
  Ask for one, or browse by month." because the guide table needs a line,
  and the add-one line is "Press `T` on any email and pick Archive to file
  it yourself." (see the phase 6 decisions below).

**Inbox** (a lens, not a mode) gets a header only: "Everything, newest
first. The modes hold the same mail, sorted."

### The first run is the moment the whole model is explained

The first run is the one time mxr shows all five modes together, because
the user is waiting anyway and their own mail is the example. Twitter's
sign-up rose 29% when it showed people content to follow before a blank
feed (Wroblewski, 2010), and Apple's HIG says to teach through doing.
This builds on "The first run classifies newest first" and adds no step.

1. While the first slice sorts, Now shows "Sorting your mail, newest
   first. Now fills in within a few minutes." with the progress line, and
   below it each mode's header line with a count as it fills.
2. When the first slice is done, one card replaces that list:

   ```text
   Your last two weeks, sorted
   Messages   14 people, 3 waiting on you        People you talk with
   To do       6 things, 2 to act on this week   What email asked you to do
   Updates    31 updates from 12 sources         Notifications, twice a day
   Reading     9 issues from 7 newsletters       Newsletters, when you like
   Archive   412 records                         Receipts, orders, bookings
   Already over, so not shown: 50 past invites, 63 quiet parcels.
   Enter open a mode   e close   ? what is this
   ```

   Enter on a row opens that mode, where its hints wait for first need. The
   catch-up card follows ("Catch up: 12 things from the last two weeks
   might still need you"), and keeping or letting go of each row teaches
   the two verbs every mode shares. The one question the first run asks,
   the catch-up window (D117), sits on this card with 14 days filled in.
3. The card closes on `e` and never returns; `mxr modes first-run
   --status` repeats the counts.
4. Before any account is added, the empty Now offers the demo: "Want to
   look around first? `mxr demo` opens a sample mailbox. Nothing you do
   there touches your mail." Like Linear's demo workspace, it's offered,
   never required.

### Teaching surfaces are part of each phase's definition of done

A phase is not done until its mode's header line, both empty states, its
hints, why and what-next lines, `?` content, toasts and key hints ship in
web, TUI and CLI with the copy above, or with revised copy recorded here.
Phase 1 built the mechanism with To do (`GetModeGuide`, `mxr modes
explain`, the copy test). Phase 2 added Now, the first-run card and `?`
leading with the mode on every screen. Each later phase fills in its own
mode's copy and hints. (Phases 1 to 6 shipped a page-top card per mode;
the 2026-10-07 amendment replaced those cards with hints.)

- **Tests:** a `mode_guide` table test (every mode has a header under 12
  words, an `about` of at most two sentences, no "AI", "smart", "magic" or
  exclamation marks, and every hint names its key as a word, with that
  key bound to that verb in the mode's keys); daemon tests for hint seen
  state (kept per id, first time kept, a shared hint is one hint, unknown
  ids refused); a CLI JSON snapshot of `mxr modes explain`; TUI tests (no
  tour on first launch, a hint in the status line only after a key on the
  page and at its element, Esc and acting dismiss it once);
  `e2e/teaching.spec.ts` (no card on first visit, no hint on arrival, a
  hint at its element on first need, never again after Esc or acting,
  across a reload and in the daemon's state, never more than one, no
  chain after a dismissal, axe in both schemes; both empty states render;
  `?` starts with the mode).
- **Check:** the five-second check below, recorded in
  `docs/dogfooding-log.md`.

### Rubric v3 grades teaching with a five-second check, and fails a tour

X13 in `docs/web-app-experience-rubric.md`: a new user can say what each
mode is for after one visit, and no tour exists. The check, in the
style of a five-second test (Lyssna's guide: brief exposure, then recall):
at least three people who have not used mxr each open every mode once in
`mxr demo`, with hints not yet dismissed and no explanation from anyone, for up
to 30 seconds. With the screen hidden, they answer "What is this screen
for?" and "What would you do with a row here?". A mode passes when at
least two of three name its job in their own words and its main verb. A
five-second test measures first impressions, not use, so it is a floor;
the dogfooding log covers use. A multi-step tour, chained coach marks, a
teaching block at the top of a page, a tips feed or a notification
advertising a feature fails X13 outright.

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
    relevant_until    INTEGER,                -- the window's end; NULL when open-ended
    window_source     TEXT,                   -- schema | ics | rule | default | user
    state             TEXT NOT NULL CHECK (state IN ('open', 'done', 'dismissed', 'expired')),
    expired_at        INTEGER,                -- set once; restore clears it
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
research notes, and each item store carries `relevant_until`,
`window_source` and its claim column (see "Items have a relevancy
window"): `update_facts` and `update_sources` (updates.md §4),
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

## Items have a relevancy window, and the first run uses it

A lead time says when an item starts to matter; a relevancy window also
says when it stops. Stale items and the first-run flood are one problem:
a notification for an event that already ended, and three years of old
to-dos on day one, are both items shown after their window closed. The
research is [relevance-and-backfill.md](../research/email-modes/relevance-and-backfill.md).
Settled as D115 and D116.

BK's real store shows both (read-only counts, 2026-10-02): 83 deliveries
listed as active, 63 of them with no event for over 30 days, because the
active list has no age bound (`DeliveryListFilter::Active` in
`crates/store/src/deliveries.rs`); 50 unanswered RSVP requests, every one
for an event already over; and 1,842 open promises the user made, 93%
undated and 1,447 from mail older than 30 days. As `ListTodos` is
specified above, To do would open with all of them. Windows clear the
dated ones. The undated residue needs a bounded catch-up, and is the
larger part.

### Every item carries the window its content implies

Each mode item gets `relevant_from` and `relevant_until`, plus a
`window_source` (`schema`, `ics`, `rule`, `default` or `user`); a to-do's
`relevant_from` is its `surface_at`. The end
comes from code, never from a model: schema.org (`Event.endDate`,
`Offer.validThrough`, `ParcelDelivery.expectedArrivalUntil`,
`Invoice.paymentDueDate`), ICS `DTEND` (`calendar_invites.ends_at`), Gmail's
`DiscountOffer.availabilityEnds`, or quoted words resolved by
`natural_time` ("expires in 10 minutes", "sale ends Sunday"). A model may
only pick among dates code found (D111). This is what Wallet passes
(`expirationDate`, `relevantDates`), Live Activities (`staleDate`),
Android (`setTimeoutAfter`) and Gmail's deal annotations already do;
iOS notifications carry no end time and leave removal to each app, which
is why a past calendar alert lingers.

| Kind | Relevant from | Relevant until | End comes from | After the window |
|---|---|---|---|---|
| Event or invite | arrival | event end; no end: start plus 1 hour | ICS `DTEND`, `Event.endDate` | RSVP to-do expires; Archive keeps the event only if accepted |
| One-time code | arrival | stated lifetime, else 10 minutes | quoted words; NIST SP 800-63B's 10-minute maximum | let go; never a record |
| Verify or confirm link | arrival | stated expiry, else 3 days | quoted words; Django's default | let go; never a record |
| Sign-in or security alert | arrival | 2 days | judgement | let go; stays in Inbox and search |
| Delivery | first email | delivered plus 1 day; "went quiet" 7 days past `expectedArrivalUntil`, or 14 days without an event when there's no ETA | carrier status, schema.org | delivered: order record in Archive; quiet: one "went quiet" line in the next cut, then let go |
| Bill | `surface_at` | due plus 14 days | `Invoice.paymentDueDate`, due words | expires; a reminder or final notice reopens the row by `dedup_key`; paid: record |
| Renewal | `surface_at` | renewal date plus 3 days | due words, schema.org | expires; the confirmation is a record |
| RSVP | arrival | reply-by date, else event start | due words, ICS | expires |
| Sale or offer | arrival or `validFrom` | `validThrough` or `availabilityEnds`, else 7 days | schema.org, Gmail annotations, words | let go; never a record |
| Newsletter issue | arrival | Reading's fade: twice the source's median interval, 2 to 14 days | the Reading rule above | `mode_done` for Reading; Later never expires silently |
| Promise you made | evidence date | dated: the date plus 7 days; undated: none | `by_when`, due words | dated: expires; undated: decays like owed, and the first run's catch-up applies |
| Person message | arrival | none: messages don't expire | | Your turn decays to Quiet after max(3 times their cadence, 7 days), capped at 30 days |
| Record | | none: records don't expire | | its "coming up" moment has its own window (return window close, trip end) |

Defaults without a source (sign-in 2 days, offer 7 days, the grace
periods) are judgement and configurable per kind and per sender through
`K`, like lead times. The bill's 14-day grace exists because a missed bill
keeps mattering until the biller says otherwise, and the biller's next
email reopens it.

### Expiry lets go in its mode, files records, and never drops what you made

- **To do.** Past `relevant_until`, a detected row moves to `state =
  'expired'` with `expired_at`, leaves the Now band and Coming up, and is
  one key from restore. A row the user made or touched never expires:
  `origin` of `manual` or `handoff`, any `field_sources` value of `user`,
  or a `scheduled_for` date. It stays as "was due Fri" until the user
  acts, as Sunsama exempts edited recurring tasks from rollover removal.
- **Updates.** A fact past its window drops out of any cut not yet shown
  and never breaks through to To do; an OTP that syncs after its 10
  minutes never surfaces. Trackers end as above.
- **Now.** Hard rule: nothing past `relevant_until` enters Now, and
  `GetNow` filters on it in the daemon, so every client agrees.
- **Archive.** Expiry never removes a record. Record-worthy items (an
  accepted event, a delivered order, a paid bill) file themselves, as they
  do on tick-off.
- **Messages.** Nothing expires; the turn decays (Kooti et al., WWW 2015:
  over 90% of replies come within a day, half within 47 minutes).
- **Provider.** Expiry is local. It writes mode state and never archives
  in Gmail; provider archive stays tied to a user action with its toast
  or preview (D098), so a background job never moves mail unseen.

Visibility is one quiet line, not a badge: "3 expired since you last
looked" at the foot of To do and Updates when the count is above zero,
opening an Expired list with restore (`u` or Enter). That is Sunsama's "N
Tasks moved to archive" and Wallet's Expired list, which hides expired
passes, keeps them and allows unhide. "Since you last looked" needs one
timestamp per mode, set when the mode is opened. Now never mentions
expiry. `mxr todo list --expired` and `mxr updates --expired` list the
same set in JSON.

### The first run classifies newest first and lets windows clear history

1. **Newest first.** Classification reads a queue ordered by message date,
   not by sync order: IMAP's initial sync fetches ascending UIDs, oldest
   first, and returns the folder in one batch
   (`crates/provider-imap/src/lib.rs`), and Gmail's list order is
   undocumented. The IMAP adapter pages newest first so a first IMAP sync
   shows something before the whole folder lands. The first slice, the
   last 14 days (917 messages on BK's store), runs through rules and the
   fast tier before anything else, so Now has People, Due soon and the
   Updates card within minutes.
2. **History in the background, with progress.** Newest to oldest,
   resumable, after the first slice: "Sorting your history: 2023, 61%" on
   Now and in `mxr modes first-run --status --format json`.
3. **Windows apply during classification.** An item whose window already
   closed is written expired at birth: `expired_at` set, `surfaced_at`
   never set, not counted in "expired since you last looked". One line
   after the run says what windows cleared: "50 past invites and 63 quiet
   parcels were already over." Detectors that create rows today (the
   delivery scan in `post_sync_fanout`, which runs on backfill pages too)
   apply windows on backfill pages as well.
4. **One bounded catch-up for the undated residue.** Open-ended items
   (undated to-dos and promises, owed replies) whose evidence is from the
   last 14 days form one batch, shown once on Now and in To do: "Catch up:
   12 things from the last two weeks might still need you." Each row is
   kept or let go with one key; "let go of all" previews first and the
   preview equals the commit, with undo (`SetCatchUp { dry_run }`, `mxr
   modes catch-up --dry-run`). It shows at most 25 rows, by act-by, then
   closeness; the rest join the Expired list with a count. Older
   open-ended items are expired at birth with the reason "older than your
   catch-up window". Fourteen days follows reply decay (Kooti) and sits
   inside Superhuman's one-month suggestion and the five weeks of mail
   Fred Wilson declared bankrupt in 2010; 25 is half the roughly 50
   overdue tasks at which Todoist's own writer stopped opening the app.
5. **Hard rule.** Nothing past its window enters Now, on the first run or
   after it, and on the first run nothing open-ended older than the
   catch-up window does either.
6. **History beyond 90 days is rules only.** Model output on old to-dos
   and updates would almost all be expired at birth, so the fast tier
   runs on the last 90 days (4,605 messages on BK's store) and rules,
   schema.org and sender labels run on everything, because records never
   expire. Arithmetic, not measured: at an assumed 600 input tokens a
   message, all 110,285 would be about 66 million tokens; the 90-day slice
   is about 2.8 million. Smart-tier record fields for older mail run
   lazily, when Archive opens or answers about that record. The horizon
   is config (`modes.model_history_days`, default 90).
7. **Re-runs never re-flood.** Every item keeps a stable claim: a to-do
   by `dedup_key` with `surfaced_at` and `expired_at`, an update fact by
   its breakthrough claim. A new rule, prompt or recipe version
   re-evaluates fields but never clears a claim, an expiry or a
   correction, and new items it finds in old mail pass the same window
   and catch-up cap, so a rule change produces at most one more catch-up
   batch, never a flood. Linear skips already-imported issues on re-import
   for the same reason.

The check on BK's real mailbox, counts only: the first run puts at most
10 to-dos in To do's Now band and at most 25 in the catch-up, and zero
items past their window appear in Now, To do's Now band or an Updates
cut. The run records expired-at-birth counts per kind against today's
baseline (83 active parcels with 63 quiet, 50 past RSVPs, 1,842 open
promises with 90 from the last 14 days). That 90 is the warning: undated
promise precision, not the window logic, decides whether the catch-up
fits under 25.

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
  `npm run build` in `site/`;
- ships its mode's teaching surfaces and copy ("The app teaches itself in
  place, with no tour"), and records the five-second check (rubric v3,
  X13).

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
merged in `ListTodos`; relevancy windows for bill, renewal, RSVP,
verify, code and promise rows, the `expired` state and the "expired since
you last looked" line with restore; the first run for To do (a dated
newest-first classification queue, expired at birth, the 14-day catch-up
capped at 25 with `SetCatchUp { dry_run }`, stable claims across
re-runs), with old promises entering To do only through it; `CreateTodo`,
`UpdateTodo`, `SetTodoDone { dry_run }`; `mxr todo` (`list`, `add --from
MESSAGE --due PHRASE`, `done`, `schedule`, `edit`, `undo`, `list
--expired`, all `--format json`), `mxr modes first-run --status` and `mxr
modes catch-up --dry-run`; the web view with bands,
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
  · due Fri 9" with "Open email to pay" and the council's domain, as does
  a lookalike-domain copy with its own domain; the receipt turns it into "Looks
  done"; a bill due in five days appears on the desk three days before,
  once, across a restart; an RSVP for an event that ended yesterday is
  never in To do. On BK's real mail `mxr todo --format json` lists
  his bills and renewals, and he records counts of right, wrong and missed,
  plus the extraction eval's local and cloud field counts. The first run
  on his mail puts at most 10 to-dos in the Now band and at most 25 in the
  catch-up, none past its window, and records expired-at-birth counts per
  kind (rubric X12).
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
  `todo_complete` matcher tests; `relevance_window` table tests per kind
  (ICS `DTEND`, `Invoice.paymentDueDate` plus grace, "code expires in 10
  minutes" and the 10-minute default, user-touched rows never expiring);
  `handler/tests/first_run.rs` with two years of fixture mail on a moved
  clock (newest slice first, nothing surfaced past its window, catch-up at
  most 25, preview equals commit, a re-run after a rule version bump
  surfaces nothing already expired or surfaced); `handler/tests/todo.rs`
  with a moved clock; CLI JSON snapshot; `e2e/todo.spec.ts`; TUI lens test;
  `index_recipe` tests (one embedding per distinct chunk text).

### Phase 2: Now, the rail, membership, per-mode done and the eval harness

`modes()` with rules only, `mode_corrections`, `mode_done`,
`ListModeItems`, `GetModeMembership`, `SetModeCorrection`, `SetModeDone {
dry_run }`, `GetNow`; `mxr modes`, `mxr why`, `mxr now`; the desktop rail,
mobile tabs and TUI sidebar with the new `g` keys and `K` menu; Now's four
capped sections with the relevancy filter in `GetNow`, the first-run
progress line and the catch-up card; the identity anchor and "Also in"; archive on last done
with its toasts and setting; the Screener as an inline question; `mxr
modes eval` in rules-only mode, so accuracy is measured before any model.
Index: the baseline at sync, version stamps, stale-only reindex and the
mode filter on search.

- **Check:** in the demo, "Action required: unusual sign-in attempt" is
  in To do or Updates, says why, and is never in People. The landlord's
  email is in Messages and To do; `e` in Messages toasts "Done in
  Messages. Still in To do" and Gmail keeps it; ticking off the to-do
  toasts "Archived in Gmail". Now never shows more than ten items, and
  never an item past `relevant_until`. `mxr modes eval --sample 200`
  prints rules-only counts on BK's mail.
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
  off, expiry never archives in the provider); `handler/tests/now.rs`
  (caps, more counts, nothing past its window on a moved clock); `e2e/now.spec.ts`;
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
breakthrough, deliveries as trackers, relevancy windows for codes,
sign-in alerts, offers and deliveries (a parcel with no event goes quiet
instead of staying active forever, fixing the unbounded active list),
`--expired` on `mxr updates`, `GetUpdatesDigest { cut }`, `mxr
updates` (`--cut`, `let-go --dry-run`, `tune`), the web briefing, Now's
card and the TUI lens. Index: one fact chunk per message, deduplicated by
template.

- **Check:** a day of demo notifications is two briefings of source lines;
  four parcel emails are one track; let go of a cut acts on exactly the
  previewed set and leaves what To do holds; a one-time code synced after
  its 10 minutes never appears; on BK's mail the 63 quiet parcels are not
  active.
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
  badge stays zero, expired facts never in a cut or a breakthrough);
  delivery quiet-window tests; `e2e/updates.spec.ts`.

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
- **As built:** the requests are
  `GetReadingEdition { mark_visit }`, `GetReadingItem`, `SetReadingLater {
  dry_run }`, `RecordReadingEngagement`, `FetchArticle`, `SaveHighlight`,
  `ExportReadingHighlights` and `SetReadingSource`; let go is `SetModeDone {
  mode: reading, dry_run }` and unsubscribe the existing `UnsubscribePurge {
  dry_run }`. Extraction lives in `crates/reading` (rules over `scraper`, no
  model) and fetched articles go through `dom_smoothie`, a maintained
  Readability.js port on the same HTML stack. Storage is migration 066.
  Three choices made while building: the unsubscribe evidence counts only
  issues that arrived after Reading was first opened and only mxr's own
  engagement, because letting go marks mail read at the provider and the
  read flag would call a skipped issue opened; expiry writes Reading's done
  mark and never archives at the provider, as "Expiry lets go in its mode"
  says; the article fetch refuses loopback, private, link-local and
  `.local`/`.internal` hosts by name and by every resolved address, pins the
  connection to the checked address, checks each redirect hop, uses no
  proxy and stops at 5 MB. Highlights are indexed as their own semantic
  chunk kind. Section-aware chunks and lazy embedding for whole issues are
  not built yet.

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
- **Decisions made while building it:**
  - Filing an email by hand is `T` (pass to a mode, then Archive), not `F`
    as the research note had it. `F` is the full-width reader in both
    clients, and `T` is this plan's key for passing an item to another
    mode, so the one key grows a menu as later phases add destinations.
  - In Archive, `e` opens the email, the same as `o`. Archive has no done,
    so `e` has nothing else to mean there, and the research note used `e`
    for the email.
  - `v` marks a card checked as a shortcut beside `,`, which also confirms
    one field at a time. The key map folded the research's `v` into `,`;
    confirming every unchecked amount and date at once is the common case
    after reading the card, so it keeps its own key.
  - An account, customer or policy number names the account, not one bill,
    so it doesn't merge bills: each bill stays a record and three months
    of them from one issuer form a series.
  - Smart-tier record fields and `mxr modes eval --extract` for records
    are not in this phase: rules and schema.org only, with every rule's
    amount and date unchecked until the user confirms it.

### Phase 7: Fast-tier classification for what rules can't tell, measured

The `ModeAspects` feature on the fast tier, the loopback-only rule with the
user's explicit cloud opt-in and API key, the 90-day model horizon over
history (`modes.model_history_days`) with rules only beyond it, the
`mode_aspects` cache,
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
- **Relevancy windows and the catch-up numbers** (sign-in 2 days, offer 7
  days, bill grace 14 days, a 14-day catch-up capped at 25) are judgement
  built from product precedent, not a study. Learn from the first run's
  expired-at-birth and catch-up counts on BK's mail, restores from the
  Expired list (a restore means the window was too short), and any
  moment BK sees something stale.
- **Teaching in place is enough for a new model of email.** The evidence
  is about tours versus contextual help in general, not about email
  modes. Learn from the five-second check per mode (X13) and from any
  moment in `docs/dogfooding-log.md` where someone opened `?` to
  understand a mode or used a Gmail habit that the mode doesn't support.
  If a mode fails the check twice, rewrite its header and card before
  adding any new surface.
- **Classifier errors are what users see first**, and Now's cap of three
  makes each one a third of a section. That is why the eval harness lands
  in phase 2.

## Decisions made on BK's behalf

BK asked on 2026-10-02 for these to be decided for him ("review and decide
for me, I just want a working app"). Each follows the research and the
rules above; D117 records them.

- **Pay links.** No one-click pay link in phase 1: the action opens the
  email with the link highlighted, and the link is never opened from the
  row. Every gate tried had a way around it (see
  [one-click-pay-link.md](../issues/one-click-pay-link.md)); the one-click
  link returns only with a design that survives review.
- **No weekly money total.** Per-item amounts only. A running total reads
  as financial advice and adds worry without an action.
- **Archive on last done.** Stays on by default. Toasts say which happened,
  and `modes.archive_on_last_done` turns it off.
- **Reading highlights.** They go to Archive search and a Markdown export.
  Obsidian waits until someone asks.
- **To do badge.** To do earns a rail badge only below one false to-do a
  week on BK's mail, measured by `mxr modes eval`.
- **Person merge.** Manual only. mxr suggests a merge when the names match
  exactly and you've written to both addresses, but never merges on its own.
- **Got it.** Stays on `.`, with the preview and the send countdown that
  can be undone.
- **Reading engagement tracking.** On by default and local, and switched
  off by the same `MXR_ACTIVITY=off`, so there is one privacy switch.
- **Records for taxes.** Invoices, receipts and statements go into the
  export. Archive never suggests deleting old statements.
- **Promises you made.** These never expire silently. Undated ones go
  through the one catch-up; dated ones stay "was due" until you act.
- **Catch-up window.** The first run asks once, defaulting to 14 days.
- **Expiry.** The "N expired since you last looked" line stays, with the
  Expired list one key away.
- **Bills past due.** They stay "was due" until you act; they don't expire.
- **Escalation model.** The default on OpenAI is GPT-6.1 Sol, the model
  OpenAI pitches as balancing intelligence and cost. GPT-6 Astra stays
  available in config.
- **TypeSafe Jev.** Not used. Its DPA covers only the customer and the
  customer's users, not third parties in mail.
- **Index recipes.** They stay in code as a versioned table and are not
  user config.
- **Gmail mail deleted while offline.** Deferred until after phase 1. The
  design is in `docs/issues/deletion-clears-derived-data.md`.
- **`mxr demo` on first run.** Offered only when no account is set up yet. A real mailbox goes straight to its own first run.
- **Dismissed hints.** The daemon stores dismissals per hint id, so a hint dismissed in the web app stays dismissed in the TUI. (Until 2026-10-07 this was one card per mode.)
- **Who takes the five-second check.** BK and two people new to mxr, during dogfooding. Until they have, the independent grader runs it on the demo mailbox.
- **Sign in with ChatGPT SDK.** Not built. See
  `docs/extractable-crates/14-chatgpt-sign-in-sdk.md`.
- **Updates leftovers and hidden mail (phase 4).** Anything not let go
  folds into the next digest instead of stacking a second one. Letting go
  of a digest takes the whole cut, including mail tuning hid (muted,
  changes only) and mail past its window, and the preview says how many of
  those it is, so nothing piles up out of sight in the inbox.
- **Breakthrough is on, not a suggestion (phase 4).** A new sign-in, a
  failed payment and a delivery problem become a to-do on arrival, once per
  message by its dedup key, only while inside their window and at most two
  days old. "Auto-actions ship as suggestions" above still holds for model
  placements; these three are rules with a fixed window, and the to-do
  expires with it.
- **The Updates fact chunk is deferred (phase 4).** Facts are cached in
  `update_facts` but are not yet a semantic index recipe; search still
  indexes Updates mail the way it indexes all mail.
- **Updates' store is migration 065 (phase 4).** 064 is Messages'
  `person_links`; versions apply by number, so phases can land in any order.
