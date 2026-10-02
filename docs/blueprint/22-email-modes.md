# 22. Email is five apps: Messages, To do, Updates, Reading, Archive

Email is five apps sharing one inbox, and you can tell because each has its
own action. mxr should give each one its own mode, place every message in
the modes it belongs to, say why, and let the user correct it. This
document is the product model, how it maps onto what exists at `3da0c119`
(v0.6.47), the data model, the classifier and its accuracy check, the
deadline rules, and a phased plan. Settled choices are D107 to D111 in
[15-decision-log.md](15-decision-log.md). The rubric that grades the work
is v3 in `docs/web-app-experience-rubric.md`.

## The five modes each have their own verb

| Mode | What lands there | The verb | Rhythm |
|---|---|---|---|
| Messages | Conversations with the few people you care about, in the mould of iMessage or WhatsApp | Reply, or start a conversation | As they arrive, ranked by person |
| To do | Admin that needs you: pay, renew, verify, sign, book | Do it, schedule it, tick it off | By deadline, surfaced ahead of it |
| Updates | Notifications: a sign-up happened, your week on some app. Not a conversation, not a to-do | Glance and let go | Batched, once a day |
| Reading | Things you asked to receive: newsletters, digests, posts | Read now or later (the email or the link it points to), or unsubscribe | When you choose to read |
| Archive | Receipts, confirmations, records | File it, then find it by search and use it | On demand |

Inbox stays as the everything view, in arrival order. It is a lens, not a
mode, and holds nothing the modes don't.

The desk becomes the cross-mode "now" view: people you owe a reply, to-dos
due soon, and one Updates digest a day. It is home, but it isn't a sixth
place. Everything on it lives in a mode, and acting on it acts in that mode.

## One email can live in several modes

Each mode looks at a different aspect of a message:

| Aspect | Mode | Example from one email |
|---|---|---|
| The conversation | Messages | Your accountant writes "Can you send the signed form by Friday?" |
| The task and its deadline | To do | "Sign and return the form, due Friday" |
| The fact | Updates | "Your tax return was filed" |
| The article | Reading | The link to the guide they attached |
| The record | Archive | The filed return, findable next April |

The accountant's email is in Messages (reply to her) and To do (sign the
form by Friday) at once. This is a property to design for, not a conflict
to resolve. A mode shows the aspect it cares about: To do shows "Sign the
form, due Fri" with the sender as context, while Messages shows the
conversation with the ask highlighted.

## Handoff passes the item to the next mode without clearing the others

Handoff works like passing a file to another department. When one mode is
done with an item, the user can pass it on, and the next mode picks up its
own aspect:

- After replying in Messages, "move to To do" (`t`) creates a to-do from
  the conversation, prefilled from the ask and any due words, and marks the
  conversation done in Messages.
