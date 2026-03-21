---
title: Security & Privacy
description: What stays local in mxr, what guardrails exist today, and which safety features are still pending.
---

mxr is local-first by design.

Mail syncs from the provider into SQLite on your machine. Search runs against the local index. The daemon, TUI, CLI, and agent workflows all operate on that local state. There is no hosted mxr relay in the middle.

## What stays local

- SQLite is the canonical store
- Tantivy index is local and rebuildable
- The daemon runs on your machine
- The TUI and CLI talk to the daemon over a local Unix socket

## What still talks to a provider

- Sync
- Send
- Provider-side mutations like archive, trash, labels, and spam
- Browser handoff for HTML or unsubscribe pages when needed

That is the intended boundary. The network is for talking to your provider, not to a hosted mxr service.

## Guardrails that exist today

- `--dry-run` on risky mutation commands
- Interactive confirmation for destructive and batch mutation flows unless `--yes` is set
- Persisted mutation history through `mxr history`
- Event and log views through diagnostics and CLI commands
- Plain-text-first reader mode, with browser escape hatch for original HTML

## Not shipped yet

- First-party MCP server
- Read-only mode for agents
- Draft-only mode for agents
- Account-scoped agent permissions
- Explicit send approval flow
- Config-based blocking of risky commands

Those are real gaps. The current model is "broad CLI with dry-run and history," not "fully permissioned agent mail sandbox."

## Practical advice

- Use `--dry-run` before any batch mutation.
- Use app passwords or provider-specific credentials where your provider recommends them.
- Keep your system keyring clean and scoped to the accounts you use.
- If an agent is involved, prefer workflows that search, read, export, and draft before workflows that mutate.
