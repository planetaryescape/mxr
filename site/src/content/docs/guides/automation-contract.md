---
title: Automation contract
description: What's safe to script. Which commands return JSON, which support --dry-run, which accept piped IDs from stdin. The machine-readable surface, plainly stated.
---

mxr is built to be scripted, but not _every_ command supports _every_ automation primitive. This page is the contract — the things you can rely on when piping mxr into a shell pipeline or an LLM agent.

When in doubt, the [auto-generated CLI reference](/reference/cli/) is the source of truth. The tables below summarise the patterns.

## The four primitives

1. **`--format <FORMAT>`** — `table` (default) for humans, `json|jsonl|csv|ids` for machines. The generated CLI reference lists the exact values per command.
2. **`--dry-run`** — preview affected ids/labels/threads without mutating provider state. Implemented by core mail mutations and selected lifecycle commands.
3. **`--yes`** — skip confirmation prompts on commands that ask before mutating. Required when stdin is not a TTY.
4. **stdin IDs** — pass message IDs on stdin, one per line. Equivalent to listing them as positional args. Available on most mutations; not all.

## Reads (commands that return data)

These are the common automation-oriented read surfaces. Exact formats live in the generated [CLI reference](/reference/cli/). JSON shapes per command live in [JSON output schemas](/reference/json-output/).

| Command | Returns | Pipeable formats |
|---|---|---|
| `mxr search` | envelopes (matching messages) | json, jsonl, csv, ids, table |
| `mxr count` | scalar count | json, jsonl, table/text |
| `mxr cat` | full message body | json, jsonl, table (and the `--view` modes for body rendering) |
| `mxr thread` | thread + messages | json, jsonl, table |
| `mxr headers` | RFC 822 headers | json, jsonl, table |
| `mxr labels` | labels with counts | json, jsonl, csv, ids, table |
| `mxr saved list` / `mxr saved run <name>` | saved searches / matches | json, jsonl, csv, ids, table |
| `mxr drafts list` | drafts | json, jsonl, csv, ids, table |
| `mxr replies list` | reply-later queue | json, jsonl, table |
| `mxr snippets list` | snippets | json, jsonl, table |
| `mxr storage` / `mxr stale` / `mxr response-time` / `mxr contacts` / `mxr subscriptions` / `mxr wrapped` | analytics summaries | json, jsonl, csv, table |
| `mxr status` | daemon health | json, jsonl, table |
| `mxr sync --status` | sync state per account | json, jsonl, table |
| `mxr events` / `mxr history` / `mxr logs` | streaming event/log records | json, jsonl, csv, table |
| `mxr notify` | unread summary | json, jsonl, text |
| `mxr accounts` | runtime account inventory | json, jsonl, csv, ids, table |
| `mxr config show` | resolved config | json, jsonl, csv, ids, table |
| `mxr config get` | one config value | text |
| `mxr attachments list` | attachments for a message | table/text |
| `mxr export` | thread export | markdown, json, mbox, llm |

## Mutations (destructive or stateful)

Core mail mutations accept either explicit message IDs as positional args, `--search QUERY` for batch ops, or piped IDs on stdin. Use the generated CLI reference for non-mail lifecycle commands.

| Command | Targets | `--dry-run` | `--search` | stdin IDs |
|---|---|---|---|---|
| `mxr archive` | message(s) | ✓ | ✓ | ✓ |
| `mxr read-archive` | message(s) | ✓ | ✓ | ✓ |
| `mxr trash` | message(s) | ✓ | ✓ | ✓ |
| `mxr spam` | message(s) | ✓ | ✓ | ✓ |
| `mxr star` / `mxr unstar` | message(s) | ✓ | ✓ | ✓ |
| `mxr read` / `mxr unread` | message(s) | ✓ | ✓ | ✓ |
| `mxr label NAME` / `mxr unlabel NAME` | message(s) | ✓ | ✓ | ✓ |
| `mxr move LABEL` | message(s) | ✓ | ✓ | ✓ |
| `mxr snooze` | message(s) | ✓ | ✓ | ✓ |
| `mxr unsnooze` | message(s) or `--all` | ✓ | — | ✓ |
| `mxr unsubscribe` | message(s) | ✓ | ✓ | ✓ |
| `mxr undo MUTATION_ID` | one mutation | ✓ | — | — |
| `mxr send DRAFT_ID` | a draft | ✓ (`--at` conflicts) | — | — |
| `mxr unsend DRAFT_ID` | a scheduled send | — | — | — |
| `mxr drafts discard` | draft(s) | — | — | — |
| `mxr rules dry-run` | a rule | n/a (always dry-run) | — | — |

## What's _not_ scriptable

- **`mxr` (no args)** — launches the TUI. There is no `--format json` for "the TUI."
- **`mxr daemon`** — is a long-running process; structured output is on `mxr status` / `mxr events` / `mxr logs`.
- **`mxr compose` / `mxr reply` / `mxr reply-all` / `mxr forward`** — open `$EDITOR` by default. For scripts, use `--body`, `--body-stdin`, `--yes`, and `--dry-run`, or use the [HTTP bridge's compose endpoints](/reference/bridge/).
- **`mxr setup`** — interactive first-run account setup. `mxr setup --demo` is legacy; use `mxr demo` for an isolated fake-provider profile.
- **`mxr accounts add`** — interactive _wizard_ by default, but goes non-interactive when you pass enough flags AND set `MXR_IMAP_PASSWORD` / `MXR_SMTP_PASSWORD` / `MXR_GMAIL_CLIENT_SECRET` env vars.

## Safe agent loop

For agents driving mutations, follow this pattern:

```text
1. SEARCH      — mxr search '<query>' --format json
2. CONFIRM     — surface the candidates to the user
3. DRY-RUN     — mxr <verb> --search '<query>' --dry-run
4. APPROVE     — user signs off on the diff
5. MUTATE      — mxr <verb> --search '<query>' --yes
6. RECORD      — capture the printed mutation_id; offer mxr undo within ~60s
7. VERIFY      — mxr history --category mutation --limit 1 --format json
```

The loop is the same whether the agent is `claude`, `cursor`, `aider`, or a hand-rolled curl-and-jq script. The contract above guarantees every step is composable.

## Idempotency

- `mxr archive` / `read-archive` / `trash` / `spam` / `star` / `unstar` / `read` / `unread` / `label` / `unlabel` / `move` / `snooze` / `unsnooze` are **idempotent** — re-running with the same target IDs leaves state unchanged after the first call.
- `mxr send DRAFT_ID` is **not idempotent** — calling twice will send twice (the daemon schedules the second send). Always check `mxr drafts list` first.
- `mxr unsubscribe` may hit a provider URL once; re-running on an already-unsubscribed message is harmless but emits no useful new state.
- `mxr undo MUTATION_ID` works within a 60-second window; after that it returns an error.

## See also

- [CLI reference](/reference/cli/) — every command, every flag, auto-generated
- [JSON output schemas](/reference/json-output/) — field names you can pipe into `jq`
- [Recipes](/guides/recipes/) — copy-pasteable pipelines using these primitives
- [For agents](/guides/for-agents/) — patterns and safe defaults when an LLM is driving
