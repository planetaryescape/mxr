---
title: Accounts
description: Manage Gmail runtime accounts and editable IMAP/SMTP accounts.
---

## Account model

mxr distinguishes between:

- Runtime accounts: what the daemon is actually running now
- Config-backed accounts: editable account definitions, mainly IMAP/SMTP

The TUI Accounts page is built from runtime inventory, not from config file entries alone.

## TUI accounts page

Open it with:

- `Ctrl-p` then `Open Accounts Page`

Actions:

- `n`: new IMAP/SMTP account
- `Enter`: edit selected account
- `t`: test selected account
- `d`: set default account
- `r`: refresh runtime account inventory

The page shows:

- Runtime-only Gmail accounts
- Editable IMAP/SMTP accounts
- Default-account state
- Provider kind and enabled state

## CLI account actions

```bash
mxr accounts
mxr accounts add gmail
mxr accounts add imap
mxr accounts add smtp
mxr accounts show ACCOUNT
mxr accounts test ACCOUNT
```

## Compose behavior

Compose, reply, and forward resolve the sender from the selected/default runtime account. That is the same source the TUI Accounts page shows.

## Multi-account notes

- Sync can run per account.
- The daemon tracks account health in status and diagnostics.
- Changing editable accounts in the TUI triggers daemon reload so the runtime view updates without a restart.
