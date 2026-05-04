---
title: Quick start
description: From install to first email in five minutes.
---

After [installing](./install/), you have one binary: `mxr`. The TUI is what you'll use most; the CLI handles scripting and one-off operations. Both talk to the same daemon.

## 1. Add an account

For Gmail on your local machine:

```bash
mxr accounts add gmail
```

The wizard prompts for an account name and your Gmail address, then opens a browser to authorize. If you're SSH'd into a remote box, see the [Gmail setup notes on SSH-friendly flows](./gmail-setup#working-over-ssh-or-in-a-container).

For IMAP+SMTP non-interactively (e.g. Gmail with an app password):

```bash
mxr accounts add imap \
  --account-name personal \
  --email you@gmail.com \
  --imap-host imap.gmail.com \
  --imap-username you@gmail.com \
  --imap-password "$APP_PASSWORD" \
  --smtp-host smtp.gmail.com \
  --smtp-username you@gmail.com \
  --smtp-password "$APP_PASSWORD"
```

Passwords also resolve from `MXR_IMAP_PASSWORD` / `MXR_SMTP_PASSWORD` env vars when stdin is not a TTY — handy for CI.

## 2. Sync

```bash
mxr sync --wait
```

`--wait` blocks until the initial sync completes. Subsequent syncs are incremental and run automatically in the background once the daemon is up.

## 3. Open the TUI

```bash
mxr
```

`j`/`k` to navigate, `<Enter>` to open, `R` for reader mode, `:` for the command palette, `/` for search, `?` for help.

## 4. Or do it from the CLI

```bash
# Search
mxr search "from:alice is:unread" --format json | jq .

# Read the first match
mxr cat <message-id>

# Reply and send
mxr reply <message-id> --body "On it." --yes

# Archive a query
mxr archive --search "label:newsletters older_than:30d" --dry-run
mxr archive --search "label:newsletters older_than:30d" --yes
```

Every read/list/status/mutation surface accepts `--format json`; pipe it to `jq` and you have a programmable inbox.

## What's next

- [Configure rules](../guides/rules/) for declarative filing.
- [Write a saved search](../guides/search/) for your daily inbox lens.
- [Hand mxr to an LLM](../guides/agent-skill/) — the same CLI is the agent surface.

Run into something? See [Troubleshooting](../troubleshooting/).