- From Updates, "this needs me" creates a to-do from a notification ("Your
  card expires next month").
- Ticking off a to-do offers to file the source email in Archive.
- From Reading, "later" keeps the article in Reading's later list.

Done in one mode does not clear the email from another. Each mode keeps its
own done state. The provider archive (removing INBOX) happens only when the
last mode that holds the message lets it go, so archiving the reply in
Messages never loses the open to-do, and Inbox (everything) still shows
mail that some mode is holding.

## Classification is rules first, then the user's model, and every item says why

Placement runs in three layers, cheapest and most explainable first. The
user's correction beats all of them.

1. **Sender rules (exist today).** `mail_kind::classify` in
   `crates/daemon/src/handler/mail_kind.rs` places a sender as Person,
   List or Automated from delivery and invite signals, address hints
   (`notifications@`, `newsletter@`, `alerts.` subdomains, no-reply local
   parts), `List-Id`, `List-Unsubscribe` and the contacts table's
   `is_list_sender`, and returns the rule as a human reason. A screener
   disposition for the sender (`crates/store/src/screener.rs`) wins over
   every rule. The base mode follows:

   | Sender kind | Screener disposition | Base mode |
   |---|---|---|
   | Person | Allow | Messages |
   | List | Feed | Reading |
   | Automated | PaperTrail | Updates, and Archive when the message is a record |
   | Denied | Deny | No mode. Still in Inbox and search |

2. **Message rules (new).** Deterministic detectors per aspect, in the
   pattern of `mxr_deliveries::detect` (`crates/deliveries/src/lib.rs`):
   local heuristics and schema.org data decide clear cases and shortlist
   unclear ones. A to-do detector looks for admin verbs with an object
   (pay, renew, verify, sign, book, confirm, RSVP, "action required"),
   due phrases ("due", "expires", "by", "before", "until") and schema.org
   `Invoice.paymentDueDate`, `Reservation` and `Event` dates. A record
   detector looks for receipts, invoices, order and booking confirmations,
   and rows already in `deliveries`. A calendar invite with no reply is a
   to-do ("RSVP"). These add aspects; they never remove the base mode.

3. **The user's model (new).** Shortlisted messages go to the configured
   model for what rules can't tell: is there a task, what is it, what are
   the due words, is this a notification. The model copies the due words,
   the daemon checks they appear in the text and resolves them with
   `mxr_core::natural_time` in the user's zone, so the date is the
   parser's, not the model's. `crates/daemon/src/handler/promises.rs`
   already works this way for promises in sent mail. Answers are cached per
   message, prompt version and content hash, as `triage_cache` does
   (`crates/store/migrations/044_triage_cache.sql`). With no model, layers
   1 and 2 still place everything.

Every item in every mode carries a reason: "Here because: you marked this
sender as a person", "Here because: asks you to renew, due 14 Oct (local
model)". `mxr why <message>` lists every mode a message is in, with the
reason and the source (your decision, a rule, or a named model).

### Corrections work per email and per sender

- **Per email:** "not a to-do", "this is a to-do", "move to Updates". Stored
  per message, so a re-run of the classifier can't undo it.
- **Per sender:** the existing screener dispositions set the base mode.
  A new per-sender aspect override covers "never make to-dos from this
  sender" and "always file this sender in Archive".

The correction is one key, its effect shows at once, and future mail from
the sender follows the per-sender rule.

### Email content goes only to the user's configured model

The privacy rules already in the daemon carry over unchanged:

- AI is off by default, and the default endpoint is a local Ollama
  (`LlmConfig::default` in `crates/config/src/types.rs`: `enabled: false`,
  `base_url: "http://localhost:11434/v1"`).
- Each request pins one provider, and what the prompt may carry, the call
  and the disclosure all follow that one endpoint (`GistPolicy::pin` in
  `crates/daemon/src/handler/thread_gist.rs`).
- Locality comes from the pinned endpoint (`llm_endpoint_is_local` in
  `crates/daemon/src/state.rs`, backed by `mxr_llm::is_loopback_endpoint`
  in `crates/llm/src/endpoint.rs`).
- Mail beyond the message at hand goes to a cloud endpoint only with
  `llm.allow_cloud_relationship_data` (`relationship_data_allowed`).
- Mail is wrapped as untrusted data in the prompt (`wrap_untrusted_mail`,
  `guarded_system_prompt`).

Mode classification adds one stricter rule, because unlike a gist (people
only, on view) it would read every incoming message in the background:
background aspect detection runs only against a loopback endpoint unless
the user sets an explicit, feature-specific opt-in
(`llm.overrides.mode_aspects` pointing at a cloud endpoint plus a
`allow_cloud_background_classification = true` flag). A cloud model the
user configured for drafting never silently starts classifying all mail.

## How the modes map onto what exists today

All of these are at `3da0c119`.

| Exists today | Where | Becomes |
|---|---|---|
| Desk lanes You owe, Due, Waiting on, New from people | `crates/daemon/src/handler/desk_lanes.rs`, `desk.rs` | The cross-mode now view: You owe (from Messages), Due soon (to-dos and promises), one Updates digest card. New from people moves into Messages |
| Reply queue (`reply-queue` route, reply-later flag in `message_flags`) | `apps/web/src/routes/reply-queue.tsx`, `crates/store/src/message_flags.rs` | A filter in Messages ("Reply later"). The flag and its timed return (`056_reply_later_due.sql`) stay as they are |
| Owed replies page (`ListOwedReplies`, `mxr owed`) | `apps/web/src/routes/owed.tsx`, `crates/store/src/owed_replies.rs` | Messages' "You owe" filter. One owed rule for desk and Messages, so the counts match |
| Waiting on (desk lane, `g w`) | `desk_lanes.rs` | A Messages filter, and still a desk lane when a wait passes its time |
| Snoozed | `apps/web/src/routes/snoozed.tsx`, `crates/store/src/snooze.rs` | A state, not a place: each mode shows its own snoozed count and list. The cross-mode Snoozed list moves under More |
| Paper trail | `crates/daemon/src/handler/places.rs`, `MailPlaceData::PaperTrail` | Split: recent notifications go to Updates, records go to Archive. `/paper-trail` redirects to Updates |
| Reading | `places.rs`, `MailPlaceData::Reading` | Reading mode, plus a later list and the existing unsubscribe |
| Screener | `crates/store/src/screener.rs`, `handler/screener.rs` | A question about which mode a new sender belongs to, asked only when rules can't tell. Off the rail; the desk mentions it when someone waits |
| Promises (Due lane, `contact_commitments`) | `crates/store/migrations/024_contact_commitments.sql`, `handler/promises.rs` | Your promises are to-dos too. To do lists them alongside detected admin, from their own table |
| Deliveries | `crates/deliveries`, `042_deliveries.sql` | Updates while in transit, Archive once delivered |
| Invites | `apps/web/src/routes/invites.tsx` | To do while unanswered ("RSVP by"), Archive after |
| Subscriptions | `apps/web/src/routes/subscriptions.tsx` | Reading's manage view |
| `mxr triage` (Action, FYI, Routine) | `crates/daemon/src/handler/triage.rs` | Superseded by mode aspects once phase 5 ships; kept until then |

The judge's tour on the demo data shows why the current rail isn't enough.
The web rail is Desk, Inbox, Reply queue, Waiting on, Snoozed, Reading,
Paper trail and Screener, then More, Labels and Tools
(`apps/web/src/components/Sidebar.tsx`), which is a folder list organised
by mail mechanics. "Action required: unusual sign-in attempt" sits in You
owe and "Build failed on release branch" in New from people, because a
person-looking sender makes every message a conversation. Desk lanes sort
by age and pace (`sort_lane` in `desk_lanes.rs`), not by who the person is.
The screener queue lists every inbound sender without a decision
(`list_screener_queue`), so 27 senders wait in a demo mailbox. A fix for
that last one is in flight on another branch: anyone you've written to is
never screened.

## What changes per client

The CLI-first contract holds (`AGENTS.md`): every mode is daemon IPC plus
CLI JSON first, then TUI, web and MCP. Nothing here is web-only.

**Daemon and protocol.** New requests: `ListModeItems { mode, filter,
account, cursor }`, `GetModeMembership { message_id }` (backs `mxr why`),
`SetModeCorrection { scope: Message | Sender, ... }`, `ListTodos`,
`CreateTodo { from_message, title, due_at }`, `UpdateTodo { state,
scheduled_for, due_at }`, `SetModeDone { mode, thread_ids, dry_run }`,
and `GetUpdatesDigest { day }`. New events: `TodoSurfaced` (lead time
reached) and `ModeAspectsReady`. Batch moves and batch done return a
preview token on `dry_run`, and the commit acts only on what the preview
listed (D098).

**CLI.** `mxr modes` (counts per mode), `mxr messages`, `mxr todo` (`list`,
`add --from MESSAGE --due PHRASE`, `done`, `schedule`, `undo`), `mxr
updates` (today's digest, `let-go --dry-run`), `mxr reading` (exists),
`mxr records` (Archive mode; `mxr archive` is already the archive verb),
`mxr mode set MESSAGE --mode updates [--sender]` for corrections, and
`mxr why` extended to list every mode. All take `--format json`.

**TUI.** The sidebar (`crates/tui/src/ui/sidebar.rs`) gets the five modes
and Inbox. To do gets its own lens beside `desk_lens.rs` and
`place_lens.rs`. The same keys as the web, enforced by
`keymapParity.test.ts`.

**Web.** The rail becomes Now (the desk, home), Messages, To do, Updates,
Reading, Archive, then Inbox. Reply queue, Waiting on and Owed become
filters in Messages; Snoozed, Screener, Invites, Deliveries and
Subscriptions move under More or into their mode. Badges still count only
work: Now and To do (due soon).

**MCP.** Tools for `mxr_list_mode`, `mxr_todos` and `mxr_mode_membership`
beside the existing `mxr_list_place` (`crates/mcp/src/lib.rs`), with
mutations through the existing preview path.

## Data model: to-dos get their own table; membership is computed

To-dos are stored in their own table, not derived from mail on every read.
A to-do has a life of its own: it outlives the email being archived, it can
be created by hand from a conversation, it gets scheduled and ticked off,
and its due date may be corrected by the user. None of that is a property
of the message. `deliveries` (`042_deliveries.sql`) and
`contact_commitments` already set the pattern: a detected row, provenance
back to the message, and non-destructive resolve and dismiss.

```sql
CREATE TABLE todos (
    id                TEXT PRIMARY KEY,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id         TEXT NOT NULL,
    source_message_id TEXT NOT NULL,
    title             TEXT NOT NULL,          -- "Renew passport"
    verb              TEXT,                   -- pay | renew | verify | sign | book | rsvp | other
    due_at            INTEGER,                -- UTC instant; NULL when there is no deadline
    due_words         TEXT,                   -- the phrase found in the message, verbatim
    surface_at        INTEGER,                -- due_at minus lead time, at the user's morning hour
    scheduled_for     INTEGER,                -- "do it on Tuesday"
    state             TEXT NOT NULL CHECK (state IN ('open', 'done', 'dismissed')),
    origin            TEXT NOT NULL CHECK (origin IN ('rule', 'model', 'handoff', 'manual')),
    reason            TEXT NOT NULL,          -- shown as "Here because: ..."
    model             TEXT,                   -- set when origin = 'model'
    surfaced_at       INTEGER,                -- claim guard: announced once
    dedup_key         TEXT NOT NULL,          -- account + thread + verb + normalised title
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    done_at           INTEGER,
    UNIQUE (account_id, dedup_key)
);
CREATE INDEX idx_todos_surface ON todos (surface_at) WHERE state = 'open' AND surfaced_at IS NULL;
```

Upserts use `ON CONFLICT(account_id, dedup_key)`, never `INSERT OR
REPLACE`. A later email in the same thread ("reminder: your renewal is due
in 3 days") updates the row instead of adding one. The surfacing claim
follows `reply_later_returned_at` (`056_reply_later_due.sql`): the wake
loop sets `surfaced_at` in the same UPDATE that finds the row, so a to-do
is announced exactly once across restarts.

Mode membership is computed, not stored. Places already work this way:
Reading and Paper trail are views over the inbox, classified at read time
(D097). Membership is a pure function of the message, its sender
classification, its stored aspects and the user's corrections:

```text
modes(message) = base_mode(sender)                       -- mail_kind + screener
               + aspects(message)                        -- message rules + cached model answers
               + open todo rows for the thread
               - corrections(message)
               - modes the user marked done (through the thread's watermark)
```

Three small stores feed it:

- `mode_aspects (message_id, prompt_version, content_hash, task_json,
  is_notice, is_record, model, generated_at)`: the model cache.
- `mode_corrections (account_id, scope, key, mode, verdict, decided_at)`:
  per message and per sender, where `scope` is `message` or `sender`.
- `mode_done (account_id, thread_id, mode, through_seq, through_count,
  done_at)`: per-mode done, using the same watermark as `desk_dismissals`
  (`051_desk_dismissals.sql`, `DeskDismissal::covers`), so a new message in
  the thread brings it back to that mode only.

Activity records for these writes go through `state.activity.record(...)`
with ids and counts only, never titles or due words, and `MXR_ACTIVITY=off`
disables them.

## Messages ranks by person

Messages answers "who matters" before "what's newest". Each person gets a
relationship strength from local history, already in the `contacts` table
(`010_contacts.sql`): `total_outbound`, `replied_count`, `last_outbound_at`,
`cadence_days_p50`, plus reply pairs. Writing to someone is the strongest
signal; their writing to you, alone, is weak. The score is a small,
explainable formula, not a model, and the row can say why it ranks there:
"you've written to Maya 48 times, last on Tuesday".

Rows are per person, like a chat list: one row per counterparty with their
latest conversation, and a group conversation is its own row. Ranking is
by strength band first (close, regular, occasional), then by whether you
owe a reply, then by recency. Someone new who writes once lands at the
bottom of "occasional" until you reply.

## Deadlines surface ahead of time, not on the day

A to-do with a deadline appears in To do as soon as it's detected, and on
the desk at its surface time:

| Verb | Lead time | Why |
|---|---|---|
| pay | 3 days | Bank transfers take a working day or two |
| renew | 7 days, 30 for documents (passport, visa, licence) | Renewals have their own processing time |
| sign | 2 days | Usually needs a quiet moment |
| book | 7 days | Slots run out |
| verify, confirm | at once | Links expire quickly |
| rsvp | 2 days | The organiser plans around replies |
| other | 2 days | A default the user can change |

Rules for the surface time:

- `surface_at = due_at - lead`, moved to the start of the user's working
  hours that day (the preferred hours `natural_time` already uses), and
  never later than now when the deadline is already inside the lead time.
- A due date in the past at detection time surfaces at once, labelled
  "was due Tue", with no red and no scolding (D101).
- A to-do without a deadline surfaces at once and stays in To do until
  done. It never shows on the desk unless scheduled.
- A scheduled to-do ("do it on Saturday") surfaces at its scheduled time
  instead of its lead time, but its due date still shows.
- Only the due words found verbatim in the message count. When the parser
  offers several readings, the earliest plausible one is used and the row
  says "due Fri 3 (assumed 15:00)". The user can correct it with the
  natural-time field (D096).
- The lead times are config with these defaults, per verb.

## Phases

Each phase ships on its own, behind no flag, with a user-visible check and
named tests. The first proves the model with the mode that has no home
today.

### Phase 1: To do, with rule-based deadline detection

The `todos` table, the rule-based to-do and deadline detector (run in
`post_sync_fanout` in `crates/daemon/src/loops.rs`, like deliveries), lead
time surfacing in the timer loop (`process_due_timers`), `mxr todo`, the
desk's Due lane showing to-dos beside promises, a web To do page and rail
entry, and a TUI lens. Handoff from a conversation (`t`, `mxr todo add
--from`) ships here too, because a hand-made to-do needs no classifier.

- **Check:** on BK's real mailbox, `mxr todo --format json` lists his real
  bills and renewals with due dates (counts reviewed by BK, no content
  recorded). In the demo, a bill due in five days appears on the desk
  three days before, exactly once, after a daemon restart.
- **Tests:** `crates/store` `todos` tests (upsert by dedup key, claim
  guard); `todo_detect` unit tests over fixtures per verb and due phrase;
  `handler/tests/todo.rs` with a moved clock (surfaces once at lead time,
  not before, survives restart, done cancels); CLI JSON snapshot;
  `e2e/todo.spec.ts` (detect, schedule, tick off, undo, handoff from a
  conversation); TUI lens test.

### Phase 2: The rail by mode, membership, reasons and per-mode done

`modes()` as above with rules only, `mode_corrections` and `mode_done`,
`ListModeItems`, `GetModeMembership`, `SetModeDone` with a dry run, `mxr
modes`, `mxr why` listing every mode, and the web and TUI rail of Now,
five modes and Inbox. Paper trail splits into Updates and Archive; Reply
queue, Waiting on and Owed become Messages filters. The provider archive
waits for the last mode to let go.

- **Check:** on the demo, "Action required: unusual sign-in attempt" is in
  To do or Updates, not Messages, and says why. An email with a reply owed
  and a to-do is in both; done in Messages leaves the to-do open.
- **Tests:** `mode_membership` unit tests (every base mode, multi-mode,
  corrections win, done watermark); `handler/tests/modes.rs` (dry-run
  token, preview equals commit, provider archive only when the last mode
  lets go); `e2e/modes.spec.ts` (rail, reasons, correction per email and
  per sender, handoff); `keymapParity.test.ts`.

### Phase 3: Messages ranks by person

The relationship strength score, per-person rows with group conversations
as their own rows, owed and waiting as filters, and the ranking reason on
hover and in JSON.

- **Check:** BK's Messages top ten are people he would name, checked by
  him against his real mail (counts of agree and disagree recorded).
- **Tests:** `relationship_strength` unit tests (writing to someone
  outranks being written to; bands; recency); `e2e/messages.spec.ts`;
  timing of the ranking SQL read-only on the real store (D5).

### Phase 4: Updates as a daily digest, and the desk as the now view

`GetUpdatesDigest`, `mxr updates`, the Updates mode grouped by sender and
day, "let go" (with a dry run for a whole day), "this needs me" handoff to
To do, deliveries shown in Updates, and the desk rebuilt as You owe, Due
soon and one digest card a day at the user's chosen time.

- **Check:** a day of demo notifications arrives as one digest card, not
  as rows, and letting go of the day archives what no other mode holds.
- **Tests:** `handler/tests/updates.rs` (digest window, one card a day,
  let go respects other modes); `e2e/updates.spec.ts`; `e2e/desk.spec.ts`
  updated for the now view.

### Phase 5: The model for to-dos, deadlines and notifications, measured

The `ModeAspects` LLM feature with its override, the loopback-only
background rule, `mode_aspects` cache, prefilter shortlist, quote and due
word checks, provenance on every model-placed item, and `mxr modes eval`:
a local labelling and scoring harness over a read-only sample of the
user's mail that reports counts only.

- **Check:** `mxr modes eval --sample 200` on BK's mail with his local
  model reports per-mode precision and recall and deadline accuracy, and
  the numbers are recorded in the rubric (counts only, no content). A
  cloud endpoint configured only for drafts classifies nothing.
- **Tests:** `mode_aspects` prompt tests (untrusted wrapping, quote check,
  due words resolved by the parser not the model); privacy tests in the
  style of `draft_compose` ("a cloud model without the background opt-in
  never sees mail", "the disclosure follows the provider that answered");
  cache hit and invalidation by content hash.

### Phase 6: Reading later and Archive records

Reading's later list (the email, or the linked article fetched and
cleaned through `mxr_reader` on request), and Archive as search with a
records facet (receipts, orders, bookings, filed to-dos) and `mxr records`.

- **Check:** "later" on a newsletter link shows the article in Reading's
  later list in reader mode; `mxr records --query "passport"` finds the
  renewal receipt from phase 1's to-do.
- **Tests:** `e2e/reading-later.spec.ts`, `e2e/archive.spec.ts`, record
  detector unit tests.

## Unresolved questions

- Messages rows per person or per conversation? This plan says per person
  with group conversations as their own rows. Confirm.
- Should the provider archive wait for the last mode to let go, or should
  archive stay an explicit verb with modes only hiding mail?
- What time should the daily Updates digest arrive, and should it be one
  card or a short list on the desk?
- Are the default lead times right for you, especially 30 days for
  document renewals?
- Should promises you made and admin to-dos share one To do list, or sit
  in two sections of it?
- Does the Screener survive as its own page once rules and "written to"
  place most senders, or does it fold into a per-sender question in each
  mode?
- Is a feature-specific opt-in enough for cloud background classification,
  or should it be impossible?
