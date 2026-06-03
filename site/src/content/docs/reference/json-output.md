---
title: JSON output schemas
description: Field names returned by `mxr ... --format json`, `--format jsonl`, and `--format ids` for jq, agents, and scripts.
---

mxr's CLI JSON is the automation contract. Human `table` output can change; `json`, `jsonl`, `csv`, and `ids` are the surfaces to script.

The CLI does **not** always mirror daemon IPC structs. Some commands intentionally emit smaller records that are easier to pipe.

## Search result

`mxr search QUERY --format json` returns an array. `--format jsonl` returns the same records, one per line.

```json
[
  {
    "message_id": "01JFQ7K3M2X8N5R0VYZA9CTBPE",
    "from": "Sarah Chen <sarah@example.com>",
    "subject": "1:1 prep, Friday",
    "date": "2026-04-30T15:42:11+00:00",
    "read": false,
    "starred": true,
    "score": 12.4
  }
]
```

| Field | Type | Notes |
|---|---|---|
| `message_id` | string | Pass to `mxr cat`, `mxr reply`, mutations, etc. |
| `from` | string | Display-ready sender, usually `Name <email>`. |
| `subject` | string | Empty subjects are possible. |
| `date` | string | RFC 3339 timestamp. |
| `read` | bool | Current local read state. |
| `starred` | bool | Current local starred state. |
| `score` | number | Search relevance score. Date-sorted queries still include it. |

With `--explain`, `mxr search` wraps the payload:

```json
{
  "results": [/* search result */],
  "explain": { /* search-mode diagnostics */ }
}
```

## Message and thread reads

`mxr cat`, `mxr thread`, `mxr headers`, and `mxr export --format json` return fuller daemon payloads because they are read surfaces, not compact search rows. Prefer `mxr search --format ids` when you only need IDs.

```bash
mxr search 'from:sarah after:2026-04-23' --format ids \
  | mxr archive --dry-run
```

## `--format ids`

`--format ids` prints one ID per line. Mutations with omitted positional IDs read that form from stdin.

```bash
mxr search 'label:newsletters older_than:30d' --format ids \
  | mxr archive --dry-run
```

This is safer and more portable than `xargs -r` on macOS.

## Subscription ranking

`mxr subscriptions --rank --format json` returns an array of subscription
sender records. `--rank` sorts by `opened_count / message_count` ascending,
then by `archived_unread_count` descending.

```bash
mxr subscriptions --rank --format json \
  | jq '.[0] | {
      sender_email,
      message_count,
      opened_count,
      replied_count,
      archived_unread_count
    }'
```

| Field | Type | Notes |
|---|---|---|
| `sender_email` | string | Sender address for the bucket; grouping is case-insensitive. |
| `message_count` | number | Non-trash, non-spam messages from that sender with an unsubscribe method. |
| `opened_count` | number | Messages in the bucket whose local `READ` flag is set. This is not tracking-pixel telemetry or distinct open events. |
| `archived_unread_count` | number | Messages that are archived while still unread; tie-breaker for `--rank`. |
| `replied_count` | number | Stable JSON field, currently `0` for `subscriptions`; reply-pair counts power sender/contact analytics, not this ranker yet. |

If `opened_count == message_count`, every message in that sender bucket is read
locally. That can come from the `mxr read` command, another mail client,
provider-side read state, filters, or bulk mark-read actions.

## Mutation dry-run

Bulk mutation dry-runs return a preview object:

```json
{
  "action": "archive",
  "dry_run": true,
  "requested": 2,
  "message_ids": ["01JFQ...", "01JFR..."],
  "messages": [
    {
      "message_id": "01JFQ...",
      "from": "Alice",
      "subject": "Quarterly review"
    }
  ]
}
```

`--format jsonl` emits one preview line per message:

```json
{"action":"archive","dry_run":true,"message_id":"01JFQ...","from":"Alice","subject":"Quarterly review"}
```

## Mutation result

Batch mutations return a summary object:

```json
{
  "action": "archive",
  "dry_run": false,
  "requested": 2,
  "succeeded": 2,
  "failed": 0,
  "message_ids": ["01JFQ...", "01JFR..."],
  "errors": []
}
```

