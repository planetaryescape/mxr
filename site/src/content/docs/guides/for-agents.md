---
title: For agents
description: How agents use mxr today, what is safe, and what is not shipped yet.
---

mxr already works with agents today through the CLI and the skill file.

That is the current story. It is simple, it is scriptable, and it does not pretend an MCP server exists when it does not.

## Current agent surface

| Surface | Status | Notes |
|---|---|---|
| CLI | shipped | structured output, batch ops, dry runs |
| Skill | shipped | documents the CLI for coding agents |
| Daemon socket | shipped | available for custom clients |
| First-party MCP server | not shipped | still on the roadmap |

## Why the CLI works well

- `--format json` gives structured output
- `--dry-run` previews risky mutations
- `--search` lets one command target a set of messages
- `mxr history` shows persisted mutation history

This is enough for a lot of useful agent work without inventing a new tool surface first.

## Good patterns

### Search first

```bash
mxr search "is:unread" --format json
```

### Preview before changing anything

```bash
mxr archive --search "label:notifications older:30d" --dry-run
```

### Check what happened after

```bash
mxr history --category mutation
```

## Example workflows

### Inbox triage

1. Search unread mail.
2. Read selected messages with `mxr cat`.
3. Draft replies or export threads.
4. Use `--dry-run` for any batch mutation.

### Meeting prep

1. Search by sender and date range.
2. Export the relevant thread as markdown.
3. Summarize open items outside mxr.

### CI cleanup

1. Search build notifications.
2. Group by thread or sender.
3. Preview archive/trash actions with `--dry-run`.
4. Apply mutations and verify with `mxr history`.

## Safe defaults

- Prefer `--dry-run` on batch changes.
- Use `--yes` only when the workflow is already known and reviewed.
- Treat `trash`, `spam`, and `unsubscribe` as high-friction commands.
- Keep in mind that agent-safe permission presets are not shipped yet.

## Current limits

- No first-party MCP server yet
- No read-only or draft-only agent mode yet
- No agent-specific account scoping yet
- No explicit send-approval flow yet

If you need those controls right now, treat mxr as a CLI tool that an agent can use carefully, not as a permissioned agent platform.
