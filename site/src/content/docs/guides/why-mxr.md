---
title: Why mxr
description: Decide whether mxr fits your email workflow.
---

Use mxr when you want your whole mail history on your own machine, with
one local runtime behind the TUI, CLI, web app, scripts and agents, and a
mail client that sorts what needs you from what merely arrived.

```bash
mxr demo
mxr desk
mxr search "is:unread" --format json
mxr web
```

What you get: an isolated demo inbox, the desk's list of replies you owe
and promises due, a machine-readable search result, and the browser UI
served by the local bridge.

## mxr keeps your mail local and works it by what you do with it

mxr is a local mail runtime. It syncs provider mail into SQLite, indexes
it locally, and exposes the same daemon-backed state through human and
machine interfaces. Because the whole history is on your disk, search,
analytics and an agent's questions run against it without a provider
round trip.

On top of that store, mxr treats email as five jobs: conversations with
people, things to do, updates to glance at, reading you chose, and records
to keep. Today the desk, Reading and Paper trail split your inbox along
those lines by rule, and every placement says why. Separate views for each
of the five modes are planned; [Email is five apps at once](/guides/email-modes/)
shows what ships for each one now and what is coming.

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
not a native desktop app; the GUI surface is the web app opened with
`mxr web`.

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
- [No native desktop app](/guides/no-native-desktop-app/)
- [Architecture](/guides/architecture/)
