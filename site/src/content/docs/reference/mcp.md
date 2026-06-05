---
title: MCP server
description: Run mxr as a local Model Context Protocol server.
---

mxr ships a first-party MCP server for agents that support stdio MCP tools.
The server does not talk to Gmail or IMAP directly. Every tool calls the local
mxr daemon over IPC with source `mcp`, so daemon profiles, account allowlists,
send gates, destructive gates, activity origins, and provider adapters stay in
one place.

## Start the server

Configure your MCP client to run:

```bash
mxr mcp serve
```

The command speaks MCP over stdin/stdout. It connects to the active mxr daemon
socket; normal daemon auto-start behavior still applies through other CLI
commands, so run `mxr status` first if you want to verify the runtime.

## Required profile

MCP IPC is denied unless `[agents.profiles.mcp]` exists in `config.toml`:

```toml
[agents.profiles.mcp]
safety_policy = "draft-only"      # read-only | restricted | draft-only | full
allowed_accounts = ["work"]       # account key, email, or account id
allow_send = false
allow_destructive = false
```

Use a narrow profile by default. Set `safety_policy = "full"`,
`allow_send = true`, or `allow_destructive = true` only for a client session
where the human approval loop is explicit.

## Tools

The server exposes stable mxr tools for common agent workflows:

- `mxr_status`
- `mxr_list_messages`
- `mxr_search`
- `mxr_read_message`
- `mxr_read_thread`
- `mxr_draft_assist`
- `mxr_save_draft`
- `mxr_mutation_preview`
- `mxr_mutate`
- `mxr_send_draft`

`mxr_read_message` only includes full body content when `include_body = true`.
`mxr_mutate` requires `confirm = true` and should be called only after
`mxr_mutation_preview`. `mxr_send_draft` requires `confirm = true`; the daemon
can still reject the request if the `mcp` profile disallows sends or the draft
fails send safety checks.

## Activity and audit

MCP requests are recorded with origin `mcp` where activity logging applies.
Activity is local-only and disabled when `MXR_ACTIVITY=off`.

Check recent MCP activity:

```bash
mxr activity list --source mcp --format json
```

## See also

- [For agents](/guides/for-agents/) — workflows and guardrails
- [Config](/reference/config/) — profile and account config
- [Automation contract](/guides/automation-contract/) — dry-run and JSON conventions
