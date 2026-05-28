---
name: mxr
description: "Use when operating the mxr email client from the CLI: read/search mail, compose/reply/forward, archive/trash/star/label/snooze, manage drafts/accounts/saved searches/rules, inspect daemon status/logs/events, run sync, or use mxr as an agent-facing email API. Pronounced Mixer."
---

# mxr CLI

`mxr` is a daemon-backed, local-first terminal email client. Every action should go through `mxr <subcommand>`.

Write `mxr`; say "Mixer".

## Core rules

1. Prefer structured output: `--format json`, `--format jsonl`, or `--format ids`.
2. Message IDs are UUIDs. Get them with `mxr search "<query>" --format ids`.
3. Batch mutations accept positional IDs, stdin IDs, or `--search "<query>"`; use `--yes` for non-interactive commits.
4. Dry-run first for mutations, compose flows, rules, reset, and undo.
5. Commands auto-start the daemon; use `mxr restart` only when you need a fresh daemon after local code changes.
6. Compose uses `$EDITOR` unless `--body` or `--body-stdin` is supplied.
7. `mxr reset --hard` and `mxr burn` wipe local runtime state only unless `--including-config` is passed.

## Common commands

```bash
mxr search "is:unread label:inbox" --format json --limit 20
mxr cat <message_id> --format json
mxr thread <message_id> --format json
mxr archive --search "from:noreply older:30d" --dry-run
mxr archive --search "from:noreply older:30d" --yes
mxr compose --to a@example.com --subject "Hi" --body "Hello" --dry-run
mxr reply <message_id> --body "Thanks" --dry-run
mxr sync --status --format json
mxr events --format jsonl
mxr logs --level error --since 1h --format jsonl
mxr doctor --check
```

## Reference

Use [`references/commands.md`](references/commands.md) for the full command and search syntax reference.
