---
title: Automated follow-ups
description: Auto-reminders, send-later, and the daemon background loops that drive them.
---

## The promise

Email follow-up is a memory tax. mxr's automated follow-ups remove it:

- Send a message and configure a reminder: if no reply lands by the
  given time, mxr surfaces it back to you.
- Schedule a draft for a future send time: write now, deliver later.

Both run as 60-second daemon background loops. They survive daemon
restarts, idempotently.

## Auto-reminders

Set on outbound messages. Stored in the `auto_reminders` table; fired
by the `auto_reminders_loop`.

```bash
# After sending, add a follow-up reminder:
mxr remind MESSAGE_ID --when "in 5d"
mxr remind MESSAGE_ID --when "monday 9am"

# Send a reply and set the follow-up in one step:
mxr reply MESSAGE_ID --body "Thanks, I'll check." --yes --remind-after "in 5d"

# Cancel before it fires:
mxr remind MESSAGE_ID --cancel
```

In the TUI compose confirmation, press `n`, enter the same relative
time string, and press `Enter` to send and set the reminder in one
flow. Use `Ctrl-p → Cancel Reminder` from the focused sent message to
cancel a pending reminder.

A reply from a person cancels the reminder: any message from someone else
stored in the conversation after yours, or one that names your message in
its In-Reply-To or References headers wherever it was filed. An
auto-responder or notification does not count. Storage order decides, not
the Date header. The check runs when the reminder is due, and a reply that
arrives while it runs stops the reminder from firing until it is checked.

While the reminder is pending, the conversation stays off the desk's
Waiting on. When the time elapses, mxr marks the sent message for
reply-later, refreshes `is:reply-later` search state, puts the conversation
back on Waiting on, and emits one `ReminderTriggered` event so connected
clients can surface the follow-up. It fires once, even if the daemon was
stopped at the time and started later. Re-setting the reminder on the same
message replaces the existing schedule. `mxr desk later THREAD_ID --at TIME`
sets the same reminder on a conversation you wrote last, with a dry run and
undo ([come back to it later](/guides/desk/#come-back-to-it-later)).

## Send Later

Schedule a draft for a future send time. The daemon's flusher loop
picks up due drafts and runs them through the same `send_stored_draft`
pipeline that interactive sends use, so the message arrives at the
provider exactly as it would have if you'd sent it manually then.

```bash
# Compose a draft (any usual flow), then schedule it:
mxr send DRAFT_ID --at "in 1h"
mxr send DRAFT_ID --at "tomorrow 9am"
mxr send DRAFT_ID --at "monday 17:00"

# Cancel a scheduled send:
mxr unsend DRAFT_ID
```

The draft itself is preserved on cancel; you can edit and reschedule.

### Idempotency under daemon restart

The flusher clears the `send_at` flag on a draft *before* invoking the
send. If the daemon crashes mid-send, the draft's status state machine
takes over: a draft in `'sending'` whose heartbeat is older than 1
hour is considered orphaned and reset to `'draft'` on the next
daemon startup, ready for the user to retry.

## Time syntax

Both `mxr remind --when` and `mxr send --at` accept the same
[time phrases](/reference/time-phrases/) as `mxr snooze --until`,
resolved in local time:

- **Relative durations**: `in 30m`, `in 2h`, `in 5d`, `in 2w`, `3d`
- **Named days**: `tomorrow`, `monday`, `tue`, `next week`, `3 oct`
- **Day + time**: `tomorrow 9am`, `monday 17:00`, `fri 3`
- **Today**: `today 17:00` (must be a future time)
- **RFC3339**: `2026-06-01T15:00:00Z`

Past times are rejected. "Today" without a specific time is rejected
(too ambiguous). Preview any phrase with `mxr time "<phrase>"`.

## Operational notes

- The reminder + send-later loops run every 60 seconds. The minimum
  practical scheduling resolution is therefore 60s; finer-grained
  scheduling would require ticker tuning.
- Both loops are crash-safe by virtue of the underlying state being in
  SQLite: restarting the daemon picks up where it left off.
- A cancelled reminder keeps its row with `cancelled_at` set. A
  cancelled send (`mxr unsend`) only clears the draft's send time; the
  draft stays.

## Composition with reply-later

If a reminder fires on an outbound message that's still awaiting a
reply, mxr flags the original outbound for reply-later so it shows up
in `mxr replies` alongside the rest of your follow-up queue:

```bash
mxr replies --format json
```

What you get: the same reply-later queue, now including due
auto-reminders.
