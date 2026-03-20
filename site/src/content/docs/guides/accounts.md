---
title: Accounts
description: Manage Gmail runtime accounts and editable IMAP/SMTP accounts.
---

## Account model

`mxr` distinguishes between:

- runtime accounts: what the daemon is actually running now
- config-backed accounts: editable account definitions, mainly IMAP/SMTP

The TUI Accounts page is built from runtime inventory, not from config file entries alone.

## TUI Accounts page

Open it with:

- `Ctrl-p` then `Open Accounts Page`

Actions:

- `n`: new IMAP/SMTP account
- `Enter`: edit selected account
- `t`: test selected account
- `d`: set default account
- `r`: refresh runtime account inventory

The page can show:

- runtime-only Gmail accounts
- editable IMAP/SMTP accounts
- default-account state
- provider kind and enabled state

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

- sync can run per account
- the daemon tracks account health in status and diagnostics
- changing editable accounts in the TUI triggers daemon reload so the runtime view updates without a restart
