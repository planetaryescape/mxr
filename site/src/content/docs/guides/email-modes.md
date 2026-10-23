---
title: Email is five apps at once
description: Understand the five workflows, overlapping membership, sorting rules, and personal categories.
---

Your inbox mixes five kinds of mail that ask for five different things.
A person is waiting on a reply. A bill has a deadline. A build notification
wants a glance. A newsletter you chose is there to read when you like. A
receipt matters a year from now, when you need its reference number. One
list of subject lines treats all five the same and offers the same
actions for each: reply, archive, label.

mxr treats email as five modes that share one local mailbox and runtime.
Each has its own view, primary object and completion action. The web app,
TUI, CLI and agent interfaces use the same daemon-backed state, with different
client surfaces. Now brings together a short list of work across the modes;
it is a starting view, not a sixth mode.

```bash
mxr now            # people waiting on you and tasks due soon
mxr messages       # people and groups, with their topics
mxr todo           # tasks ordered by when to act
mxr updates        # notifications grouped into a briefing
mxr reading        # newsletters as an edition
mxr records        # receipts, orders, bookings and bills
mxr modes explain  # what each shipped mode does and its keys
```

## Each mode does one job, with its own verb

| Mode | What it holds | What you do there | Completion |
|---|---|---|---|
| [Messages](/guides/messages/) | People you are in conversation with | Read the request, reply, acknowledge or mark its topic done | Done clears that topic in Messages; a later message can bring it back |
| [To do](/guides/todo/) | Dated actions with a source email | Check the evidence, act, schedule or complete one task | Completing a row ticks that task; other tasks remain open |
| [Updates](/guides/updates/) | Source-grouped notifications | Glance, open the source email, make a task when needed, then let go | A preview selects the digest messages to mark done in Updates |
| [Reading](/guides/reading/) | Newsletters and posts you chose | Read, save to Later, unsubscribe or let go | Let go or expiry clears an item from Reading; Later stays saved |
| [Archive](/guides/archive/) | Sourced records: receipts, orders, bookings, invoices | Retrieve a field and follow it to its source email or document | Records stay filed; Archive has no done action |

**Inbox** stays as the everything view, in arrival order. It is not a sixth
mode. The mode answers what an email is for; Inbox shows what arrived.

Each view keeps its own object and action. Use Now to choose a person in Messages, a
task in To do, the Updates briefing, or a reading item.
Use Archive when you need a filed record and its source.

## A conversation can belong to several modes

A conversation can contain a reply to write, an open task and a filed record.
Each mode tracks its own work. Finishing in Messages leaves an open to-do
alone. Archive keeps filed records without holding their email in the inbox.

Inspect membership and preview what finishing in one mode would do:

```bash
mxr modes why MESSAGE_ID --format json
mxr modes done THREAD_ID --mode messages --dry-run
```

When the last inbox-holding mode lets go, mxr may archive the conversation at
the provider. This is enabled by default through
`modes.archive_on_last_done`; the preview shows the provider effect before
you apply `mxr modes done THREAD_ID --mode messages`. Archive records do not
hold an email in the inbox and filing a record does not perform this action.

## Sorting rules and categories serve different jobs

Each inbound email has one stored home and deciding reason. Messages,
Updates, Reading, the Inbox chip and arrivals read that answer. Your moves
and sender choices take precedence over teaching rules and built-in rules;
Spam, Trash and blocked senders stay outside those homes.

When no rule decides, possible personal mail goes to Messages and machine
mail goes to Updates, marked **Not sure**. A missing body remains pending.
Open `mxr why MESSAGE_ID --format json` for the stored assessment and evidence,
or `mxr classify report --format json` for bounded fallback counts and clusters.
The stored-assessment details require changes made after
`v0.6.158`; that release does not include them.

While sync changes the evidence, an email can briefly be still sorting. It
stays in Inbox and arrivals while its mode waits for a stored answer. If
`mxr why` returns `classification_pending`, retry once sorting finishes.

A home describes the email's purpose. Inbox membership, snoozes, done marks and
expiry still decide whether it appears as open work. Inspect a message's
placement before correcting it:

```bash
mxr why MESSAGE_ID --format json
mxr reclassify MESSAGE_ID reading --dry-run
mxr reclassify MESSAGE_ID reading
mxr arrivals list --mode reading --format json
mxr corrections --format json
```

Use `--sender` with Messages, Updates or Reading when the choice should apply
to that sender's mail. [Rules](/guides/rules/) can automate placement with a
preview of their effect.

