---
title: Why mxr
description: Decide whether mxr fits your email workflow.
---

Use mxr when you want email state on your own machine, with one local
runtime behind the TUI, CLI, web app, scripts, and agent workflows.

```bash
mxr demo
mxr search "is:unread" --format json
mxr web
```

What you get: an isolated demo inbox, a machine-readable search result,
and the browser UI served by the local bridge.

## The fit

mxr is a local mail runtime. It syncs provider mail into SQLite,
indexes it locally, and exposes the same daemon-backed state through
human and machine interfaces.

Choose mxr when you want:

- synced mail in SQLite on your machine
- Gmail and IMAP receive paths, plus Gmail or SMTP send paths
- Gmail-style local search over stored mail
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

Use something else when you need:

- a hosted service that manages provider auth for users
- one remote automation endpoint across many SaaS products
- a production backend email API for many users
- a terminal UI only, with no daemon or broad CLI contract
- a first-party MCP server as the integration surface

The documented agent surface is the CLI plus the HTTP bridge:

```bash
mxr search "from:alice newer_than:14d" --format ids
mxr archive --search "from:no-reply older_than:30d" --dry-run
```

What you get: pipeable selectors first, then a preview of any batch
mutation.

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

| Interface | Use it for | Entry point |
|---|---|---|
| CLI | scripting, batch work, agents | `mxr <command>` |
| TUI | daily reading and triage | `mxr` |
| Web app | browser reading, sender views, dashboards | `mxr web` |
| Daemon socket | custom local clients | `mxr daemon` |
| HTTP bridge | browser and HTTP clients | `mxr web --print-url` |
| Agent skill | coding-agent workflows over the CLI | [Agent skill](/guides/agent-skill/) |

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
- [Automation contract](/guides/automation-contract/)
- [For agents](/guides/for-agents/)
- [No native desktop app](/guides/no-native-desktop-app/)
- [Architecture](/guides/architecture/)
