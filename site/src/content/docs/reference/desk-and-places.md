---
title: Desk and places
description: The desk's lanes and Done rules, how mail is placed, and the JSON each command prints.
---

This page lists the rules behind the [desk](/guides/desk/) and the two
places, [Reading and Paper trail](/guides/reading-and-paper-trail/), and the
output of their commands. For keys, see the
[keybindings reference](/reference/keybindings/). For flags, see the generated
pages for [`mxr desk`](/reference/cli/desk/),
[`mxr reading`](/reference/cli/reading/),
[`mxr paper-trail`](/reference/cli/paper-trail/),
[`mxr sweep`](/reference/cli/sweep/), [`mxr why`](/reference/cli/why/),
[`mxr sender`](/reference/cli/sender/) and [`mxr pin`](/reference/cli/pin/).

The desk and the places are built from the local store: who wrote last in
each conversation, your contacts, screener decisions, past reply times, open
commitments and cadence watches. No language model decides what goes where.

## Lanes

| Lane | JSON key | A conversation is here when | Order within the lane |
|---|---|---|---|
| You owe | `owed` | Someone you are in conversation with wrote last and you have not replied. | Furthest past your usual reply time first. |
| Due | `due` | A promise you made is due within 7 days, or overdue by up to 30 days. | By due date. |
| Waiting on | `waiting` | You wrote last, at least 12 hours ago, and they have not answered. A watched contact who has gone quiet longer than their [cadence](/guides/timing-and-cadence/) joins too. A time set with [`mxr desk later`](#later) that passed with no reply brings it back, however old. | Furthest past their usual reply time first. |
| New from people | `people_new` | A person you have not written to before wrote in the last 7 days. | Newest first. |

- "In conversation with" means you have written to them before, or you
  allowed them in the [screener](/guides/triage-flow/). An allowed sender's
  new mail lands in You owe, not New from people.
- The thread lanes only look at conversations with activity in the last 30
  days.
- A conversation that qualifies for two lanes shows once, in the first of:
  You owe, Due, Waiting on, New from people.
- A conversation set to reply later is off the thread lanes until its time.
  From then it is in You owe while their message is not in the trash, even
  if it is archived, older than 30 days, or from a sender that is not a
  person: you asked for it back.
- "Usual" is the median of your past replies to that person (or theirs to
  you), and needs at least two of them. With no history, ordering assumes 24
  hours.
- A row is `overdue` when it is at least an hour old and past the usual
  time, when a promise is past its date, or when a watched contact is past
  their cadence. The web app colours the row's age when it is overdue.

### When a row leaves on its own

| Lane | Leaves when |
|---|---|
| You owe, New from people | The conversation leaves the inbox: archived, snoozed or trashed. You reply. |
| Waiting on | They reply. The conversation is snoozed or trashed, or archived when it has mail from them. A conversation with only your own messages stays after archive. |
| Waiting on (watched contact) | They write. Archive does not take it off. |
| Due | The promise is resolved. What happens to the conversation does not matter. |
| Any thread lane | You set a time with [`mxr desk later`](#later). It comes back then. |

### Reasons

Each row's `reason` says why it is there, for example: `wrote to you`,
`replied to your message`, `2 messages since you last wrote`, `copied you`,
`first message from them`, `no reply to your last message`,
`you followed up, no reply yet`, `usually in touch every ...`,
`back from reply later`, `no reply by the time you set`. A Due row's
reason is the promise in your own words. A row a time brought back keeps
its reason even when a gist has an ask.

## Everything else

The desk's `elsewhere` counts are not unread counts:

| Key | Counts |
|---|---|
| `reading` | Reading mail that arrived in the inbox in the last 7 days, read or not. |
| `paper_trail` | Paper trail mail from the last 7 days, deliveries and invites excluded. |
| `deliveries` | Active deliveries. |
| `invites` | Upcoming invites you have not answered. |
| `screener` | New senders with no decision yet, for `screener_account`. |

## Done

`mxr desk done`, and Done in the web app and TUI, do the same thing per lane:

| Lane | Archives | Marks read | Also |
|---|---|---|---|
| You owe | Yes | Yes | Off the desk until someone writes. |
| New from people | Yes | Yes | Off the desk until someone writes. |
| Waiting on | No | Yes | Stops waiting until someone writes. |
| Due | No | Yes | Resolves that promise. Other open promises in the conversation stay. |

- Every lane also takes the conversation out of the reply-later queue, a
  timed one included, and cancels a pending "bring it back" time
  (`reminders_cancelled`), so nothing Done put away comes back on a timer.
- "Until someone writes" means until a new message is stored in the
  conversation, from them or from you. Moving it back to the inbox by hand
  does not bring it back. A resolved promise does not come back.
- Without `--lane`, the lane is Waiting on when you wrote last, otherwise
  You owe. `--promise COMMITMENT_ID` implies Due and takes one thread id.
- Undo restores labels, read state, the dismissal, the promise, the
  reply-later flag with its original time and any time it was set for, and
  a cancelled "bring it back" time. The undo window is 60 seconds.
- When a conversation cannot be put away (its account is offline, say),
  its item has an `error`, the rest are still done, and the command exits
  non-zero. The first attempt's undo id still restores everything it
  changed. Running Done again is a new Done with its own undo id.

`mxr desk dismiss` is the older "done waiting" on its own: no mark read, no
reply-later change. `mxr desk restore` reverses it.

## Later

`mxr desk later THREAD_ID... --at TIME`, and `b` in the web app and TUI,
set a time on each conversation. Who wrote last decides what it does, the
same rule Done uses without `--lane`:

| Who wrote last | `kind` | Until the time | At the time |
|---|---|---|---|
| They did | `reply_later` | Off the desk and out of the reply queue. The inbox is not touched. | Back in You owe (`back from reply later`) and the reply queue, ranked by the time. |
| You did | `waiting` | Off Waiting on. | If nobody else has written since your message: back in Waiting on (`no reply by the time you set`) and your message joins the reply queue. |

- `--at` is resolved in local time before anything is sent; the daemon gets
  the instant, never the words, and refuses a time that has passed. The web
  app sends the instant its preview showed.
- Reply later moves every reply-later flag in the conversation to the time.
  Waiting clears the conversation's reply-later flags and cancels its other
  pending reminders, so this one time is what brings it back.
- Who wrote last, and whether someone replied, go by the order mail was
  stored, not its Date header, and count only people: an auto-responder or
  notification neither makes it theirs nor cancels a wait. A person's reply
  cancels a waiting time, before or at the time. A new message in a
  reply-later conversation does not bring it back early.
- A conversation is back as soon as its time passes, whether or not the
  daemon was running; a wait the daemon fires late (it was off for weeks)
  still comes back. The daemon announces each return once per
  conversation (`ReplyLaterReturned`, or `ReminderTriggered` for waiting),
  even across restarts. Moving a time later always wins over the old one.
- Undo puts back what it replaced, and takes the reply-queue entry a wait
  added if it fired in the meantime.
- `--dry-run` returns the same items without changing anything. The real
  run prints an undo id; undo puts back every flag and reminder it replaced.
  A conversation that cannot be set has an `error` and the command exits
  non-zero.

```json
{
  "dry_run": false,
  "until": "2026-10-06T08:00:00Z",
  "mutation_id": "01a0f2d3-5a6c-7db2-8456-4781c4303b08",
  "undo_unavailable": false,
  "items": [
    {
      "thread_id": "309ae832-4d84-5d78-a3ef-76a7eda21496",
      "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
      "kind": "waiting",
      "message_id": "b5700241-8d68-562a-92d3-ec69db053cea"
    }
  ]
}
```

`message_id` is the message the time is on: their latest message for reply
later, yours for waiting.

## Placement rules

Every message in the inbox that is not from a person goes to Reading or
Paper trail. The first rule that matches wins:

| # | Rule | Place | `rule` |
|---|---|---|---|
| 1 | You moved this sender with `mxr sender kind` or Move sender. | Your choice | `decision` |
| 2 | A delivery update or a calendar invite. | Neither: they have their own pages | `delivery`, `invite` |
| 3 | A notifying local part: `notifications@`, `alerts@`, `receipts@`, `billing@` and similar, matched anywhere in the local part. | Paper trail | `automated_address` |
| 4 | A newsletter local part: `newsletter@`, `digest@` and similar. | Reading | `newsletter_address` |
| 5 | A notifying subdomain, such as `alerts.example.com`. | Paper trail | `automated_domain` |
| 6 | A newsletter subdomain, such as `news.`, `updates.` or `marketing.`. | Reading | `newsletter_domain` |
| 7 | A `List-Id` header, then a `List-Unsubscribe` header. | Reading | `list_id`, `list_unsubscribe` |
| 8 | A `no-reply@` address with no list headers. | Paper trail | `no_reply_address` |
| 9 | A sender known to write to lists. | Reading | `list_sender` |
| 10 | Anything else. | A person: the desk | `person` |

Subdomain rules look at subdomain labels only, so `news.com` itself does not
match rule 6. A notifying address that also carries `List-Unsubscribe`
(GitHub notifications do) stays in Paper trail. A `no-reply@` sender with
list headers is marketing, so rule 7 sends it to Reading. The rules run per
message, so one sender can appear in both places. `mxr why MESSAGE_ID` prints
the place and the rule.

Both places are views over the inbox. Snoozed mail, mail you sent, deliveries
and invites are never in a place.

### Sender kinds

A sender kind is stored as the sender's screener decision, so the
Screener's **Decisions** tab lists it too:

| `mxr sender kind` | Screener decision | Where their mail goes |
|---|---|---|
| `people` | allow | The desk. |
| `reading` | feed | Reading. |
| `paper-trail` | paper-trail | Paper trail. |
| `screened-out` | deny | Nowhere. New inbound mail is trashed and marked read as it syncs. |
| `auto` | (cleared) | The placement rules decide again. |

A move applies to the sender's existing inbox mail at once and to their mail
from then on. It keeps any screener route label. Mail that releases before
v0.6.38 archived on arrival for feed and paper-trail senders stays archived.

## Sweep

- A sweep archives every unpinned message in one sender's bundle
  (`--sender`) or in the whole place.
- The preview is the daemon's dry run of the same request. It returns a
  `preview_token`. The real sweep archives only the messages that preview
  listed, and rechecks each chunk: mail that arrived after the preview, or a
  message pinned or moved away since, stays.
- A token works once, for the sweep it came from, and expires after 10
  minutes. An expired token archives nothing.
- In the web app and TUI, a whole-place sweep confirm opens on **Cancel**;
  one sender's sweep confirms directly.
- `--dry-run` and `--yes` cannot be combined. Without either, `mxr sweep`
  asks on a terminal and refuses elsewhere.
- Pins are local to this machine. They are not provider stars.
- The daemon returns at most 500 bundles per page and 200 messages per
  bundle.

## Output

### `mxr desk --format json`

Trimmed to one row:

```json
{
  "kind": "Desk",
  "owed": {
    "rows": [
      {
        "lane": "owed",
        "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
        "thread_id": "080a03cf-08ab-5aca-a5e6-73c9f269800d",
        "message_id": "51cc03e8-5bb2-5e6d-8df7-1aa6ae33f04c",
        "message_ids": ["7e7baa56-...", "51cc03e8-5bb2-5e6d-8df7-1aa6ae33f04c"],
        "counterparty_email": "jon@papertrail.example",
        "counterparty_name": "Jon Bell",
        "subject": "Research notes: terminal workflows",
        "reason": "wrote to you",
        "since": "2026-09-27T10:26:13Z",
        "age_seconds": 80528,
        "usual_seconds": 2820,
        "usual_samples": 2,
        "overdue": true,
        "unread": true,
        "starred": false
      }
    ],
    "total": 14
  },
  "due": { "rows": [], "total": 0 },
  "waiting": { "rows": [], "total": 4 },
  "people_new": { "rows": [], "total": 7 },
  "elsewhere": {
    "reading": 6,
    "paper_trail": 3,
    "deliveries": 2,
    "invites": 0,
    "screener": 27,
    "screener_account": "190b5fb4-5733-5322-8aa6-5f775e603573"
  },
  "last_from_people_at": "2026-09-28T07:14:13Z",
  "generated_at": "2026-09-28T08:48:21.503732Z"
}
```

- `usual_seconds` is absent when the pace is unknown.
- A Due row also has `commitment_id`. Pass it to `mxr desk done --promise`.
- A row a time you set brought back has `back_at`, the time that was set,
  and is `overdue`.
- `--format jsonl` prints one row per line with its `lane`, `--format ids`
  prints thread ids, and `--format csv` prints the rows as CSV.
- `--limit` (default 25) caps rows per lane. Each lane's `total` still
  counts all of them.
- `--gists` adds each conversation's cached gist, never waiting on a model:
  `gists` (a list in desk order, `[]` when none is cached) in JSON, a
  `gist` field (`null` when none) on each JSONL row, and `gist` and `ask`
  columns in CSV. The table shows the ask in place of the reason on You owe
  and New from people, and the gist under the row. Gist fields are
  described under
  [`mxr briefing gists`](/guides/briefings-and-loop-in/#many-conversations-at-once-mxr-briefing-gists).

### `mxr desk done --format json`

```json
{
  "dry_run": true,
  "items": [
    {
      "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
      "thread_id": "309ae832-4d84-5d78-a3ef-76a7eda21496",
      "lane": "waiting",
      "archived": 0,
      "marked_read": 0,
      "dismissed": true,
      "reply_later_cleared": 0,
      "reminders_cancelled": 0
    }
  ],
  "mutation_id": null,
  "undo_unavailable": false
}
```

A real run has the `mutation_id` to pass to `mxr undo`. An item can also
carry `resolved_commitment_id` (Due) and `error` (it could not be put away).

### `mxr reading` and `mxr paper-trail --format json`

```json
{
  "kind": "Place",
  "place": "paper_trail",
  "bundles": [
    {
      "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
      "sender_email": "uptime@alerts.demo.mxr.local",
      "sender_name": "Uptime Robot",
      "kind": {
        "kind": "paper_trail",
        "rule": "automated_domain",
        "reason": "automated sending domain",
        "corrected": false
      },
      "message_count": 1,
      "unread_count": 1,
      "pinned_count": 0,
      "newest_at": "2026-09-28T03:42:13Z",
      "newest_subject": "Interview panel for Staff Engineer candidate",
      "messages": [
        {
          "message_id": "cc2cbc75-87a9-5c63-b941-aa63ec3607fb",
          "thread_id": "33497bb2-2a20-5be5-90fd-c316e8565630",
          "subject": "Interview panel for Staff Engineer candidate",
          "snippet": "Alert #0: status changed, investigate if this is still active",
          "date": "2026-09-28T03:42:13Z",
          "unread": true,
          "pinned": false,
          "starred": false
        }
      ]
    }
  ],
  "total_bundles": 3,
  "total_messages": 3
}
```

`kind.corrected` is `true` when the placement comes from your sender kind.
`--messages` (default 3) sets how many messages each bundle lists; the counts
always cover the whole bundle. `--limit` (default 50) and `--offset` page
through bundles. `--format jsonl` prints one bundle per line and
`--format ids` prints the listed message ids.

### `mxr why --format json`

```json
{
  "kind": "MessageKind",
  "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
  "message_id": "cc2cbc75-87a9-5c63-b941-aa63ec3607fb",
  "sender_email": "uptime@alerts.demo.mxr.local",
  "mail_kind": {
    "kind": "paper_trail",
    "rule": "automated_domain",
    "reason": "automated sending domain",
    "corrected": false
  }
}
```

### `mxr sweep --format json`

A preview (`--dry-run`):

```json
{
  "dry_run": true,
  "archived": 0,
  "job": null,
  "preview": {
    "place": "paper_trail",
    "sender_email": "pager@alerts.demo.mxr.local",
    "count": 1,
    "pinned_excluded": 0,
    "preview_token": "<preview-token>",
    "sample_subjects": ["Launch checklist for Project Aurora"],
    "senders": [
      {
        "account_id": "190b5fb4-5733-5322-8aa6-5f775e603573",
        "sender_email": "pager@alerts.demo.mxr.local",
        "sender_name": "Pager Relay",
        "count": 1
      }
    ]
  }
}
```

A real sweep (`--yes`) sets `archived` and `job`. The archive runs as a
background job, and `job.undo_ids` holds one mutation id per chunk: run
`mxr undo` once for each.

```json
{
  "archived": 1,
  "job": {
    "kind": "mutation.archive",
    "status": "succeeded",
    "progress": { "total": 1, "succeeded": 1, "failed": 0, "skipped": 0, "completed": 1 },
    "undo_ids": ["01a0e736-9a10-7280-aa5a-58382e053076"]
  }
}
```