[Personal categories](/guides/personal-categories/) add your own groupings,
such as Travel or Household, across modes and accounts. They keep provider
labels and folders unchanged. Category rules add or remove membership;
manual inclusions and exclusions win over those rules.

```bash
mxr categories --format json
mxr categories --format json why MESSAGE_ID CATEGORY_ID
```

## Measure how well mxr sorts

`mxr classify eval` grades the mail already in your mailbox against labelled
scenarios and prints how often today's rules agree with the labels. It reads
stored mail and changes nothing. Only the demo mailbox carries labels. `mxr demo`
runs in its own profile and leaves your config alone, and every `mxr` command
uses it until `mxr demo stop`:

```bash
mxr demo --no-tui
mxr classify eval                  # the quick set, in seconds
mxr classify eval --set full       # every labelled email, up to a minute
mxr demo stop                      # back to your real profile
```

On a profile without labelled mail it prints `No labelled mail here; run mxr
demo`. In a terminal the output is a table; piped, it is JSON. `--format table`
forces the table. This is what the quick set printed on v0.6.150:

```text
Classifier eval, quick set (label schema v1)

question evaluated  correct  accuracy
who             99       89     89.9%
home            99       89     89.9%
todo            99       98     99.0%
record          99       95     96.0%

99 of 147 labelled emails are in this mailbox; 49901 stored emails have no label.

25 mismatches
subscriptions-disney-receipt  demo-msg-103  who  expected machine  got person  (possible personal mail; no decisive list or automation evidence)
```

Each email gets four questions. **who** asks whether a person, a list, a
machine or a screened-out sender wrote it. **home** asks which mode it lands in.
**todo** and **record** ask whether a to-do or an Archive record was made from
it. A mismatch names the scenario, the demo message id, the question, the
expected and actual answers, and the rule that decided. `[label is a judgement
call]` marks a label that may be the label's fault rather than the classifier's.
Labels say what mxr should do, so a gap shows up as a mismatch.

| Set | Grades | Use it |
|---|---|---|
| `--set quick` (default) | The 147 labelled emails of the hand-written demo mail and its near-miss twins; 99 of them are in the demo mailbox | After you change how mail is sorted |
| `--set full` | Every labelled email in the mailbox: on the 50,000-message demo, 38,747 emails across both accounts | Before you rely on a change; CI runs it |

The full set groups its mismatches into clusters, biggest first, with one
example each, and prints a second accuracy table for held-out phrasings. Those
are wordings kept out of the mail used while tuning the classifier. If a held-out
figure falls well behind the overall one, a change fitted the generator's
wording rather than the mail. `--format json` lists every mismatch,
`--format csv` writes one row per mismatch, and `--account` limits the run to
one account.

## Model work will run in two tiers (planned)

Today every model feature uses the one model you set under `[llm]`, or a
per-feature override. Model features are off until you turn them on, and
the default endpoint is a model server on your own machine.
[LLM features](/guides/llm-features/) lists what runs today.

The modes plan splits model work by how hard it is:

- A **fast tier** does the bulk work that touches every incoming message:
  deciding which mode it belongs in. It runs on your local model by
  default.
- A **smart tier** pulls out the fields that need care: the amount and
  deadline of a bill, the fields of a receipt, what a person asked you. It
  only sees mail already placed in To do, Archive or Messages. It can use a
  cloud model you set up with your own API key, or stay local.
- When the smart model can't find an answer that code can check, the task
  can step up to a stronger model you name, instead of every call paying
  for the strongest one.

The current LLM configuration already supports a local endpoint or a cloud
provider with your own key. The proposed tier routing is separate from that
choice. Amounts and dates a model pulls out must
appear word for word in the email, and code checks that. Many tasks won't
use a model at all: where code can do the job, such as quote stripping or
ranking people, code does it.

Check whether model features are on, and which model and endpoint the daemon uses:

```bash
mxr llm status --format json
```

## Each mode will search the part of the email it cares about (planned)

Today semantic search uses header chunks and overlapping windows of the
cleaned body and attachment text, plus saved highlights. Archive also adds
a chunk of record fields, with PDF text when the attachment is cached.
Embedding runs on your machine.
[Semantic search](/guides/semantic-search/) covers how to run it.

The plan gives each mode its own recipe. Messages indexes each message's
new text without the quoted history. To do indexes one line per task.
Updates indexes one fact per message and folds repeats. Reading embeds
articles only when you read them. Archive indexes record fields and
leaves reference numbers to exact keyword search. Archive's recipe ships:
a record's emails get one field chunk beside the header, plus their PDF
text. The other recipes replace today's chunking only if a retrieval test
on real mail shows they find things at least as well.

