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
```

## See also

- [CLI overview](/reference/cli/) — every command and its accepted output formats
- [Automation contract](/guides/automation-contract/) — stdin IDs, dry-run, and confirmation rules
- [Recipes](/guides/recipes/) — pipelines using these shapes
- [HTTP bridge](/reference/bridge/) — HTTP routes and daemon payloads
