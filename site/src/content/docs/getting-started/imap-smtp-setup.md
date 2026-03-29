---
title: IMAP / SMTP setup
description: Connect any email provider to mxr using IMAP for sync and SMTP for sending.
---

IMAP and SMTP are first-party adapters in mxr, shipped alongside Gmail. Any email provider that supports IMAP and SMTP works: Fastmail, ProtonMail (with Bridge), Outlook, Yahoo, your company's Exchange server, self-hosted Dovecot, anything.

They sync into the same local runtime and IPC surface as Gmail accounts. The daemon still speaks the internal mxr model; IMAP folder semantics are handled in the adapter.

## Add an IMAP/SMTP account

Edit your mxr config file:

```bash
mxr config path  # shows the location
```

Add an account entry:

```toml
[accounts.work]
name = "work"
email = "you@example.com"

[accounts.work.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "you@example.com"
password_ref = "keyring:mxr/work"
use_tls = true

[accounts.work.send]
type = "smtp"
host = "smtp.example.com"
port = 587
username = "you@example.com"
password_ref = "keyring:mxr/work"
use_tls = true
```

## Store your password

mxr reads passwords from the system keyring. Store it with:

```bash
# macOS
security add-generic-password -a "you@example.com" -s "mxr/work" -w

# Linux (using secret-tool)
secret-tool store --label="mxr/work" service mxr account work
```

The `password_ref` in the config uses the format `keyring:<service>` to look up the credential.

## Common provider settings

| Provider | IMAP Host | IMAP Port | SMTP Host | SMTP Port |
|---|---|---|---|---|
| Fastmail | imap.fastmail.com | 993 | smtp.fastmail.com | 587 |
| Outlook / Office 365 | outlook.office365.com | 993 | smtp.office365.com | 587 |
| Yahoo | imap.mail.yahoo.com | 993 | smtp.mail.yahoo.com | 587 |
| ProtonMail (Bridge) | 127.0.0.1 | 1143 | 127.0.0.1 | 1025 |
| Self-hosted (Dovecot) | your-server.com | 993 | your-server.com | 587 |

All use TLS. Port 993 is IMAP over TLS. Port 587 is SMTP with STARTTLS.

## Verify the account

```bash
# Check it appears in status
mxr status

# Test connectivity
mxr accounts test work

# Trigger a sync
mxr sync --account work
```

## Multiple accounts

Add as many accounts as you need. Each gets its own section:

```toml
[accounts.personal]
name = "personal"
email = "me@fastmail.com"

[accounts.personal.sync]
type = "imap"
host = "imap.fastmail.com"
port = 993
username = "me@fastmail.com"
password_ref = "keyring:mxr/personal"
use_tls = true

[accounts.personal.send]
type = "smtp"
host = "smtp.fastmail.com"
port = 587
username = "me@fastmail.com"
password_ref = "keyring:mxr/personal"
use_tls = true
```

You can mix and match: one account on Gmail, another on IMAP/SMTP, a third on something else. They all sync into the same local database and are searchable together.

## TUI account view

Manage accounts from the TUI:

- `Ctrl-p` then `Open Accounts Page`

IMAP/SMTP accounts are fully config-backed and editable through the Accounts page.

## App passwords

Some providers require app-specific passwords instead of your regular password when IMAP access is enabled:

- **Fastmail**: Settings > Privacy & Security > App Passwords
- **Yahoo**: Account Security > Generate app password
- **Outlook with 2FA**: Security > App passwords

Use the app password in your keyring, not your login password.