See what today's index holds:

```bash
mxr semantic status
```

## What you can use today, mode by mode

| Mode | Status today | Shipped features that already serve it | Planned |
|---|---|---|---|
| Now | Built | [Now](/guides/now/) (`mxr now`, `g h`): People, Due soon, the Updates card and an evening Reading pick, at most ten things; first-run progress, arrivals (`mxr arrivals`), corrections (`mxr corrections`), the rail (`mxr modes rail`), done here per mode (`mxr modes done`) and "Also in" (`mxr modes why`) | Background classification by your model and a model-history horizon |
| Messages | Built (`g m`) | [Messages](/guides/messages/) (`mxr messages`): people as rows with their topics inside, Your turn by closeness and pace, Got it (`mxr messages ack`), thread-aware quote stripping, manual merges with a suggestion; [Focus & reply](/guides/focus-and-reply/) | Incoming Gmail reactions as a mark on the message; the Messages index recipe once the retrieval eval favours it |
| To do | Built (`g x`) | [To do](/guides/todo/) (`mxr todo`): bills, renewals, forms and promises with act-by dates; add and edit tasks, review the first-run catch-up (`mxr todo catchup`), [review later evidence](/guides/todo-resolution/), and optionally [link Microsoft tasks](/guides/todo/#link-a-task-with-microsoft-to-do); [calendar invites](/guides/calendar-invites/) (`mxr invites`) | An accuracy-gated rail badge; the planned `mxr modes eval` command is not available yet |
| Updates | Built (`g u`) | [Updates](/guides/updates/) (`mxr updates`): one line per source at the 08:00 and 16:30 cuts, code-computed changes, parcels, builds and incidents as trackers, let go in one key (`mxr updates let-go`); a new sign-in, failed payment or delivery problem leads Needs a look as a suggested to-do that `t` adds; [deliveries](/guides/deliveries/) (`mxr deliveries`) | One fact per message in the semantic index, and model-written facts for generic subjects (phase 7) |
| Reading | Built (`g r`) | [Reading](/guides/reading/) (`mxr reading`): an edition banded since your last visit, earlier and fading, a 66-character reader with the linked article on request, a Later shelf, let go with `e` and `E`, [unsubscribe](/guides/unsubscribe/) with evidence, highlights you can edit and remove, and [Marginalia](/guides/marginalia/) on `O` | Section-aware search chunks and lazy embedding for whole issues |
| Archive | Built (`g e`) | [Archive](/guides/archive/) (`mxr records`): receipts, orders, bookings, bills and documents as records, an answer box with [`mxr ask`](/guides/archive-intelligence/) fallback, CSV/PDF export, recurring subscriptions (`mxr records subscriptions`), renewal reviews, coming-up moments and record-field search chunks | Record fields read by the smart tier, and learned templates per issuer |

Classification by your own model is phase 7. Phase numbers come from the
plan and can change; this page changes when each phase ships.

## Deleting an email clears its derived data

When mxr observes a provider deletion, it removes the local message and
its per-message data, invalidates affected summaries and briefings, and
removes extracted facts that have no remaining evidence. Managed cache
files and in-memory index entries are cleaned through a durable queue.
File cleanup has bounded retries; an in-memory index failure waits for its
next rebuild or daemon restart. Failures are reported in daemon logs.
[Security & Privacy](/guides/security-and-privacy/#deleting-an-email-removes-most-of-what-mxr-derived-from-it)
explains the timing and the user-authored state and no-text receipts kept
deliberately. Moving a message to Trash is not a provider deletion.

## Find a mode's explanation again

```bash
mxr modes explain todo --format json
mxr modes hint todo.runway --show
```

`modes explain` lists the mode's purpose, actions, keys and hint IDs.
`modes hint ID --show` makes a dismissed hint eligible to appear again;
without `--show`, it dismisses that hint across clients. Use the mode's
`?` help in the app when you want the explanation beside your mail.

## Read the plan

- [22. Email is five apps](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/22-email-modes.md):
  the design, the data each mode needs, and the phases in order
- [Decision log, D107 to D114](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/15-decision-log.md):
  the settled choices behind the modes
- [Roadmap](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/14-roadmap.md):
  which phases are done

## See also

- [Work through your day](/guides/your-day/): today's workflow, start to
  finish
- [Clear the desk](/guides/now/)
- [Clear Reading and Paper trail](/guides/reading-and-paper-trail/)
- [Archive](/guides/archive/)
- [Glossary](/guides/glossary/#email-modes)
