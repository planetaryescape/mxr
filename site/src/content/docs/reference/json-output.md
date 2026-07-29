---
title: JSON output schemas
description: Field names returned by `mxr ... --format json`, `--format jsonl`, and `--format ids` for jq, agents, and scripts.
---

mxr's CLI JSON is the automation contract. Human `table` output can change; `json`, `jsonl`, `csv`, and `ids` are the surfaces to script.

The CLI does **not** always mirror daemon IPC structs. Some commands intentionally emit smaller records that are easier to pipe.

## Search result

`mxr search QUERY --format json` returns one object with `results`, `paging`, and `explain`. It is not a bare array, so `jq` filters need to go through `.results`.

```json
{
  "results": [
    {
      "message_id": "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
      "from": "Sarah Chen <sarah@example.com>",
      "subject": "1:1 prep, Friday",
      "date": "2026-04-30T15:42:11+00:00",
      "read": false,
      "starred": true,
      "score": 1777563776.0
    }
  ],
  "paging": {
    "limit": 50,
    "offset": 0,
    "total": 73,
    "has_more": true,
    "next_offset": 50
  },
  "explain": null
}
```

| Field | Type | Notes |
|---|---|---|
| `message_id` | string | Pass to `mxr cat`, `mxr reply`, mutations, etc. |
| `from` | string | Display-ready sender, usually `Name <email>`. Missing display names leave a leading space before `<email>`. |
| `subject` | string | Empty subjects are possible. |
| `date` | string | RFC 3339 timestamp. |
| `read` | bool | Current local read state. |
| `starred` | bool | Current local starred state. |
| `score` | number | The ranking value the executed search produced, as a 32-bit float. Its meaning depends on how the search ran; see below. |

The mode you request and the path the search falls back to together decide what `score` holds.

| Request | Execution | `score` |
|---|---|---|
| `--mode lexical --sort relevance` | lexical | BM25 relevance |
| `--mode lexical` with the default date sort | lexical | message date in Unix seconds |
| `--mode hybrid` | hybrid | RRF fusion score |
| `--mode semantic` | semantic | dense score |
| `--mode hybrid` or `--mode semantic` | lexical fallback | BM25 relevance |
| any mode, query rejected by the structured parser | Tantivy fallback | matches the requested sort |

Omitting `--mode` picks the request path from `search.default_mode`.

An `f32` carries 24 mantissa bits, so present-day epoch seconds land on 128-second steps: the example message is 1777563731 and comes back as `1777563776.0`. Read `date` when you need a stable value.

### Paging

`paging` sits in the JSON envelope. `--format jsonl` and `--format csv` write the same object to stderr instead.

| Field | Type | Notes |
|---|---|---|
| `limit` | number | The page size the CLI sent. Defaults to 50 when `--limit` is omitted. |
| `offset` | number | The `--offset` value. Defaults to 0. |
| `total` | number | Match count for the query, not the number of rows on this page. |
| `has_more` | bool | True when matches exist past this page. |
| `next_offset` | number or null | Offset to pass to `--offset` for the next page. Null when `has_more` is false. |

`results` is `[]` when nothing matches, and `total` is then 0. A lexical request reports the full match count for the query. A hybrid or semantic request ranks a bounded candidate window, and its `total` can be the size of that pool rather than a full count. Use `has_more` and `next_offset` to page.

### Explain

`explain` is null unless you pass `--explain`.

```bash
mxr search 'quarterly report' --mode hybrid --explain --format json
```

An illustrative `explain` for that command, on a config with semantic search switched off:

```json
{
  "explain": {
    "requested_mode": "hybrid",
    "executed_mode": "lexical",
    "semantic_query": "quarterly report",
    "lexical_window": 204,
    "dense_window": null,
    "lexical_candidates": 73,
    "dense_candidates": 0,
    "final_results": 50,
    "rrf_k": null,
    "notes": ["semantic search disabled in config; used lexical ranking"],
    "results": [
      {
        "rank": 1,
        "message_id": "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
        "final_score": 12.4,
        "lexical_rank": 1,
        "lexical_score": 12.4,
        "dense_rank": null,
        "dense_score": null
      }
    ]
  }
}
```

