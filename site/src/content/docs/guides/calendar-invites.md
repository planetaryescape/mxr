---
title: Handle calendar invites
description: Inspect, find, and RSVP to email calendar invites safely.
---

Handle meeting invites without leaving mxr. mxr parses `text/calendar`
parts and `.ics` attachments from stored mail, shows the invite in the
message view, collects every detected invite into a dedicated **Calendar
invites** page in the TUI and web app, lets you find invite-bearing
messages with search, and sends standards-based iMIP replies after a
dry-run preview.

## See what mxr found

List recent invites before acting on any one message.

```bash
mxr invites list --limit 20
```

What you get: a table of invite-bearing messages with the summary,
method, UID, sequence, organizer, start time, location, attendee state,
and parser warnings.

Use JSON when you are building a script or checking parser output.

```bash
mxr invites list --limit 20 --format json \
  | jq -r '.[] | "\(.message_id)\t\(.metadata.summary // "(no title)")"'
```

## Inspect one invite

Show the normalized invite attached to a message before deciding.

```bash
mxr invite show MESSAGE_ID --format json
```

What you get: one object with `message_id`, local invite row metadata,
and the parsed calendar metadata: `method`, `uid`, `sequence`,
`recurrence_id`, `starts_at`, `ends_at`, `organizer`, `attendees`,
`rrule`, `raw_ics`, and `warnings`.

For a quick terminal read, omit JSON.

```bash
mxr invite show MESSAGE_ID
```

## Reply safely

Always dry-run before sending. The dry-run builds the exact iMIP
`METHOD:REPLY` body and calendar part, but does not contact the provider.

```bash
mxr invite reply MESSAGE_ID accept --dry-run --format json
```

What you get: a preview with the attendee address mxr matched, organizer
address, generated subject/body, generated `text/calendar` reply, and any
warnings.

Send only after the preview looks right.

```bash
mxr invite reply MESSAGE_ID accept
```

Use the same path for maybe or no.

```bash
mxr invite reply MESSAGE_ID tentative --dry-run
mxr invite reply MESSAGE_ID decline --dry-run
```

## Find invite mail

Calendar invites are indexed as a search filter.

```bash
mxr search 'has:calendar newer_than:30d' --format ids
```

What you get: one message ID per invite-bearing message, ready to pass to
`mxr invite show`, `mxr thread`, or an fzf picker.

Aliases are available when you think in email-app language.

```bash
mxr search 'has:invite' --format ids
mxr search 'has:invites from:alice@example.com' --format json
```

## Backfill after upgrade

New syncs persist invite rows automatically. If you already had invite
mail in SQLite before upgrading, backfill from stored body metadata.

```bash
mxr invites backfill --format json
```

What you get: a count of invite rows restored from existing message
bodies. Run it once after upgrading; future sync batches keep the table
fresh.

Check the result with search.

```bash
mxr search 'has:calendar' --format ids | head
```

## In real life

### Prepare for today's meetings

```bash
mxr search 'has:calendar newer_than:7d' --format json \
  | jq -r '.[] | "\(.date)\t\(.from)\t\(.subject)"'
```

What you get: a small agenda-like list from mail, useful when the
calendar app is not the system of record.

### Review every pending RSVP

```bash
mxr invites list --format jsonl \
  | jq -r 'select((.metadata.attendees // [])[]?.partstat == "NEEDS-ACTION")
           | .message_id' \
  | while IFS= read -r id; do
      mxr invite show "$id"
    done
```

What you get: each invite that still needs action, expanded one at a time
so you can decide deliberately.

### Let fzf pick the meeting

```bash
mxr invites list --format jsonl \
  | jq -r '"\(.message_id)\t\(.metadata.starts_at // "")\t\(.metadata.summary // "(no title)")"' \
  | fzf --delimiter='\t' --with-nth=2,3 \
  | cut -f1 \
  | xargs -I{} mxr invite show {}
```

What you get: an interactive invite picker keyed by time and summary.

## Agent prompts that work

```text
"Find calendar invites from the last 30 days. Use `mxr search
'has:calendar newer_than:30d' --format ids`, inspect each with
`mxr invite show`, and do not RSVP without showing me
`mxr invite reply MESSAGE_ID <action> --dry-run --format json` first."
```

```bash
mxr invite reply MESSAGE_ID accept --dry-run --format json
```

## In the apps

The same invites are a dedicated, scrollable surface in both UIs — one
place to see every detected invite and RSVP without opening each message.
Rows are event-centric (when, summary, organizer, your RSVP status), built
from the local invite table rather than plain message rows. All surfaces
read the same `Request::ListInvites`, so the list is identical whether you
use the CLI, TUI, or web.

- **TUI:** open **Calendar invites** from the sidebar (below Owed). Then
  `j`/`k` to move, `a` accept · `t` (or `m`) tentative · `d` decline,
  `A`/`T`/`D` to reply with a comment, `Enter`/`o` to open the underlying
  message, `h` back to the sidebar. An RSVP sends after a short window;
  press `u` to undo before it goes out. The row's status refreshes once it
  lands. See [keybindings](/reference/keybindings/#calendar-invites-lens).
- **Web:** the **Calendar invites** entry in the sidebar opens `/invites` —
  the same list with inline Accept / Tentative / Decline buttons (and a
  "with comment" option in the row menu). See
  [Web app](/guides/web-app/#calendar-invites-page).

## See also

- [Search workflow](/guides/search/)
- [Pre-send safety](/guides/pre-send-safety/)
- [Web app](/guides/web-app/#calendar-invites-page)
- [CLI: mxr invite](/reference/cli/invite/)
- [CLI: mxr invites](/reference/cli/invites/)
- [JSON output schemas](/reference/json-output/)

```bash
mxr invites list --limit 5
```
