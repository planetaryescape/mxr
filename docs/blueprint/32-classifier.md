# 32. How mxr understands mail, decides what deserves attention, and gets better

Draft for BK, 21 October 2026. Design only; no code has changed. Code
references are at `fd818964c` (origin/main) unless marked.

Email is five apps sharing one inbox ([22-email-modes.md](22-email-modes.md)).
The modes give each job its own experience. Under them sit three things
the modes all depend on and that today are built piecemeal: knowing what
each email is and what it asks of you, deciding which of all that
deserves your attention right now, and getting better at both from what
you correct. This document designs those three as one system over one
shared understanding of each email.

| Part | Its one job | It never |
|---|---|---|
| Understanding | Say what each email is, what it asks of you, whether to keep it and until when it matters | decides what you should do first |
| Attention (Now) | Choose what deserves the time you have now, across every mode | re-reads or re-classifies mail |
| Teaching and learning | Turn everything you correct into fixes and into measurement | changes anything without a preview, or learns from clicks |

The modes own doing the work. Now owns deciding which work. Understanding
owns knowing what the mail is. Labels and the eval own getting better.

This document supersedes these parts of 22: "Classification is rules
first…", "Model work runs in two tiers…", "Each model task gets the tool
that fits it", "Uncertain placements are rule conflicts…", "Four kinds of
mail are never sorted away" (as rules; the guarantees move into the
resolver and Now), and "Now shows at most ten things in four fixed
sections". It folds in the 9 October custom classification proposal
([custom-classification.md](../research/email-modes/custom-classification.md)) and the 18 October amendment to D119. Mode
experiences, the arrivals line, relevancy windows and the first run stay
in 22.

## mxr makes three promises, and today's code can keep none of them reliably

**Every email has a home you can see, explain and fix.** You can open any
email and see which mode holds it, the one reason that decided it, and
fix it at the scope you mean: this email, this sender, or mail like this.

**If you open Now, you get what you need to do right now.** BK's words
from the 19 October conversation: whether because of importance,
urgency, consequence, value or neglect. The contract is that ten minutes
in Now leaves you confident you haven't overlooked something more
important elsewhere in your email.

**It gets better without a bigger model.** Mistakes you notice, mistakes
it knows it might be making, and mistakes it is confidently making all
become measured fixes, checked against a labelled corpus so a fix never
quietly breaks what worked.

Today each of these is undermined by how the pieces were built: each for
the feature that needed it, each reasonable alone.

| Promise | What breaks it today | Evidence |
|---|---|---|
| A home | About eight placement paths re-run `classify` and `conversation_shape` and disagree; deliveries and invites count in Updates on the arrivals line but sit in no mode | `arrivals.rs:66`, `modes.rs:250,310`, `places.rs:215`, `updates.rs:684`, `gist_kind.rs:69`, desk lanes, Reading, `quiet_count` |
| A home | "You've written to them" is defined three ways; evidence SQL is duplicated nearly verbatim and rebuilt on every read | `desk_lanes.rs:187,245`, `desk.rs:346`, `places.rs:121`; `store/desk.rs:569-616`, `store/places.rs:214-242` |
| A home | Amounts parsed four ways, dates six or more, schema.org three; "automated sender" defined at least six ways; payment failed, verify and sign-in detected twice | `todo::money`, `updates::numbers`, `mail_kind` `AMOUNT`, `gist_numbers`; `todo::dates`, `records::rules`, `updates::window`, `natural_time`; todo `FAILED`/`VERIFY` vs updates `PAYMENT_FAILED`/`VERIFY` |
| A home | Two promise extractors accept a date the model wrote, against D111 and D115 | `commitments_extract.rs`, `relationship/src/commitments.rs:240` |
| Now | Four fixed sections, never reordered, three items each, Reading only after 17:00: a dashboard by mode, so a busy Messages starves an important update or reading | `now.rs:1-6`, `mode_rules.rs:381` |
| Now | No item carries what ignoring it would cost or how long it takes, so nothing can rank across modes | to-dos have act-by (`todo/src/timing.rs:27`); Reading has affinity (`reading/src/edition.rs:125`); nothing compares them |
| Better | Corrections are logged with the rule that misfired, and nothing reads them | `mode_corrections.rule`, migration 070 |
| Better | No demo email carries an expected answer; `mxr modes eval` doesn't exist; placements carry no policy version | `provider-fake/src/fixtures.rs:953+`; `ModesAction`, `cli/mod.rs:2654`; `effective_rows`, `arrivals.rs:192` |
| Better | Each feature decides alone whether it acts automatically; three preview stores implement the same token pattern | RSVP auto-done and renewal to-dos automatic, resolutions suggested; `TreatmentPreviews`, `CategoryRulePreviews`, `CategoryPreviews` |

The 18 October repair was the right fix, and it shows the pattern: what
an email is for and whether you owe a reply had to be pulled apart after
the fact, because the classifier had one axis where mail has several.

## One shared model: emails, assessments, items, surfaces

```text
 email ─▶ FACTS ─▶ ASSESSMENT ─────────────▶ ITEMS ──────────────▶ NOW
          observed  five answers, a home,     the units modes      ranks items by
          once      aspects, uncertainty,     show: a turn, an     cost of ignoring,
                    the deciding reason,      action, an update    when it lands, value,
                    a policy version          fact, a readable,    neglect and the time
                                              a record, each with  you have
                                              attention evidence
                          │                        │
                          ▼                        ▼
                 SURFACES show both: mxr's clients (CLI, TUI, web, MCP, desktop)
                 and each account's mail server (mxr/ labels, via the effects queue)
                          │
                          │ every correction, on any surface, is one TEACHING EVENT
                          ▼
                 overrides (email, thread, sender, domain, rule) ─▶ back into the resolver
                 LABELS (the right answer, recorded) ─▶ eval against the scenario corpus
                                                    ─▶ redesign ─▶ new policy version
```

