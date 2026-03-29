---
title: Accounts
description: Manage runtime accounts and config-backed account definitions across Gmail, IMAP, and SMTP.
---

## Account model

mxr distinguishes between:

- Runtime accounts: what the daemon is actually running now
- Config-backed accounts: editable account definitions used to produce runtime accounts

The TUI Accounts page is built from runtime inventory, not from config file entries alone.

## TUI accounts page

Open it with:

- `4`
- `Ctrl-p` then `Open Accounts Page`

Actions:

- `j` / `k`: move account selection
- `n`: new IMAP/SMTP account
- `Enter` / `o`: edit selected account
- `t`: test selected account
- `d`: set default account
- `c`: edit config
- `r`: refresh runtime account inventory

The page shows:

- Details on the left
- Account list on the right
- Runtime Gmail, IMAP, and SMTP-backed accounts
- Editable config-backed accounts where supported
- Default-account state
- Provider kind and enabled state
- Last test/status messaging

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
- Runtime-only accounts are inspectable in the TUI even when they are not editable there.
