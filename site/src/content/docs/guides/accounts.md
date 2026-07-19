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

## Owned addresses and aliases

An account owns a set of addresses: its primary, plus any aliases you
register. mxr uses that set to classify mail as inbound vs outbound, and to
let you send from any of them.

```bash
mxr accounts addresses list work
mxr accounts addresses add work accounts@planetaryescape.xyz
mxr accounts addresses add work support@planetaryescape.xyz
mxr accounts addresses remove work support@planetaryescape.xyz
```

Once an address is registered it becomes a valid sender for that account. A
reply defaults its From to whichever owned address the original was delivered
to, and `--from` / the `from:` frontmatter field can send as any owned address.
See [Sending from an alias](/guides/compose/#sending-from-an-alias-per-message-from).
Only the account's own registered addresses are accepted; an unowned From is
rejected before the message is sent.

## Selecting an account

```bash
mxr accounts --format table
mxr search "is:unread" --account work --format json
mxr archive --account work --search "from:noreply older_than:30d" --dry-run
```

What you get: the account selectors available locally, account-limited
search results, and a preview of the matching work-account messages
before any mutation runs.

Most mail-facing CLI commands accept `--account <selector>`.

The selector can be:

- account key
- email address
- account id
- display name, when it matches exactly one account

Use the same selector on reads, lists, replies, and drafts:

```bash
mxr search "is:unread" --account work --format json
mxr count "from:github.com" --account you@example.com
mxr archive --account work --search "from:noreply older_than:30d" --dry-run
mxr reply MESSAGE_ID --account work --body "Thanks, will do." --dry-run
mxr drafts --account personal --format json
```

If you omit `--account`, mxr keeps the command's normal behavior. Search,
counts, lists, reads, and batch mutations operate across all enabled
accounts unless the command is already inherently tied to one account
(for example `mxr sync --account`, `mxr accounts show`, or compose sender
selection).

Unknown or ambiguous selectors fail before the command is sent to the
daemon. Direct-ID commands also check that the message, draft, delivery,
or invite belongs to the selected account before reading or mutating it.

List the selectors currently available:

```bash
mxr accounts --format json
```

## Compose behavior

Compose, reply, and forward resolve the sender from the selected/default runtime account. That is the same source the TUI Accounts page shows. To send from a specific [owned address](#owned-addresses-and-aliases) rather than the account primary, use `--from <address>` or edit the `from:` frontmatter field — see [Sending from an alias](/guides/compose/#sending-from-an-alias-per-message-from).

## Multi-account notes

- Sync can run per account.
- Mail reads, searches, saved searches, reply queues, deliveries, invites, drafts, and core mutations can run per account with `--account`.
- The daemon tracks account health in status and diagnostics.
- Changing editable accounts in the TUI triggers daemon reload so the runtime view updates without a restart.
- Runtime-only accounts are inspectable in the TUI even when they are not editable there.