Single-message mutation commands can return a command-specific `result` payload, but the stable fields are `action`, `dry_run`, and `message_ids`.

## Calendar invite

`mxr invite show MESSAGE_ID --format json` returns one invite object.
`mxr invites list --format json` returns an array of the same shape;
`--format jsonl` emits one object per line.

```bash
mxr invite show MESSAGE_ID --format json
```

```json
{
  "id": "018f8c0f-7b78-7c44-9f48-3e5a0ef4f7aa",
  "account_id": "01JFQ7K3M2X8N5R0VYZA9CTBPE",
  "message_id": "01JFQ8A2KRZ3F2HQ3V9T6QZJ7N",
  "metadata": {
    "method": "REQUEST",
    "component_kind": "VEVENT",
    "uid": "meeting-123@example.com",
    "sequence": 2,
    "recurrence_id": null,
    "summary": "Planning session",
    "starts_at": "20260518T140000Z",
    "ends_at": "20260518T143000Z",
    "location": "Room 3",
    "organizer": {
      "email": "alice@example.com",
      "name": "Alice"
    },
    "attendees": [
      {
        "email": "you@example.com",
        "name": "You",
        "partstat": "NEEDS-ACTION",
        "role": "REQ-PARTICIPANT",
        "rsvp": true
      }
    ],
    "warnings": []
  },
  "created_at": 1778832070,
  "updated_at": 1778832070
}
```

Important fields:

| Field | Type | Notes |
|---|---|---|
| `message_id` | string | Pass to `mxr invite reply`, `mxr thread`, or `mxr cat`. |
| `metadata.method` | string | Calendar method such as `REQUEST`, `CANCEL`, or `REPLY`. RSVP sending only supports actionable `REQUEST` invites. |
| `metadata.uid` | string | iCalendar UID. Used with sequence and recurrence identity for update safety. |
| `metadata.sequence` | number or null | Higher sequence means a newer invite update exists. |
| `metadata.recurrence_id` | string or null | Identifies one instance of a recurring event. |
| `metadata.raw_ics` | string or null | Raw local calendar text, useful for debugging and dry-run inspection. |
| `metadata.warnings` | string[] | Parser or safety warnings to show before replying. |

## Calendar invite RSVP

`mxr invite reply MESSAGE_ID accept --dry-run --format json` returns the
preview directly. Without `--dry-run`, it sends and returns the result
directly.

```bash
mxr invite reply MESSAGE_ID accept --dry-run --format json
```

```json
{
  "message_id": "01JFQ8A2KRZ3F2HQ3V9T6QZJ7N",
  "action": "accept",
  "attendee_email": "you@example.com",
  "organizer_email": "alice@example.com",
  "subject": "Accepted: Planning session",
  "body_text": "You accepted this invitation.",
  "ics": "BEGIN:VCALENDAR\nMETHOD:REPLY\n...",
  "warnings": []
}
```

Successful sends return:

```json
{
  "message_id": "01JFQ8A2KRZ3F2HQ3V9T6QZJ7N",
  "action": "accept",
  "provider_message_id": "provider-id-or-null",
  "rfc2822_message_id": "<mxr-generated@example.local>"
}
```

## Common `jq` patterns

```bash
# Senders by volume from compact search rows
mxr search 'newer_than:7d' --format json \
  | jq -r 'group_by(.from)
           | map({sender: .[0].from, count: length})
           | sort_by(-.count) | .[]
           | "\(.count)\t\(.sender)"'

# Subjects from a sender
mxr search 'from:legal@' --format jsonl \
  | jq -r '.subject'

# IDs from attachment-bearing matches
mxr search 'has:attachment older_than:30d' --format ids

# Invite IDs to inspect before replying
mxr search 'has:calendar newer_than:30d' --format ids \
  | xargs -I{} mxr invite show {} --format json
```

## See also

- [CLI overview](/reference/cli/) — every command and its accepted output formats
- [Automation contract](/guides/automation-contract/) — stdin IDs, dry-run, and confirmation rules
- [Recipes](/guides/recipes/) — pipelines using these shapes
- [HTTP bridge](/reference/bridge/) — HTTP routes and daemon payloads
