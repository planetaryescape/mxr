---
title: Why mxr
description: Compare an agent connector with a local mail runtime for synced history.
---

mxr is a local mail runtime for you and your agents. A Gmail connector gives
an agent access to Gmail; mxr syncs mail from connected accounts into a
local mailbox that you, your scripts, and your agents can use through common
mail operations. A person and an agent using the same local daemon work with the
same mailbox state, while client paths can expose different operations and
authority. A client built on the daemon can reuse its existing sync, search,
mail operations, and applicable policy checks instead of reimplementing
those paths.

The provider remains part of the system: mxr syncs new mail and sends or
applies provider-backed changes through it. Synced messages and locally
cached content can be read from this machine; sync, sending, uncached
attachments, remote articles, and configured model endpoints may use the
network. Initial sync can take time. The demo uses isolated synthetic mail.

After [installing mxr](/getting-started/install/), try it with synthetic mail:

```bash
mxr demo
mxr subscriptions --rank --format json
```

The result is a limited candidate set. By default the query returns up to
200 candidates; `--rank` sorts those results using the current READ flags.
`opened_count` is the number currently marked READ, not a count of tracked
opens.

## mxr keeps your mail local and works it by what you do with it

mxr is a local mail runtime. It syncs provider mail into SQLite, indexes
synced messages locally, and exposes daemon-backed state through human and
machine interfaces. Local search and cached-mail reads do not need a
provider request; sync, sending, provider-backed changes, uncached
attachments, remote articles, and configured model endpoints can still use
the network.

On top of that store, mxr treats email as five jobs: conversations with
people, things to do, updates to glance at, reading you chose, and records
to keep. Each job has its own view: Messages, To do, Updates, Reading and Archive.
Now brings together the work that needs you. Inspect placement with
`mxr why MESSAGE_ID`; [Email is five apps at once](/guides/email-modes/)
explains membership, sorting and personal categories.

Choose mxr when you want:

- synced mail in SQLite on your machine
- Gmail, Outlook and IMAP receive paths, plus Gmail, Outlook or SMTP send paths
- Gmail-style local search over stored mail
- a desk of what needs you, with newsletters and notifications kept off it
- AI features that are off by default, run against a model you choose, and
  default to a local endpoint
- dry-runnable batch mutations
- structured CLI output for shell scripts and agents
- a TUI, browser UI, and HTTP bridge on the same daemon contract

Check the surfaces in your checkout:

```bash
mxr --help
mxr status --format json
mxr web --print-url
```

What you get: the command surface, daemon health, and active local web
URL.

## Non-goals

mxr is not a hosted connector layer, managed auth service, remote
workflow platform, or backend email API for a server product. It is also
not a standalone desktop app; the GUI surface is the web app, opened with
`mxr web` or in the thin [desktop window](/guides/desktop-app/) around it,
with the modes also in [Raycast](/guides/raycast/).

Use something else when:

- You want a hosted connector layer more than a local mail runtime.
- You mainly want a terminal UI and do not care about a broad CLI or daemon.
- You need a production backend email API, not a local user-facing workflow tool.
- You need a hosted MCP endpoint or managed OAuth connector. mxr's MCP server is local stdio and uses your local daemon.

## Provider capability matrix

| Adapter | Sync | Send | Labels / folders | Mutations | Notes |
|---|---|---|---|---|---|
| Gmail | yes | yes | labels | yes | Direct Gmail API adapter. |
| IMAP | yes | no | folders | yes | Usually paired with SMTP. |
| SMTP | no | yes | no | no | Send-only adapter. |
| Outlook Personal | yes | yes | folders/categories | yes | Microsoft consumer accounts. |
| Outlook Work | yes | yes | folders/categories | yes | Microsoft 365 work/school accounts. |
| Fake | yes | yes | fixture labels | yes | Tests and `mxr demo`. |

Verify provider setup:

```bash
mxr accounts --format json
mxr sync --status --format json
```

What you get: configured accounts and the latest sync state per account.

## Interface capability matrix

| Interface | Status | Best for | Notes |
|---|---|---|---|
| CLI | available | scripting, batch work, agents | broadest current shell surface |
| TUI | available | daily reading and triage | mailbox-focused interface |
| Daemon socket | available | custom clients | JSON over Unix socket |
| HTTP bridge | available | local web/custom clients | loopback bearer-token bridge |
| Agent skill | available | coding-agent workflows | documents CLI patterns |
| MCP server | available | MCP-native agents | `mxr mcp serve`, stdio, daemon profile gates |

Start with the demo when you are evaluating fit:

```bash
mxr demo
mxr search "has:attachment" --format json
mxr reset --hard --dry-run
```

What you get: realistic local mail state and safe reset preview without
touching your real profile.

## See also

- [Quick start](/getting-started/quick-start/)
- [Email is five apps at once](/guides/email-modes/)
- [Automation contract](/guides/automation-contract/)
- [For agents](/guides/for-agents/)
- [MCP server](/reference/mcp/)
- [Desktop app](/guides/desktop-app/)
- [Raycast](/guides/raycast/)
- [Architecture](/guides/architecture/)
