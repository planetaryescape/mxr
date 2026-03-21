---
title: Why mxr
description: Where mxr fits, what it is good at, and when something else may be a better fit.
---

mxr is for people who want their email runtime on their own machine.

That puts it in a different spot from a classic terminal client and a different spot from a hosted connector layer.

## The rough map

There are a few overlapping categories here:

- Classic terminal mail clients: mutt, neomutt, aerc
- CLI-first mail tools: himalaya, gog, gws
- Local indexing tools: notmuch
- Hosted connector layers: Nylas CLI, Composio, Zapier MCP, EmailEngine
- Local runtime and bridge experiments: Post, `email-mcp`

mxr borrows ideas from all of them, but the center of gravity is simple: local store, local search, daemon-backed workflow, broad CLI surface.

## What earlier tools are good at

| Tool | Good fit when you want... | Less central there |
|---|---|---|
| mutt / neomutt | a long-established keyboard mail workflow | local daemon + broad JSON CLI |
| aerc | a modern terminal UI | daemon-backed local runtime |
| himalaya | a clean CLI-first mail client | shared daemon/state layer |
| notmuch | local indexing over existing local maildirs | provider sync + mutations under one tool |
| gog / gws | Gmail scripting | provider-agnostic mail workflow |
| mxr | one local runtime for TUI, CLI, scripts, and agents | hosted connector orchestration |

That is not a knock on the older tools. It is the point. mxr exists because those tools proved the value of keyboard mail, local indexing, and scriptable mail.

## Hosted connectors and nearby projects

| Tool | Better fit when you need... | mxr difference |
|---|---|---|
| Nylas CLI | managed provider access with a hosted layer | mxr keeps runtime and state local |
| Composio | cross-app automation with managed auth | mxr stays mail-first and local |
| Zapier MCP | a hosted action layer across many apps | mxr is a local mail runtime, not a SaaS router |
| Gmail MCP servers | Gmail-specific agent access | mxr aims for a broader local mail workflow across providers |
| EmailEngine | a self-hosted email API for backend systems | mxr is aimed at local human + agent workflows |
| Post | a local mail daemon + CLI on macOS | mxr is a Rust codebase with Gmail/IMAP/SMTP focus |
| `email-mcp` | local MCP access to IMAP/SMTP | mxr is broader than the MCP bridge alone |

Hosted connector layers are good fits when you want remote workflows, managed auth, or one endpoint across many SaaS products. mxr is a better fit when you want the mail system itself on your machine.

## Choose mxr if...

- You want synced mail in SQLite on your machine.
- You want one tool that works as TUI, CLI, script target, and agent surface.
- You care about search, batch operations, exports, and local workflows.
- You want provider adapters behind one internal model.

## Choose something else if...

- You want a hosted connector layer more than a local mail runtime.
- You mainly want a terminal UI and do not care about a broad CLI or daemon.
- You need a production backend email API, not a local user-facing workflow tool.
- You need a first-party MCP server right now. mxr has not shipped that yet.

## Provider capability matrix

| Adapter | Sync | Send | Labels / folders | Mutations | Notes |
|---|---|---|---|---|---|
| Gmail | yes | yes | labels | yes | direct Gmail adapter |
| IMAP | yes | no | folders | yes | usually paired with SMTP |
| SMTP | no | yes | no | no | send-only adapter |
| Fake | yes | yes | fixture labels | yes | tests and local development |

## Interface capability matrix

| Interface | Status | Best for | Notes |
|---|---|---|---|
| CLI | available | scripting, batch work, agents | broadest current surface |
| TUI | available | daily reading and triage | mailbox-focused interface |
| Daemon socket | available | custom clients | JSON over Unix socket |
| Agent skill | available | coding-agent workflows | documents CLI patterns |
| MCP server | not shipped | tool-native agent integration | planned, not current |
