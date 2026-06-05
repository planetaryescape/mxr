---
title: IMAP / SMTP setup
description: Connect any email provider to mxr using IMAP for sync and SMTP for sending.
---

IMAP and SMTP are first-party adapters in mxr, shipped alongside Gmail. Any email provider that supports IMAP and SMTP works: Fastmail, ProtonMail (with Bridge), Outlook, Yahoo, your company's Exchange server, self-hosted Dovecot, anything.

They sync into the same local runtime and IPC surface as Gmail accounts. The daemon still speaks the internal mxr model; IMAP folder semantics are handled in the adapter.

For Gmail-over-IMAP, mxr detects Gmail's All Mail folder (`\\All`, `[Gmail]/All Mail`, or `[Google Mail]/All Mail`) and uses it as the canonical sync source when available. Archived Gmail messages are just messages without `INBOX`; syncing All Mail prevents archived-only mail from disappearing from local search. Non-Gmail IMAP servers continue to sync normal folders and map archive/move operations to provider folder operations.

## Add an IMAP/SMTP account

The shortest path is `mxr accounts add`. It writes the config entry, stores the password in your OS keychain, and runs an authentication round-trip before saving anything:

```bash
# Interactive — prompts for host, port, username, password
mxr accounts add imap
mxr accounts add smtp

# Combined IMAP + SMTP — one call, both sides
MXR_IMAP_PASSWORD="$WORK_IMAP_PW" MXR_SMTP_PASSWORD="$WORK_SMTP_PW" \
  mxr accounts add imap-smtp \
    --account-name work \
    --email you@example.com \
    --imap-host imap.fastmail.com --imap-username you@example.com \
    --smtp-host smtp.fastmail.com --smtp-username you@example.com

# Or split — useful when sync and send live on different servers
MXR_IMAP_PASSWORD="$WORK_IMAP_PW" mxr accounts add imap \
  --account-name work \
  --imap-host imap.fastmail.com \
  --imap-username you@example.com

MXR_SMTP_PASSWORD="$WORK_SMTP_PW" mxr accounts add smtp \
  --account-name work \
  --smtp-host smtp.fastmail.com \
  --smtp-username you@example.com
```

Passwords resolve in this order: the `--imap-password` / `--smtp-password` flag (if present and stdin is not a TTY), then the `MXR_IMAP_PASSWORD` / `MXR_SMTP_PASSWORD` environment variables, then an interactive prompt. Pass the env-var form when scripting to keep secrets out of shell history. The credential is written to the OS keychain (Keychain on macOS, Secret Service on Linux) — never to `config.toml`.

If a password ever goes stale (provider rotated it, you regenerated an app password), re-run `mxr accounts repair work` to overwrite the keychain entry without touching the rest of the account config.

### Manual TOML (escape hatch)

You can write the config by hand if you prefer. `mxr config path` shows the file location. The shape is:

```toml
[accounts.work]
name = "work"
email = "you@example.com"

[accounts.work.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "you@example.com"
password_ref = "mxr-work-imap"
use_tls = true

[accounts.work.send]
type = "smtp"
host = "smtp.example.com"
port = 587
username = "you@example.com"
password_ref = "mxr-work-smtp"
use_tls = true
```

`password_ref` is the **service name** the daemon will query in the OS keychain (paired with the `username` as the account). Store the password yourself with:

```bash
# macOS
security add-generic-password -a "you@example.com" -s "mxr-work-imap" -w

# Linux (using secret-tool)
secret-tool store --label="mxr-work-imap" service "mxr-work-imap" account "you@example.com"
```

The `mxr accounts add` flow above is the supported path; the manual TOML shape is documented for advanced users only.

## Common provider settings

| Provider | IMAP Host | IMAP Port | SMTP Host | SMTP Port |
|---|---|---|---|---|
| Fastmail | imap.fastmail.com | 993 | smtp.fastmail.com | 587 |
| Migadu | imap.migadu.com | 993 | smtp.migadu.com | 587 |
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

Add as many accounts as you need — each `mxr accounts add` invocation appends a new entry to the config:

```bash
MXR_IMAP_PASSWORD="$PERSONAL_IMAP_PW" mxr accounts add imap \
  --account-name personal \
  --imap-host imap.fastmail.com \
  --imap-username me@fastmail.com
MXR_SMTP_PASSWORD="$PERSONAL_SMTP_PW" mxr accounts add smtp \
  --account-name personal \
  --smtp-host smtp.fastmail.com \
  --smtp-username me@fastmail.com
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

Use the app password when `mxr accounts add` prompts for one — never your login password.
