---
title: Gmail Setup
description: Connect a Gmail account to mxr.
---

## Prerequisites

You need a Google Cloud project with the Gmail API enabled and OAuth desktop credentials created.

## Steps

1. Open [Google Cloud Console](https://console.cloud.google.com).
2. Create or select a project.
3. Enable the Gmail API.
4. Create OAuth 2.0 desktop credentials.
5. Run:

```bash
mxr accounts add gmail
```

6. Enter the client id and secret when prompted.
7. Complete browser authorization.
8. Verify the runtime account exists:

```bash
mxr status
```

9. Start syncing:

```bash
mxr sync
```

## TUI account view

The TUI Accounts page shows runtime accounts, not just config-file accounts. That means a Gmail account added through browser auth should appear there even if it is not an editable IMAP/SMTP config entry.

Open it with:

- `Ctrl-p` then `Open Accounts Page`

## Notes

- Gmail runtime accounts may be runtime-only.
- IMAP/SMTP accounts are editable through the Accounts page and config-backed.
- Compose resolves the sender from the active/default runtime account, not from a static status string.
