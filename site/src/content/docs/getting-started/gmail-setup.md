---
title: Gmail Setup
description: Connect a Gmail account to mxr.
---

## Why you need your own Google Cloud project

mxr ships with a default OAuth client ID for quick testing, but Google shows a prominent "unverified app" warning screen when using it. This is normal for open-source projects — Google's verification process requires a privacy policy review that doesn't fit the local-first model.

For daily use, we recommend creating your own Google Cloud project. This takes about 5 minutes, removes the warning screen entirely, and means your OAuth credentials are fully under your control. As a developer, you've likely done this before.

## Create your Google Cloud project

1. Open [Google Cloud Console](https://console.cloud.google.com).
2. Create a new project (e.g., "mxr-email").
3. Go to **APIs & Services > Library** and enable the **Gmail API**.
4. Go to **APIs & Services > Credentials** and click **Create Credentials > OAuth client ID**.
5. Select **Desktop app** as the application type.
6. Copy the **Client ID** and **Client Secret**.

## Connect your Gmail account

1. Run:

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