- **Facts** are what can be observed about an email without interpreting
  it. Computed once, cached, versioned.
- **An assessment** is mxr's understanding of one email at one policy
  version. It is stored, and everything downstream reads it.
- **Items** are the units each mode shows, built from assessments: a
  conversation turn in Messages, an action in To do, a fact or tracker in
  Updates, a readable in Reading, a record in Archive. Each carries the
  evidence attention needs.
- **Projections** (modes, Inbox, the arrivals line, `mxr why`, Now) read
  assessments and items plus live state (in inbox, done, snoozed,
  window). None of them classifies.
- **Surfaces** are where projections are shown and teaching happens:
  mxr's own clients, and each account's mail server, where the
  projection is a set of `mxr/` mailbox labels. A surface never decides
  anything; it shows the assessment and reports what the person changed.
- **Teaching events** are every act that tells mxr the right answer, from
  any surface: a key in a client, a "Not sure" answer, a label moved in
  Gmail. One service applies them.
- **Provider effects** are every write mxr makes to a mail server: the
  archive on last done, an action rule's provider actions, and the
  `mxr/` labels. One queue carries them all.
- **Labels** record what the right answer was, from every teaching event
  and from misses mxr can detect on its own. They drive the eval and the
  redesign loop. (A *mailbox label* is the provider's tag: a Gmail label,
  an Outlook category, an IMAP keyword. The two are never confused in
  code or docs.)

## Understanding answers five questions about every email

An email isn't "a Reading email". It has a sender of some nature, a
purpose, possibly something it asks of you, possibly something worth
keeping, and a time it stops mattering. Asking these separately lets one
email live in several modes without special cases.

| Question | Answers | Decides |
|---|---|---|
| Who sent it? | person, list, machine, screened out, unknown | the home, with the next question |
| What is it for? | conversation, notification, content, document (can be several) | the home: conversation → Messages, notification → Updates, content → Reading |
| What does it ask of you? | a reply; an action (verb, object, due words, amount); nothing; unknown | Your turn in Messages; a to-do in To do |
| Is it worth keeping? | a record kind and its fields, or no | a record in Archive |
| Until when does it matter? | relevant-from, relevant-until, and where that came from | expiry in every mode; whether Now may show it |

The home is always one of Messages, Updates or Reading. To do and Archive
are aspects added on top. Unknown is a real answer to every question and
never silently turns into a claim: amended D119 established this for
reply obligation, and here it holds for all five.

### Each email is judged on its own, and a thread stays whole on screen

A thread can hold emails of different kinds: the landlord's question is
a conversation, the signed lease he sends back is a document, his note
that the boiler engineer is booked is a notification. So the unit of an
assessment is one email, which is also what the arrivals line counts.

A thread appears in every mode where one of its emails is live, opened
at that email with the rest of the thread around it, and the "Also in"
line names the other modes. Done in one mode releases that mode's emails
only. Gmail archives whole threads, so the provider archive (D108) still
happens only when no mode holds any email in the thread.

### Every email has a home, and "nothing decided" is measured

No inbound email ends in no mode. When nothing decides the home, a named
fallback places it: Messages when the sender may be a person (a wrong
guess there is seen and fixed in one key, and a real person is never
quietly buried), Updates when the sender is a list or machine but its
purpose is unknown. The assessment records that the fallback decided, the
item carries "not sure", and its reason says so: "No rule decided;
Messages for now". Deliveries and invites, in no mode today, get a real
home.

Fallback placements are a measurement. `mxr classify report` groups them
by their facts (sender shape, headers present, candidates found), so a
cluster that keeps landing in fallback is either a distinction a judge is
missing or, as BK put it, a mode nobody has thought of yet. A new mode
still has to earn its place the way the five did, with its own unit,
verb, rhythm and end state (22, first table); a cluster is the evidence
that starts that conversation, not the decision.

### "Why here" shows the answer that decided

Every item shows one reason: the deciding answer and its source.
"Updates · no-reply sender". "Reading · your rule Bank statements".
"To do · asks you to renew, due 14 Oct". The full picture, every answer
and every overridden claim, is in `mxr why --format json` and the eval,
not on screen. The reason is rendered from the deciding claim's evidence,
so the explanation can't drift from the decision.

## Understanding runs as four stages, once per email per policy version

```text
 1 FACTS          2 JUDGMENTS          3 DECISION            4 PROJECTIONS
 observe once ─▶  one judge per    ─▶  one resolver,     ─▶  modes, Inbox, arrivals,
 shared parsers   question: rules,     one precedence        items, why: read the
                  then a model only    ladder, a stored      stored assessment,
                  where rules say      assessment            never classify
                  unknown
```

### Facts are the only code that reads the raw message

One record of facts per email, built at ingest and cached by content hash
plus facts version:

- header evidence (today's `MailEvidence`, `core/src/mail_evidence.rs`)
- one sender context with one definition of written-to, inbound and
  outbound counts and template history (today three builders)
- thread inputs: recipients, where your addresses appear, reply references
- parsed candidates: every amount, date phrase with its resolved date,
  schema.org object, tracking number, link with its domain, the authored
  text and its request sentences

Each kind of candidate has one parser in one crate, and every judge,
item builder and Now read the same result. The strongest existing parser
of each kind becomes the one (`mxr_todo::money` for amounts,
`natural_time` for dates, one schema.org reader). Facts hold
observations, never verdicts: no "is automated" flag lives here, only
what an automated-sender judgment looks at.

Mailbox labels are facts with one exception. A user's own labels and the
provider's own categories (Gmail's Promotions, for instance) are evidence a
judge or a rule may use. mxr's own `mxr/` labels are never facts: they are
the assessment's projection, so reading them would make a judge read its
own answer. A change someone makes to an `mxr/` label reaches the classifier
only as a teaching event.

