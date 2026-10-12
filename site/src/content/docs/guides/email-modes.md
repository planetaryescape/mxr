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

mxr treats email as five modes that share one mailbox. Each has its own view
and actions. Now brings together the work that needs your attention.

```bash
mxr now            # people waiting on you and tasks due soon
mxr messages       # people and groups, with their topics
mxr todo           # tasks ordered by when to act
mxr updates        # notifications grouped into a briefing
mxr reading        # newsletters as an edition
mxr records        # receipts, orders, bookings and bills
```

## Each mode does one job, with its own verb

| Mode | What it holds | What you do there | When you're finished |
|---|---|---|---|
| Messages | People you are in conversation with | Reply, or start a new topic | Nobody is waiting on you |
| To do | Things you have to act on, such as a bill, a renewal or a form to sign | Do it, schedule it, tick it off | Nothing needs you until a named date |
| Updates | Notifications that tell you something happened | Glance, then let go of the whole batch | Nothing new since the last digest |
| Reading | Newsletters and posts you asked for | Read now, save for later, or unsubscribe | You're current; nothing is owed |
| Archive | Records: receipts, orders, bookings, invoices | Ask for a fact and copy it, or open the document | Never: records stay |

Two more views sit beside the modes. **Now** is the front page: at most ten
items drawn from the modes, so you can answer "what needs me right now?"
without opening each one. **Inbox** stays as the everything view, in
arrival order.

Messages, To do, [Updates](/guides/updates/), [Reading](/guides/reading/) and
Archive are built in their own shape.

## A conversation can belong to several modes

A conversation can contain a reply to write, an open task and a filed record.
Each mode tracks its own work. Finishing in Messages leaves an open to-do
alone. Archive keeps filed records without holding their email in the inbox.

Inspect membership and preview what finishing in one mode would do:

```bash
mxr modes why MESSAGE_ID --format json
mxr modes done THREAD_ID --mode messages --dry-run
```

When the last active mode lets go, mxr also archives the conversation at the
provider unless `modes.archive_on_last_done` is disabled. Review the preview
before applying `mxr modes done THREAD_ID --mode messages`.

## Sorting rules and categories serve different jobs

Sorting chooses where you work on mail. Sender signals and rules determine
placement; your explicit corrections take precedence. Inspect a message's
placement before correcting it:

```bash
mxr why MESSAGE_ID --format json
mxr reclassify MESSAGE_ID reading --dry-run
mxr reclassify MESSAGE_ID reading
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

Today semantic search indexes every message the same way: a header chunk
plus overlapping windows of the body and attachment text, embedded on your
machine. [Semantic search](/guides/semantic-search/) covers how to run it.

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
| Now | Built | [Now](/guides/now/) (`mxr now`, `g h`): People, Due soon, the Updates card and an evening Reading pick, at most ten things; the rail (`mxr modes rail`), done here per mode (`mxr modes done`) and "Also in" (`mxr modes why`) | The first-run progress card and catch-up across every mode |
| Messages | Built (`g m`) | [Messages](/guides/messages/) (`mxr messages`): people as rows with their topics inside, Your turn by closeness and pace, Got it (`mxr messages ack`), thread-aware quote stripping, manual merges with a suggestion; [Focus & reply](/guides/focus-and-reply/) | Incoming Gmail reactions as a mark on the message; the Messages index recipe once the retrieval eval favours it |
| To do | Built (`g x`) | [To do](/guides/todo/) (`mxr todo`): bills, renewals, forms and promises as one row each, with act-by dates; [review later email evidence](/guides/todo-resolution/) before accepting a completion or date change; [calendar invites](/guides/calendar-invites/) (`mxr invites`) | A rail badge once `mxr modes eval` shows under one false to-do a week |
| Updates | Built (`g u`) | [Updates](/guides/updates/) (`mxr updates`): one line per source at the 08:00 and 16:30 cuts, code-computed changes, parcels, builds and incidents as trackers, let go in one key (`mxr updates let-go`); a new sign-in, failed payment or delivery problem leads Needs a look as a suggested to-do that `t` adds; [deliveries](/guides/deliveries/) (`mxr deliveries`) | One fact per message in the semantic index, and model-written facts for generic subjects (phase 7) |
| Reading | Built (`g r`) | [Reading](/guides/reading/) (`mxr reading`): an edition banded since your last visit, earlier and fading, a 66-character reader with the linked article on request, a Later shelf, let go with `e` and `A`, [unsubscribe](/guides/unsubscribe/) with evidence, highlights | Section-aware search chunks and lazy embedding for whole issues |
| Archive | Built (`g e`) | [Archive](/guides/archive/) (`mxr records`): receipts, orders, bookings, bills and documents as one record each, an answer box (`mxr records ask "lisbon booking ref"`) with [`mxr ask`](/guides/archive-intelligence/) as its fallback, and a CSV export | Record fields read by the smart tier, and learned templates per issuer |

Classification by your own model is phase 7. Phase numbers come from the
plan and can change; this page changes when each phase ships.

## Deleting an email will delete what mxr derived from it (planned)

When your provider deletes a message and mxr syncs that change, mxr
removes the message, its body, its keyword search entry, its semantic
chunks in the database and the per-message data built from it. Some
derived data still outlives it today, such as cached summaries and gists,
extracted promises and decisions, and opened attachment files.
[Security & Privacy](/guides/security-and-privacy/#deleting-an-email-removes-most-of-what-mxr-derived-from-it)
lists what is and isn't removed. The modes plan treats "deleting an email
deletes everything derived from it" as a rule every new store must follow.

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