| Field | Type | Notes |
|---|---|---|
| `requested_mode` | string | `lexical`, `hybrid`, or `semantic`, as asked for by `--mode`. |
| `executed_mode` | string | What actually ran. It falls back to `lexical` when semantic retrieval is unavailable. |
| `semantic_query` | string or null | Semantic text extracted from the query. The lexical fallback paths set it too, so a value here does not prove a dense pass ran. |
| `lexical_window` | number | Candidate window for the lexical pass. Equal to `--limit` for a lexical request, wider for a hybrid or semantic one, including requests that fall back to lexical. |
| `dense_window` | number or null | Candidate window for the dense pass. Null when no dense pass ran. |
| `lexical_candidates` | number | Rows the lexical pass produced. |
| `dense_candidates` | number | Dense rows left after filtering. 0 when no dense pass ran. |
| `final_results` | number | Rows returned on this page, recounted after post-search filters such as disabled accounts. |
| `rrf_k` | number or null | Reciprocal rank fusion constant. Set only when hybrid fusion ran. |
| `notes` | string[] | Reasons for fallbacks and other diagnostics. |
| `results[]` | object[] | One entry per row on the page, with `rank`, `message_id`, `final_score`, and the nullable `lexical_rank`, `lexical_score`, `dense_rank`, `dense_score`. `--format table` and `--format ids` print only the first five; JSON carries all of them. |

### `--format jsonl`

`--format jsonl` writes the same result records to stdout, one per line, and writes the `paging` and `explain` envelope to stderr as a single line. Capture stderr if you need to page.

```bash
mxr search 'from:sarah' --format jsonl 2>/dev/null | jq -r '.subject'
mxr search 'from:sarah' --format jsonl 2>paging.json >/dev/null && jq '.paging.next_offset' paging.json
```

```json
{"message_id":"019706f4-9b6e-7c31-8a3f-2a1c4de50b91","from":"Sarah Chen <sarah@example.com>","subject":"1:1 prep, Friday","date":"2026-04-30T15:42:11+00:00","read":false,"starred":true,"score":1777563776.0}
```

The stderr line has the same shape in `--format csv`.

### `mxr search --group-by`

`--group-by from|list|category` runs an aggregation instead of a message search, so the payload is different. `--format json` returns one object:

```json
{
  "query": "invoice",
  "group_by": "from",
  "total": 12,
  "groups": [
    {
      "key": "billing@example.com",
      "label": "Example Billing <billing@example.com>",
      "count": 8,
      "unread": 2,
      "oldest": 1704067200,
      "newest": 1777564800
    }
  ]
}
```

`--format jsonl` flattens each group onto its own line and repeats `query` and `group_by` on every line. `key` is the normalized grouping value and `label` is the display form. `oldest` and `newest` are Unix seconds and are typed as nullable. `total` counts matching messages, not groups, and `--group-by category` can file one message under several categories, so the group counts can add up to more than `total`.

Groups are sorted by `count` descending, then `unread` descending, then `newest` descending, then `label`. `--limit` truncates that sorted list. Aggregations have no `paging` and no `explain`, and `--offset` is not sent.

`--mode` applies to aggregations too, and it decides how much of the mailbox they see. A lexical aggregation groups every match, so `total` and the groups describe the same messages.

A hybrid or semantic aggregation runs the query twice. The first run decides how many top results the second run will group. The second run searches a candidate pool that can be wider, and `total` reports how many candidates it ranked. The groups cover only the top results the first run allowed, so `total` can be larger than the number of grouped messages. Neither number counts every match in the mailbox, so use `--mode lexical` when you need exact counts.

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

`mxr search --format ids` writes paging state to stderr as `#` comment lines, so stdout stays clean for the pipe. Both lines are conditional: the first is skipped only when the page holds every match and `--offset` is 0, and the second appears only when a next page exists.

```
# search page: returned=50 total=73 offset=0
# more results: rerun with --offset 50
```

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
| `replied_count` | number | Stable JSON field, currently `0` for `subscriptions`; reply-pair counts power sender/contact analytics, not this ranker. |

If `opened_count == message_count`, every message in that sender bucket is read
locally. That can come from the `mxr read` command, another mail client,
provider-side read state, filters, or bulk mark-read actions.

## Mutation dry-run

`--dry-run` on a core mail mutation returns one preview object:

```json
{
  "action": "archive",
  "dry_run": true,
  "requested": 2,
  "selected_messages": 2,
  "selected_threads": 1,
  "message_ids": [
    "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
    "019706f5-1d02-7a48-b0c7-6e5f9a2d4413"
  ],
  "messages": [
    {
      "message_id": "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
      "from": "Alice",
      "subject": "Quarterly review"
    }
  ]
}
```