### Judgments answer each question with evidence, or abstain

Each question has one judge, and each judge returns a claim:

```text
Claim { question, answer, evidence: fact references or verbatim quotes,
        source: built-in rule (id, version) | model (name, prompt version)
                | your teaching (email, sender, domain, rule),
        policy version }
```

Rules answer first. Today's `mail_kind` heuristics become the Who and
What-for judges with their logic kept; the to-do detectors become the
What-it-asks judge; records detection becomes Worth-keeping; the window
tables become Until-when. A model answers only where rules return
unknown. A judge never reads another judge's answer, so nothing can be
circular, and a missing input (an unfetched body) makes a claim pending,
not false. Claims carry evidence, never scores.

### One resolver holds the whole precedence ladder

One function turns facts, claims and your teaching into the assessment.
It is the only place precedence lives:

1. Boundaries: spam, trash, blocked. Nothing below can restore them.
2. Your move of this one email (`X`) and your choice for this sender or
   domain (`K`), ordered by time between themselves as built today
   (`arrival_ledger.rs:132-144`).
3. Your teaching rules, highest priority first, stable rule-ID tie-break.
4. Built-in rule claims.
5. Model claims, only for questions rules left unknown.
6. Never-bury floors: security alerts, failed payments and dated
   deadlines always gain their aspect and are marked so Now can't starve
   them. Floors only add; they never lower a placement.
7. Fallback, when nothing decided the home.

The assessment stores the home, the aspects, the window, the claim that
won each question and the claims it overrode, the uncertainty, and the
policy version. Re-running the same version on the same facts changes
nothing, so sync replay can never undo a decision or a correction.

### Uncertainty means claims disagree or something that matters is unknown

No confidence scores, as D119 decided. An assessment is uncertain when:

| Kind | Example |
|---|---|
| Two independent claims conflict | Who says a person you write to; the thread says you were only copied (today's only conflict) |
| A model disagrees with a rule | rules say content, the model finds a request to you |
| The home was a fallback | a first-time sender with no list headers and no recognisable request |
| A high-consequence answer is unknown | an amount is present but the action judge abstained |

Uncertain mail is still placed, marked "not sure" in its mode, and the
first three a day reach Now as one-key questions. The cap counts
questions shown, not candidates (fixing trust follow-up 1). Keeping an
email where it is counts as an answer. When one kind of uncertainty asks
more than one question a day for a week, that is a defect in a judge and
goes to the improvement loop instead of asking more. Every answer is a
label.

## Items carry the evidence Now needs, so Now never re-reads mail

Now compares a person waiting, a bill, a production alert and an essay.
That's only possible if every item states, with evidence, what ignoring
it would cost, when that lands, roughly how long it takes, and how much
you tend to value it. These come from the assessment and the item's own
data, are computed in code, and are classes with evidence, never scores.

| Item (mode) | What ignoring it costs | When that lands | Rough effort | Value |
|---|---|---|---|---|
| Turn (Messages) | a person directly asked you; how close they are (history both ways) | their usual reply pace passed (desk pace, `desk_lanes.rs:26-30`), or a date they named | an acknowledgement is quick; a question takes longer | contact history |
| Action (To do) | by kind: security and failed payment severe; legal, money and renewals material; the rest minor | act-by on the runway (`todo/src/timing.rs:37`) | a table by action kind (pay, sign, form, book), adjustable per item | — |
| Fact (Updates) | needs-you signals (sign-in, failed payment, incident) severe; the rest none | its window (sign-in two days, `updates/src/window.rs`) | a minute | source history (let go vs opened) |
| Readable (Reading) | none: reading is never a debt | its fade (`reading/src/fade.rs`) | from length | affinity (`edition.rs:125`), and what you send to Marginalia (D121) |
| Record (Archive) | none until a date in it nears, then it raises an action | its "coming up" date | — | — |

Severity is a short ordered set (severe, material, minor, none) with the
evidence that put the item there. The effort table is code, starts as
judgment, and is tuned from what you do.

## Now decides what deserves the time you have, across every mode

The five modes describe the kind of interaction an item needs, not how
much attention it deserves. A casual message can matter less than a
security alert; an essay you've been meaning to read can be worth more
than five trivial replies. When the modes are read as a priority order,
ten-minute sessions get spent on Messages and To do, and the important
update or reading never surfaces. That is priority starvation, and it
makes mxr an easy way for a procrastinator to sort what they'll ignore.
Now fixes it by choosing across modes.

### Now has two parts: what can't wait, and the next best thing

**Can't wait.** Items whose cost lands before your next likely visit
(estimated from your Now visits, `mode_views`), plus every never-bury
item inside its window. Always shown, however little time you have,
ordered by when the cost lands. If nothing qualifies, nothing is shown
here, and Now says so.

When what can't wait is more than the time you said you have, Now still
shows all of it, because hiding something that can't wait breaks the
promise. It orders the items so the ones that land first and fit come
first, marks the rest with their effort and deadline ("start tonight: 20
min, due tomorrow"), and says plainly: "More than 5 minutes needs you
today." (BK, 21 October 2026.)

**Next best.** One recommendation at a time, the way Marginalia offers
reading (BK, 21 October 2026). Now picks the item that best uses your attention, says why
("Bill due Thursday, more pressing than the replies waiting"), and after
you act, skip or let it go, it chooses again from the new state. It isn't
a list you work down and it doesn't commit you to an itinerary.

The choice is made in code, by ordered rules rather than a weighted
score, so every pick names its deciding reason:

1. Higher severity before lower.
2. Within a severity, the cost that lands sooner first.
3. Neglect promotes: an item passed over while relevant rises one step
   after it has been eligible for several visits, and a mode with
   valuable items you haven't touched for days gets its best item
   considered. "You've passed over this three times" is a reason.
4. Value admits optional things: a readable or an update with no cost is
   offered only when nothing above it is pending, or when neglect
   promotes it, and only when your history says you value it. Unread is
   never a reason.

No mode has a fixed rank and none has a quota. If the five things that
most deserve you are updates, Now shows updates.

### Telling Now how much time you have is optional

Opening Now immediately shows what can't wait and the next best item.
A quick choice of 5, 10, 20 or 30+ minutes turns it into a session: Now
prefers items that fit, and an item too big for the time but too
important to skip is offered as "start" rather than hidden. Now never
writes a plan for you, such as model-made steps for a long task. Sessions
end on the end state, not on a count of items done, because ten minutes
of easy low-value work is not a good session.

### Now knows when to leave you alone

When nothing can't wait and nothing passes the value bar, Now says
"Nothing needs you before Thursday", and the arrivals line confirms every
email since your last visit is accounted for (22, "Clear means accounted
for, not seen"). It offers one readable on its own only when that's
earned: your history says you value the source, and you haven't read in
a while ("Nothing needs you. You've spent the week on admin, and here's
something from a writer you always finish."). Never more than one, never
framed as owed, and one key says "not now" (BK, 21 October 2026). It never manufactures
urgency, and it never learns its ranking from what you click, because
that would teach it your procrastination. It learns only from explicit
attention feedback and from misses it can detect.

## Surfaces show the assessment and take teaching, and the mail server is one of them

BK, 21 October 2026: mxr's sorting is written back to each account's mail
server as ordinary mailbox labels, and changing one there teaches mxr. In
this design that isn't a feature beside the classifier. The server is a
surface like any client: it shows the same projection, and what the
person changes on it arrives as the same teaching events a key press makes.

### The server shows the projection as `mxr/` mailbox labels

| Mailbox label | Projects | Changes when |
|---|---|---|
| `mxr/Messages`, `mxr/Updates`, `mxr/Reading` | The assessment's home | The home changes: a teaching event, a rule, an explicit reclassification |
| `mxr/To do`, `mxr/Archive` | An aspect the email carries | The aspect is created or dismissed |
| `mxr/Open` | Item state: something in it still needs you in some mode | It's done, snoozed or expired in every mode that holds it |
| `mxr/<category>` | A personal category set to sync | The category is added or removed |

Home and aspect labels are permanent filing tags, so a ticked-off to-do
keeps `mxr/To do` and `label:mxr/to-do` finds every to-do email ever;
only `mxr/Open` is live. Each personal category has its own setting,
synced or local, and mode names are reserved for modes. The labels are a
pure function of the stored assessment, item states and category
memberships, so the server can never show something mxr's own views don't.

### A mirror reconciles three states per message

For each message on a labelling surface, the mirror keeps:

| State | What it is |
|---|---|
| Desired | The `mxr/` labels the assessment, items and synced categories imply now |
| Written | The `mxr/` labels mxr last confirmed on the server, with the provider's revision marker (Gmail history id, IMAP `MODSEQ`, Outlook change key) |
| Observed | The `mxr/` labels sync sees now |

On every sync and every assessment change it does one thing in order:

1. **Observed differs from written** after the written revision: the
   person changed it on the server. The difference becomes teaching
   events (next section), which update overrides, which update the
   assessment, which updates desired.
2. **Desired differs from observed:** the difference is enqueued as a
   provider effect. When the provider confirms, written takes the new
   state and revision.

mxr's own writes are recognised by revision, not by guessing from values,
so an echo of its own write is never mistaken for a correction, and a
correction made while mxr's write is in flight is still seen. Where a
provider gives no revision marker, the mirror falls back to comparing with
written and says so in diagnostics.

### What the server labels, and with whose consent

New mail and the last 90 days of existing mail are in scope; older mail
comes into scope when it enters a mode again (a reply, a new to-do or
record, an open from search). Mail out of scope is left as it is on the
server, neither labelled nor cleaned. Nothing is written until the person
agrees: once at setup, and once after the upgrade that adds this, mxr says
which labels it will write and how many emails the first pass covers. The
setting is per account. Turning it off offers, through the same preview and
effects path, to remove every `mxr/` label mxr wrote.

Providers expose this as one capability on `MailSyncProvider`: read the
`mxr/` labels on a message and set a desired set of them, mapped in the
provider crate to Gmail labels, Outlook categories or IMAP keywords (names
sanitised to the IMAP keyword charset; a server whose `PERMANENTFLAGS`
won't take new keywords reports the surface as unavailable). The order is
Gmail, then Outlook, then IMAP, all in scope; IMAP keywords are invisible
in many mail apps, and the setup screen says so.

### Server health is part of the trust surfaces

A label backlog or a failing write shows where sync health already shows:
the freshness line ("Gmail labels 120 behind"), `mxr doctor` and sync
status. A server change that can't be honoured (removing a home label with
nothing in its place: every email has a home) is written back and named in
`mxr why`, never dropped silently.

## Teaching is one model for everything you tell mxr

There are three scopes of teaching about what mail is, plus one kind of
feedback about attention.

| Scope | How | Stored as |
|---|---|---|
| This email | `X`: a picker on any email | an email move (exists) |
| This sender, or everyone at its domain | `K`: exact address by default; "everyone at barclays.co.uk" is a second, deliberate choice in the same menu | a sender choice (exists; domain scope is new) |
| Mail like this | a teaching rule, prefilled from the email you started at | a rule (exists for treatment; extended below) |
| Attention | on a Now recommendation: "not now" (snooze, no label), "not worth my attention" (lowers value for that source or kind) | an attention label |

`K` covers the exact address by default because a company's alerts and
its marketing often belong in different modes, and a domain can include
real people. All three scopes appear in one teaching list, so you can see
in one place everything you've taught mxr and undo any of it.

### Every correction is one teaching event, whatever surface it came from

```text
TeachingEvent { scope: email | thread | sender | domain,
                change: home → mode | add/dismiss aspect | done in all modes
                        | add/remove category | attention feedback,
                source: client key | Not-sure answer | server label change
                        | accepted audit finding | accepted rule suggestion,
                at, account }
```

One teaching service applies every event the same way: it writes the
override at its scope (email moves and sender choices keep their storage
and time-ordered precedence), records a label at the current policy
version, writes the activity entry, and triggers reassessment of what the
scope covers. Rules stay a separate, persistent kind of teaching with
their own planner; an accepted rule suggestion creates a rule, not an
event.

A server label change maps onto the event a key would make:

| Change on the server | Teaching event |
|---|---|
| A home label replaced by another (`mxr/Updates` → `mxr/Messages`) | home → that mode |
| `mxr/To do` or `mxr/Archive` added or removed | add or dismiss that aspect |
| `mxr/Open` removed | done in every mode holding it |
| A synced category's label added or removed | add or remove that category |
| A home label removed with nothing in its place | none: rejected, written back, named in `mxr why` |

Thread scope exists because Gmail's apps apply a label to every message in
a conversation. When one sync sees the same change on every message of a
thread, the mirror emits one thread-scope event; it applies to the emails
already in the thread, and later emails are judged on their own. In mxr's
clients, `X` on a conversation offers the same scope.

### Rules split in two: teaching rules sit inside understanding, action rules run after it

Today one rule can mix two jobs. `SetTreatment` answers a question about
the email; label, archive, forward, category and shell actions automate
something (`crates/rules/src/action.rs:7-35`). The engine already treats
them differently: one winning treatment, every other action accumulated
(`engine.rs:42-97`). This design makes that split the model.

| | Teaching rules | Action rules |
|---|---|---|
| Do | Answer one of the five questions: treat as Messages, Updates or Reading; create a to-do; file a record; set a window | Do something: label, add a category, archive at the provider, forward, run a shell hook |
| Run | In the resolver, rung 3 | After the assessment is stored |
| Conditions read | Facts only, so a rule can never depend on its own result | Facts and the assessment: "when it lands in Reading and is from X" |
| Conflicts | One winner per question; the loser shows as shadowed in preview | Every matching action runs, as today |
| Replay | The same rule version on the same facts changes nothing | Provider and shell effects run once per email per rule version, never on reclassification |

One rule may still hold both kinds; the daemon runs each part at its
stage, and preview shows both. "Bank statements: treat as Updates, and
add the Finance category" stays one rule. Today rules run first in the
post-sync fan-out (`loops.rs:1119-1219`), before to-dos and records
exist; action rules move to after the assessment. An action whose
condition needs an unfetched body waits for it rather than evaluating
false, as `evaluate_known` already does (`condition.rs:120`).

Categories ("Finance", "School") answer a different question, what mail
is about, and compose with modes: a Finance category can hold a
conversation, a bill and a receipt without forcing them into one
workflow. They stay a local store with stable IDs that never writes to
the provider (`docs/guides/personal-categories.md`), assigned by hand or
by action rules.

### Rules and built-in judges speak one condition language over facts

Sorting rules may only use header conditions today
(`rule_treatment.rs:27-75`), while built-in judges see sender history,
thread shape and parsed candidates. Conditions extend to the facts:
"someone you've written to", "has a List-Id", "has an amount", "has a due
date", "schema.org Invoice", "you are only copied". Then you can say
anything the classifier can, and a built-in judge can be explained in the
words you'd write. Built-in judges stay code, because the request grammar
and document recognition are richer than a condition tree, but each has a
stable ID, a version and a plain name, so "Why here" names a built-in
rule and your rule the same way, and yours always outranks it.

### One planner previews and applies every kind of teaching

A rule preview runs the resolver with the candidate rule added and shows
the assessment diff: current and proposed home, aspects to be created or
reused, provider actions, the winning rule, shadowed matches, and your
email and sender choices that will be kept. It is the same planner as
`mxr classify eval --compare`, so preview and apply can't disagree, and
the eval can report what your rules contribute (`--rules on|off`). The
three preview stores that exist today (sorting rules, category rules,
category membership) become one token mechanism. New rules apply to
future mail by default; existing mail is an explicit, bounded selection
with honest counts, as `rule_treatment.rs` does now.

## Learning: every correction is a label, and every change is a measured redesign

### Labels record the right answer, separately from the fix

An override changes placement; a label records that mxr, at a given
policy version, got a question right or wrong. Every act that says what
the right answer was writes one, and so do misses mxr can detect itself.

| Label source | About |
|---|---|
| Email move, sender or domain choice, in a client or on the server | understanding |
| Not-sure answer, including keeping it where it is | understanding |
| To-do dismissed or edited, record field fixed | understanding |
| A correction against one of your own rules | that rule |
| Audit finding you accepted | understanding |
| "Not worth my attention" on Now | attention |
| Detected miss: an item became overdue or expired while eligible and never offered by Now, or you acted on it in its mode after its cost landed | attention |
| Scenario expectations in the corpus | both |

Labels stay local. Evals on your mail report counts, with message IDs
available only on your machine for review.

### Five sources catch five kinds of mistake

| Source | Catches |
|---|---|
| Your corrections | mistakes you notice |
| Not-sure questions | mistakes mxr knows it might be making |
| Audit (red-teaming) | confident mistakes, which never reach Not sure |
| Detected attention misses | things Now let starve |
| The scenario corpus | regressions when anything changes |

### The demo dataset is rebuilt from labelled scenarios and becomes the corpus

No labelled set exists today. The demo generator picks a sender pool by
`thread_num % 13` (`fixtures.rs:1043-1052`) and the curated fixtures
state their intent in prose (`modes_demo.rs:1-14`). The rebuilt generator
is scenario-first: a scenario is a small typed template that produces
mail and declares the expected answers for every email it produces.

```text
Scenario "bill with due date from a utility" {
  sender:   machine, no-reply local part, own domain, no List-Id
  variants: subject phrasings ×6, body phrasings ×8, html-only ×1, schema.org Invoice on/off
  expect:   who=machine, for=[notification, document], asks=action(pay, due words),
            keep=record(invoice), until=due+grace, home=Updates, aspects=[To do, Archive],
            severity=material, lands=act-by
}
```

- Every email carries its label by construction. The 50,000 email demo
  is drawn from scenarios with a fixed seed, so the same command always
  produces the same mail and the same expectations.
- Near misses are scenarios too: for each scenario, a twin that differs
  in the one fact that should change the answer. A newsletter from a
  person you write to, a receipt that asks a question, "no need to reply"
  on the next line, a copied thread that names you. These are where a
  classifier splits hairs, and most of the curated set is made of them.
- Scenarios compose into threads whose emails have different expected
  answers, which exercises per-email judgment.
- Attention scenarios are timelines: a mailbox state, a clock, a series
  of visits with time budgets and actions, and what Now must do. "A
  severe item is offered within one visit." "A valuable readable passed
  over for a week is offered when nothing is pending." "Reading is never
  shown as owed." "With nothing pending, Now says so."
- One dataset serves the demo, screenshots, e2e tests and both evals, so
  the demo can't drift from what the system is graded on. Today's curated
  `*_demo.rs` fixtures become scenarios, their prose turned into `expect`
  blocks.
- Real mail shapes scenarios without entering them. A new kind of miss
  found by audit or a correction becomes a scenario written from its
  shape (headers present, sender kind, phrasing pattern), never a copy of
  the email.
- Some phrasing variants are held out of anything an agent sees while
  changing a judge, and the eval reports held-out accuracy separately. If
  held-out accuracy falls behind, the judge learned the generator, not
  the mail.

Each expectation states its label-schema version, so changing the
questions doesn't silently invalidate old expectations.

### Two evals, a quick set and a full set

```text
mxr classify eval --set quick|full|mine [--compare <policy version>] [--rules on|off]
mxr now eval      --set quick|full
```

`classify eval` reports, per question, precision and recall, a confusion
table, deadline and field accuracy, fallback rate and held-out accuracy;
with `--compare`, every email whose answer changed between versions,
grouped by the claim that changed it. `now eval` replays the attention
timelines and reports every expectation broken. `mxr draft eval`
(`handler/draft_eval.rs`) is the pattern to reuse. These replace the
planned `mxr modes eval`.

The quick set (about 2,000 emails: every near-miss twin, every attention
timeline, and a seeded sample of ordinary cases) is what agents run after
every change. The full set (all 50,000) runs in CI before merge. The demo
stays at 50,000 for performance work. Run times are estimates until the
first build measures them.

### Audit is red-teaming as a command

`mxr classify audit` samples recent confident assessments, gives each to
a reviewer model with its claims and evidence, and asks whether any
answer is wrong and why. Findings are proposals, never labels: the
reviewer can be wrong, so you accept or reject each, and only accepted
findings become labels. It follows the same privacy rules as background
classification.

### A change is a redesign checked against the corpus, not a patch

```text
labels cluster (same claim, same kind of miss), or a fallback cluster grows,
or one uncertainty kind asks too often, or one of your rules decides a lot
  └─▶ an agent reads the cluster, the judge or Now rule involved, and its history
        └─▶ redesigns it with the new distinction, rather than adding an exception
              └─▶ writes the cluster's cases into the corpus as scenarios and twins
                    └─▶ quick eval while working; full eval in CI:
                        no regression, the cluster fixed, held-out holding,
                        every changed answer explained
                          └─▶ ships under a new policy version
```

The agent's instructions live in the repo as a skill, so any agent runs
the loop the same way. The corpus is never edited to make a change pass.
Each policy version is recorded on the assessments it produced, so
reclassifying existing mail under a new version is explicit, previewed
and comparable.

Two of the loop's inputs come straight from teaching. When several of
your corrections share a pattern, mxr proposes a teaching rule prefilled
from them, through the same planner. A teaching rule of yours that
decides many emails is evidence that a built-in judge is missing that
distinction; `mxr classify report` lists your rules by how much they
decide.

## What happens without asking is one table, raised only by measurement

Every effect the system can cause is listed once with its level. An
effect moves from suggestion to automatic only when the eval on your
labels shows its precision above a bar recorded in the rubric.

| Effect | Level |
|---|---|
| Placing every email in a home | automatic |
| Creating a to-do from a detected action | automatic (BK: detected to-dos are created) |
| Creating a renewal to-do from a yearly record | automatic (a missed renewal costs money; a wrong one is dismissed, and that dismissal measures it) |
| Closing an RSVP to-do when you answered the invite | automatic (your own action is the evidence) |
| Expiring an item at the end of its window | automatic, never a to-do you made or edited, never at the provider |
| Writing `mxr/` labels to the mail server | automatic, once the person has agreed at setup or upgrade |
| Closing a to-do because a receipt arrived | suggestion |
| An update breaking through to To do | suggestion |
| Generalising corrections into a rule | suggestion |
| Accepting an audit finding | suggestion |

### Every write to a mail server goes through one effects queue

Three things write to providers today, each its own way: the archive on
last done (D108), action rules' provider actions (label, archive), and now
the `mxr/` labels. They share one queue. Each effect is keyed by message,
kind and the state it moves to, so a retry or replay never writes twice;
the queue batches to each provider's limits (Gmail's batch modify takes up
to 1,000 messages), backs off on throttling, and records every outcome.
Bulk effects (a first labelling pass, turning labels off, an explicit
reclassification) are planned and previewed by the same planner as rules
and reclassification, so the preview counts include the server writes, and
the selection previewed is exactly what runs. Failures surface in sync
health, never only in a log.

## Models are optional judges, never the foundation

Everything above works with no model. A model is one more judge behind
the same interface: it answers only questions rules left unknown, or
reviews in audit. It never writes a date or an amount: code finds the
candidates, the model selects one or answers none, and the daemon checks
quotes verbatim, as the to-do resolution worker already does
(`todo_resolution/worker.rs:262-290`). Its claim names its model and
prompt version, so "Why here" names it and the eval can compare it with
the rules.

D114's two tiers keep their shape with sharper jobs:

| Tier | Job |
|---|---|
| Fast | Who and What-for where rules say unknown; the model-versus-rule disagreement check |
| Smart | Fields for actions and records, only on emails already holding that aspect; audit reviews |

Structured output (JSON schema on `OpenAiCompatibleProvider`) comes
first; today every feature parses free text.

## The invariants hold throughout

- Local-first and daemon-owned: every capability is daemon IPC plus CLI
  JSON, then TUI, web and MCP on the same surface.
- Every mutation, batch and reclassification has a preview whose
  selection is exactly what apply changes.
- Providers are reached only through `MailSyncProvider` and
  `MailSendProvider`. mxr writes to the server only the D108 archive on
  last done and, with the person's consent, tags in its own `mxr/`
  namespace; it never changes a user's own labels or folders.
- AI is off by default. Background model work runs on loopback unless
  the user sets `allow_cloud_background_classification` with their own
  API key. Mail is wrapped as untrusted data and is never instructions.
- No confidence thresholds, no scores on screen. Model text is marked and
  names its model.
- Measurements on real mail are counts only.

## What stays, what merges, what is new

| Today | In this design |
|---|---|
| `mail_kind::classify` heuristics | the Who and What-for judges, logic kept, reading facts |
| `MailEvidence`, sender history, three signal builders, duplicated evidence SQL | one facts record per email |
| todo detect, records detect, deliveries, updates facts, Reading extract | judges and item builders reading facts; their own parsers removed |
| `conversation_shape` | input to the Who claim |
| eight placement paths | one stored assessment, read by every projection |
| `RULES_VERSION` ×3, `EXTRACTOR_VERSION` | one version per judge, combined into the policy version |
| arrivals ledger, `mode_corrections`, screener decisions, `rule_treatments` | kept; every correction arrives as a teaching event and also writes a label; screener decisions gain domain scope |
| archive on last done, action rules' provider actions | one provider-effects queue, shared with the mirror |
| provider labels in rule conditions (`HasLabel`) | facts, with the `mxr/` namespace excluded |
| `SetTreatment` sorting rules, category rules | teaching and action rules in one engine with one condition language and one planner |
| Now's four fixed sections and caps (`now.rs`) | can't wait plus next best, ranked across modes, optional time budget |
| desk pace, act-by, update windows, Reading affinity and fade | the attention evidence on items |
| `triage_cache` and `mxr triage` | superseded by the What-it-asks judge |
| dead `KindRuleData::WrittenTo`, `todo::complete::looks_done` | removed |
| — | labels, the scenario corpus, `classify eval`, `now eval`, `classify audit`, `classify report`, rule suggestions, the effects table, attention feedback |

## Build order: measure first, then ship one usable rung at a time

Every rung is a release a person can install and use, with its own docs,
CLI JSON and tests; components such as facts, the resolver or the effects
queue are built inside the first rung that needs them, never as a stage on
their own. The ladder, with status as of 21 October 2026:

| Rung | After it ships, you can… | Builds | Status |
|---|---|---|---|
| R0 | trust that a promise's deadline came from the email's own words | the shared due-words check | shipped, v0.6.120 |
| R0b | re-check dates the old extractors saved, with a preview and undo | `mxr commitments recheck-dates` | in progress |
| R1 | see how well mxr sorts, per question, with every mismatch | scenario corpus (quick set), `mxr classify eval` | shipped, v0.6.122 |
| R1b | have every one of the 50,000 demo emails graded in CI | scenario-first demo generator, held-out phrasings, full set | next |
| R2 | see one home for every email everywhere, with the deciding reason | facts, judges for who and what-for, the resolver, the stored assessment, every projection reading it, fallback, `classify report` | planned |
| R2b | stop organisations with person-like addresses landing in Messages | the redesign loop on the who judge | planned |
| R3 | correct once and have it counted | teaching events and service (thread scope), labels, the four uncertainty kinds, `classify eval --set mine`, the effects table in code | planned |
| R3a | see your sorting in Gmail and teach mxr by changing it there | provider-effects queue, the label capability, the mirror, consent | planned |
| R3b–d | sync chosen categories; get the same on Outlook, then IMAP | the same capability per provider | planned |
| R4 | write rules in the classifier's own terms, by domain, from corrections | fact conditions, domain scope on `K`, the teaching list, rule suggestions | planned |
| R5 | have rules act after sorting and use it | action rules after the assessment, on the effects queue; one planner | planned |
| R6 | open Now and get the next best thing across modes | attention evidence on items, Can't wait, Next best, neglect, `now eval` | planned |
| R7 | give Now a time budget and tell it what isn't worth your time | sessions, earned Reading offers, attention feedback, miss detection | planned |
| R8 | ask mxr to red-team its own sorting | `classify audit`, the redesign skill | only when R3's labels show confident misses worth hunting |
| R9 | have a model answer what rules can't | fast-tier judge, model-versus-rule disagreement | only when fallback and unknown rates justify it |

R1b comes before R2 because R2 rewrites placement and 147 labelled emails
is too thin a regression gate for that. After R2, R2b, R3 and R6 can run in
parallel: they touch the judges, teaching and Now respectively, and share
only the assessment R2 fixes.

### Baseline: today's classifier on the quick set

Step 1's first rung is built: `mxr classify eval --set quick` grades the
stored mail against labelled scenarios, and a daemon test snapshots the
result so every later change shows up as a reviewed diff. The labels are in
`crates/provider-fake/src/fixtures/scenarios/` (147 emails: 99 from the
curated demo fixtures and 48 from 24 near-miss twin pairs). The twins are
not in the 50,000-email demo, so on a demo `mxr classify eval` grades the 99
curated emails and the daemon test grades all 147. Labels state what mxr
should do. The labels that rest on a judgement call say so in `review` on
their scenario, and the output marks a mismatch on one.

Measured by `scripts/cargo-test -p mxr --lib -- handler::tests::classify_eval`
(the snapshot in `crates/daemon/src/handler/tests/snapshots/` is the record):

| Question | Right | Of | Accuracy |
|---|---|---|---|
| Who sent it | 131 | 147 | 89.1% |
| Home | 132 | 147 | 89.8% |
| To-do aspect | 144 | 147 | 98.0% |
| Record aspect | 142 | 147 | 96.6% |

39 answers are wrong, on 20 emails. The clusters:

- Senders with no automation cue in the address read as people and land in
  Messages: the council (a bill and three statements), DocuSign (three
  emails), Disney+ (a cancellation and three receipts) and a museum's
  ticket mail. Twelve emails, wrong on both who and home.
- An opt-out header on mail that is not a newsletter reads as a list: a
  shop's receipt and an application acknowledgement go to Reading.
- Content cues move mail between person and machine: a receipt that asks
  you to confirm an address reads as a person, a colleague's FYI to a group
  reads as a machine.
- To-do and record gaps: a card about to expire makes no to-do, a failed
  gym payment is filed as a record, and the labelled judgement calls
  (statements as records, a failed payout as a to-do) disagree with the
  rules.

The numbers are for the labelled set, which was written to find splits,
not sampled from a mailbox, so they are not a rate of error in real mail.

## Decisions this rests on

From BK, 21 October 2026:

- There is no labelled set; the demo dataset is rebuilt from labelled
  scenarios to serve as one.
- Each email is judged on its own; threads stay whole on screen.
- Every incoming email has a home; fallback placements are tracked as a
  signal for missing distinctions or a missing mode.
- "Why here" shows the answer that decided.
- `K` covers the exact address by default; "everyone at the domain" is a
  second, deliberate choice.
- Renewal to-dos stay automatic.
- Fallback goes to Messages when the sender may be a person, Updates when
  it is a list or machine.
- Agents run a quick fixed set; CI runs all 50,000.
- Now's next best shows one item at a time.
- When nothing needs you, Now offers one readable only when it's earned.
- When what can't wait exceeds your time, Now shows all of it, ordered,
  and says how much time it needs.
- Sorting is written back to the server as `mxr/` mailbox labels, two-way:
  a change made on the server is a correction mxr learns from.
- Labels go on new mail and the last 90 days; older mail when it enters a
  mode again.
- Kind labels are permanent; a separate `mxr/Open` label tracks what's
  live.
- Each personal category is synced or kept local by its own setting.
- All providers: Gmail labels, Outlook categories, IMAP keywords (in that
  order).
- Writing labels needs the person's consent, asked once at setup and once
  after the upgrade that adds it.

From the 19 October conversation (`chatgpt.com/c/6ad616d9-04b0-83ed-8e62-fff9cee96d28`):

- Modes describe the kind of interaction, not importance; Now decides
  importance across modes.
- If you open Now, you get what you need to do right now, whether because
  of importance, urgency, consequence, value or neglect.
- Now behaves like a proactive assistant over the modes, and borrows
  Marginalia's one-at-a-time, time-aware recommendations.
- The classifier holds back rather than guessing, keeps a parking lot for
  human review, learns from that review, is red-teamed on real mail, and
  is improved by redesign checked against a labelled corpus, not by
  hotfixes.

Carried from earlier decisions: D108 (archive on last done), D111 and
D114 (layers and tiers, reshaped here), D115 (windows), D116 (first run),
D119 and its amendment (trust, Not sure, purpose separate from
obligation), D121 (Reading in Marginalia), and BK's standing rule that
detected to-dos are created automatically while auto-closing stays a
suggestion.

## Unresolved questions

None at the design level. Step 1 will raise the next ones: how well the
scenario catalogue covers real mail, and the measured baseline of today's
classifier and Now.
