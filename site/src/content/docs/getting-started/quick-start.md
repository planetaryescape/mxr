---
title: Quick start
description: Try mxr safely, then connect your own inbox.
---

After [installing](/getting-started/install/), use `mxr` to inspect synthetic mail on this computer. Then choose the CLI, web app, TUI, or an agent; they use the same local daemon.

## 1. Try the demo inbox

Start with a realistic local demo before connecting your own mail:

```bash
mxr demo
```

This creates a separate demo config, database, socket, and daemon. It seeds a two-account synthetic inbox. The demo profile is separate from your real profile and uses a canned model provider for model-assisted demo results. Those results demonstrate the workflow, not model quality.

`mxr demo` reports its progress and opens the demo TUI when seeding finishes. To leave the terminal available for another client, run `mxr demo --no-tui`. The demo supports a smaller seed with `mxr demo --messages 5000`.

**Demo mode is sticky.** Subsequent `mxr` commands target the demo profile until you stop it. The TUI and web app show a `DEMO` marker. Check the selected profile with `mxr demo status`. Exit with:

```bash
mxr demo stop
```

Other demo subcommands:

```bash
mxr demo status        # is demo active? where are its files?
mxr demo reset         # wipe demo data so the next `mxr demo` re-seeds from scratch
mxr demo --reset       # equivalent: reset + restart in one step
mxr demo --no-tui      # seed + sync without launching the TUI
```

Query local synthetic mail before choosing a client:

```bash
mxr search "is:unread" --format json --limit 10
```

The result is local mailbox data. For the five mail modes and Now, continue to [Work through your day](/guides/your-day/).

:::caution[Stop demo mode before connecting your mail]
Demo mode remains active for later `mxr` commands until you run `mxr demo stop`. Stop it before adding or connecting a real account.
:::

## 2. Add your account

For Gmail on your local machine:

```bash
mxr accounts add gmail
```

The wizard prompts for an account name and your Gmail address, then opens a browser to authorize. If you're SSH'd into a remote box, see the [Gmail setup notes on SSH-friendly flows](/getting-started/gmail-setup/#working-over-ssh-or-in-a-container).

For Outlook.com, Hotmail, or Live:

```bash
mxr accounts add outlook
```

For a Microsoft 365 work or school account:

```bash
mxr accounts add outlook-work
```

For IMAP+SMTP non-interactively (e.g. Gmail with an app password):

```bash
MXR_IMAP_PASSWORD="$APP_PASSWORD" MXR_SMTP_PASSWORD="$APP_PASSWORD" \
  mxr accounts add imap-smtp \
    --account-name personal \
    --email you@gmail.com \
    --imap-host imap.gmail.com \
    --imap-username you@gmail.com \
    --smtp-host smtp.gmail.com \
    --smtp-username you@gmail.com
```

`MXR_IMAP_PASSWORD` / `MXR_SMTP_PASSWORD` env vars resolve when stdin is not a TTY, which is handy for CI. You can also pass `--imap-password` / `--smtp-password` literal values if you don't mind shell history.

## 3. Sync

`mxr sync` starts a sync in the daemon and returns as soon as it has started. The account keeps syncing afterwards, so you can start reading mail right away. Add `--wait` to stay until that account reports idle:

```bash
mxr sync                                  # trigger, return immediately
mxr sync --wait                           # trigger, then wait for idle
mxr sync --wait --wait-timeout-secs 900   # a first backfill needs a longer wait
```

What you get: with `--wait`, a live progress line on stderr each time the count moves, then exit 0 once the account is idle.

```text
personal: 3,000/50,000 — Stored 3000 messages
```

The denominator appears only when the provider can say how much is left. Gmail and IMAP cannot count the remainder without paying for it, so there you get a bare running count.

:::caution[`--wait-timeout-secs` bounds the wait, not the sync]
It defaults to 60 seconds. When it expires, `mxr sync` exits non-zero with `timed out after 60s waiting for sync to quiesce` and the sync carries on in the daemon. Give a first backfill a generous value.
:::

`--wait` also exits non-zero if the sync it started failed. An error the account was already carrying before the trigger does not count.

Check on it any time without waiting:

```bash
mxr sync --status
mxr sync --status --format json | jq '.[] | {account_name, sync_in_progress, progress}'
```

Subsequent syncs are incremental and run automatically in the background once the daemon is up.

## 4. Open the TUI

```bash
mxr
```

`j`/`k` to navigate, `<Enter>` to open, `R` for reader mode, `Ctrl-p` for the command palette, `/` for search, `?` for help.

## 5. Or open the web app

```bash
mxr web
```

The web app opens on [Now](/guides/now/): the few things that need you now,
from every mode, at most ten. The TUI opens on the same Now.
[Work through your day](/guides/your-day/) walks through it.

## 6. Or do it from the CLI

```bash
# What needs you
mxr desk

# Search
mxr search "from:alice@example.com is:unread" --format json | jq .

# Read the first match
mxr cat <message-id>

# Reply and send
mxr reply <message-id> --body "On it." --yes

# Archive a query
mxr archive --search "label:newsletters older_than:30d" --dry-run
mxr archive --search "label:newsletters older_than:30d" --yes
```

Most read/list/status/mutation surfaces accept `--format json`; the generated [CLI reference](/reference/cli/) lists the exact formats per command.

## What's next

- [Work through your day](/guides/your-day/): Now, Messages, To do, Updates, Reading and Archive.
- [Configure rules](/guides/rules/) for declarative filing.
- [Write a saved search](/guides/search/) for your daily inbox lens.
- [Hand mxr to an LLM](/guides/agent-skill/): the same CLI is the agent surface.
- [Use mxr with an existing agent](/getting-started/agent-quick-start/): connect local Codex with MCP or use the CLI skill.

Run into something? See [Troubleshooting](/troubleshooting/).