`requested` and `selected_messages` both hold the resolved message count.
`selected_threads` counts the distinct threads those messages belong to, and
falls back to the message count when the envelopes could not be resolved.
`messages[].from` is the sender's display name, falling back to their address
when the message carries no display name. The full `Name <address>` form never
appears here, so match on `message_id` instead of parsing `from`. `subject`
reads `(no subject)` when the subject is empty. Both fields are empty strings
for a message whose envelope did not resolve.
`messages[].unsubscribe_method` appears on `mxr unsubscribe --dry-run` preview
records, for every message whose envelope resolved. The value is `OneClick`,
`HttpLink`, `Mailto`, `BodyLink`, or `None`.

`--format jsonl` emits one preview line per message, without the counts:

```json
{"action":"archive","dry_run":true,"message_id":"019706f4-9b6e-7c31-8a3f-2a1c4de50b91","from":"Alice","subject":"Quarterly review"}
```

## Mutation result

Archive, trash, spam, read, star, label, and move report the selection at the
top level and the per-account outcome under a nested `result`:

```json
{
  "action": "archive",
  "dry_run": false,
  "selected_messages": 2,
  "selected_threads": 1,
  "message_ids": [
    "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
    "019706f5-1d02-7a48-b0c7-6e5f9a2d4413"
  ],
  "result": {
    "requested": 2,
    "succeeded": 2,
    "skipped": 0,
    "failed": 0,
    "accounts": [
      {
        "account_id": "0196b21c-4e80-7f0a-9c3d-71b8ee402f55",
        "account_name": "work",
        "succeeded": 2,
        "skipped": 0,
        "failed": 0,
        "error": null
      }
    ],
    "mutation_id": "019706f7-3a55-7b02-8e11-5d0c9f6a2b34"
  }
}
```

`result.mutation_id` is set by the undoable mutations (archive, trash, spam,
read, and read-and-archive) and is what you pass to `mxr undo`. Star, label,
and move return no `mutation_id`. When an undoable mutation lands but its undo
entry could not be written, `mutation_id` is absent and
`"undo_unavailable": true` takes its place.

Mutations the daemon only acknowledges emit the same top-level fields with
`"ok": true` instead of `result`. One dispatched as a background job returns
`action`, `dry_run`, `message_ids`, and a `job` object; poll it with
`mxr jobs <job_id>`.

### `mxr snooze`, `mxr unsnooze`, `mxr unsubscribe`

These three walk the selection one message at a time, so they report a flat
batch shape with no nested `result`:

```json
{
  "action": "unsubscribe",
  "dry_run": false,
  "requested": 3,
  "succeeded": 2,
  "failed": 1,
  "selected_messages": 3,
  "selected_threads": 2,
  "message_ids": [
    "019706f4-9b6e-7c31-8a3f-2a1c4de50b91",
    "019706f5-1d02-7a48-b0c7-6e5f9a2d4413",
    "019706f6-2c11-7d63-9b40-8ad3c5e21f07"
  ],
  "errors": [
    {
      "message_id": "019706f6-2c11-7d63-9b40-8ad3c5e21f07",
      "error": "no unsubscribe method"
    }
  ]
}
```

`requested` here is the length of `message_ids`, and `failed` is the length of
`errors`. `selected_messages` and `selected_threads` are omitted when the
command has no selection to report, as with `mxr unsnooze --all`.
`mxr snooze --dry-run` and `mxr unsubscribe --dry-run` still use the preview
shape above.

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
  "account_id": "0196b21c-4e80-7f0a-9c3d-71b8ee402f55",
  "message_id": "019712a3-88d4-70e9-b5a2-4c9f0d61e7aa",
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
  "message_id": "019712a3-88d4-70e9-b5a2-4c9f0d61e7aa",
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
  "message_id": "019712a3-88d4-70e9-b5a2-4c9f0d61e7aa",
  "action": "accept",
  "provider_message_id": "provider-id-or-null",
  "rfc2822_message_id": "<mxr-generated@example.local>"
}
```

## Common `jq` patterns

```bash
# Senders by volume from compact search rows
mxr search 'newer_than:7d' --format json \
  | jq -r '.results
           | group_by(.from)
           | map({sender: .[0].from, count: length})
           | sort_by(-.count) | .[]
           | "\(.count)\t\(.sender)"'

# Subjects from a sender (jsonl stdout is bare records, so no .results here)
mxr search 'from:legal@' --format jsonl 2>/dev/null \
  | jq -r '.subject'

# Check whether more pages remain
mxr search 'has:attachment' --limit 200 --format json \
  | jq '.paging | {total, has_more, next_offset}'

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
