---
title: Email is five apps at once
description: How mxr thinks about email as Messages, To do, Updates, Reading and Archive, what you can use for each today, and what is still planned.
---

Your inbox mixes five kinds of mail that ask for five different things.
A person is waiting on a reply. A bill has a deadline. A build notification
wants a glance. A newsletter you chose is there to read when you like. A
receipt matters a year from now, when you need its reference number. One
list of subject lines treats all five the same and offers the same
actions for each: reply, archive, label.

mxr's plan is to treat email as five apps that share one inbox, called
modes. Each mode has its own view, its own unit on screen and its own
verbs. Most of this is not built yet. Today you get the desk, Reading,
Paper trail, the screener, promises and deliveries, and each of those
already does part of one mode's job.

Start with what ships:

```bash
mxr desk           # people you owe, promises due, threads waiting on others
mxr reading        # newsletters and lists, grouped by sender
mxr paper-trail    # receipts and notifications, grouped by sender
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

The desk you use today is the first version of Now. Reading and Paper
trail are early versions of Reading, Updates and Archive.

## One email can live in several modes (planned)

A landlord writes "please sign and send it back by 15 Oct. Are you around
Thursday?" with the lease attached. Under the plan, that one email shows in
three modes, each showing its own part of it:

- Messages shows the landlord, with "are you around Thursday?" as what they
  asked.
- To do shows "Sign lease renewal, act by Mon 13, due Wed 15".
- Archive shows the signed lease once you've sent it back.

Each mode keeps its own done state, so replying in Messages doesn't tick
off the to-do. When you finish with an item in one mode, it moves on to the
mode that needs it next, and mxr tells you where it went: "Done in
Messages. Still in To do (due Wed)." The provider archive happens when the
last mode lets go.

Today a message is in one place at a time. The desk, Reading and Paper
trail are views over your inbox, and mail the rules place as from a person
goes to the desk, never to Reading or Paper trail.

## Rules decide first, then your model, and every item says why

Placement today is rule-based. The rules look at the sender's address,
domain and list headers, and your own choice for a sender always wins. Ask
mxr why a message is where it is:

```bash
mxr why MESSAGE_ID --format json
```

The output names the place, the rule that decided it and the reason
(`account_id` and `message_id` trimmed here):

```json
{
  "kind": "MessageKind",
  "sender_email": "uptime@alerts.demo.mxr.local",
  "mail_kind": {
    "kind": "paper_trail",
    "rule": "automated_domain",
    "reason": "automated sending domain",
    "corrected": false
  }
}
```

Correct a sender once and their mail follows from then on:

```bash
mxr sender kind news@example.com reading
```

The plan keeps rules as the first layer and adds two more. Message rules
will spot things a sender rule can't, such as a due date, an invoice in
schema.org markup or an unanswered invite. Then, only if you have
configured a language model, your model will decide what rules can't tell.
Every item will carry its reason ("Here because: asks you to renew, due 14
Oct (local model)"), and corrections will work per email as well as per
sender. With no model configured, rules still place everything.

The full placement rules for today are in the
[desk and places reference](/reference/desk-and-places/#placement-rules).

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

Running everything in the cloud with your own key will be supported, so you
won't need a local model server. Amounts and dates a model pulls out must
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
leaves reference numbers to exact keyword search. Each recipe replaces
today's chunking only if a retrieval test on real mail shows it finds
things at least as well.

See what today's index holds:

```bash
mxr semantic status
```

## What you can use today, mode by mode

| Mode | Status today | Shipped features that already serve it | Planned |
|---|---|---|---|
| Now | Partly built, as the desk | [The desk](/guides/desk/): You owe, Due, Waiting on and New from people lanes (`mxr desk`), with low tide when it's clear | Four fixed sections across the modes, at most ten items (phase 2) |
| Messages | Not built as a mode | [Owed replies](/guides/forgotten-work/) (`mxr owed`), [Focus & reply](/guides/focus-and-reply/), the reply queue (`mxr replies`), [sender view](/guides/sender-view/), gists that say what a person asks | People as rows with their topics inside, Got it, thread-aware quote stripping (phase 3) |
| To do | Not built as a mode | [Promises](/guides/forgotten-work/#commitments-promises-you-made) (`mxr commitments`) on the desk's Due lane, [calendar invites](/guides/calendar-invites/) (`mxr invites`), [reply later and snooze](/guides/triage-flow/) | Bills, renewals and forms detected with deadlines, act-by dates and one action button (phase 1) |
| Updates | Partly built, as Paper trail | [Paper trail](/guides/reading-and-paper-trail/) (`mxr paper-trail`, `mxr sweep`), [deliveries](/guides/deliveries/) (`mxr deliveries`), [rules](/guides/rules/) | A briefing by source in two digests a day, let go in one key (phase 4) |
| Reading | Partly built, as the Reading place | [Reading](/guides/reading-and-paper-trail/) (`mxr reading`), [subscriptions and unsubscribe](/guides/unsubscribe/) (`mxr subscriptions --rank`) | An edition with read time, a Later shelf, fading items (phase 5) |
| Archive | Not built as a mode | [Search](/guides/search/), [semantic search](/guides/semantic-search/), [archive intelligence](/guides/archive-intelligence/) (`mxr ask`, `mxr decisions`), pins in Paper trail | Records with an answer box: ask "lisbon booking" and get the reference back (phase 6) |

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
- [Clear the desk](/guides/desk/)
- [Clear Reading and Paper trail](/guides/reading-and-paper-trail/)
- [Glossary](/guides/glossary/#email-modes)
