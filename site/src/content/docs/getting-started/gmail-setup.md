---
title: Gmail setup
description: Connect a Gmail account to mxr.
---

## Working over SSH or in a container?

The OAuth flow opens a browser on the same machine the daemon is running on. If you're SSH'd into a server, that browser opens *on the server*, not your laptop, and the localhost callback won't reach you.

mxr auto-detects this case (no TTY / no `DISPLAY` / `SSH_CONNECTION` set) and switches to the [Limited Input Device flow (RFC 8628)](https://datatracker.ietf.org/doc/html/rfc8628): it prints a code and a `https://www.google.com/device` URL; you open that URL in any browser and paste the code. To see the prompt, run the daemon in the foreground while you add the account:

```bash
# Terminal 1: keep this running
mxr daemon --foreground

# Terminal 2:
mxr accounts add gmail --account-name personal --email you@gmail.com
```

The bundled Gmail OAuth client may be configured as a Desktop-app type, which Google does **not** allow for device flow. If you see `invalid_request: device_id` from Google, drop down to one of:

- **Bring your own credentials** of OAuth client type *TV and Limited Input devices* (see [the create-your-own-credentials section below](#create-your-own-credentials)) — this gives you a stable BYOC client that supports device flow.
- **IMAP + app password** — the simplest SSH-friendly path. `mxr accounts add imap --email you@gmail.com --imap-host imap.gmail.com --imap-username you@gmail.com --imap-password "$APP_PASSWORD" --smtp-host smtp.gmail.com --smtp-port 587 --smtp-username you@gmail.com --smtp-password "$APP_PASSWORD"`. Generate the app password at <https://myaccount.google.com/apppasswords> (requires 2FA).

---

mxr connects to Gmail through the Gmail API using OAuth. You have two options for credentials:

1. **Bundled credentials** — mxr ships with a default OAuth client. You can use it to get started fast, but Google will show a scary "unverified app" warning during authorization because the app hasn't gone through Google's verification process (and likely never will — Google's verification requirements don't fit open-source desktop apps well).

2. **Your own credentials** (recommended) — create your own Google Cloud project. Takes about 5 minutes. No warning screens, full control over your OAuth tokens, and no dependency on mxr's bundled client. Your credentials talk directly to Google with your own project — mxr never sees them.

We strongly recommend option 2. The bundled credentials work, but the unverified warning is confusing, Google may throttle shared client IDs, and your own project gives you full control. The rest of this guide walks you through creating your own credentials and connecting your account.

## Create your Google Cloud project

### 1. Create the project

1. Open [Google Cloud Console](https://console.cloud.google.com).
2. Create a new project (e.g., "mxr-email").

### 2. Enable the Gmail API

:::caution
This step is required. If you skip it, mxr will show a sync error when it tries to fetch your email.
:::

1. In your new project, go to **APIs & Services > Library**.
2. Search for **Gmail API**.
3. Click **Enable**.

### 3. Configure the OAuth consent screen

1. Go to **APIs & Services > OAuth consent screen**.
2. Select **External** user type (unless you have a Google Workspace org).
3. Fill in the required fields (app name, user support email, developer contact).
4. On the **Scopes** page, add: `https://mail.google.com/`
5. On the **Test users** page, add your own Gmail address.
6. Click **Save and Continue** through to the end.

Your app will be in "Testing" mode. This is fine — it means only the test users you added can authorize. You don't need to publish or verify the app for personal use.

### 4. Create OAuth credentials

1. Go to **APIs & Services > Credentials**.
2. Click **Create Credentials > OAuth client ID**.
3. Select **Desktop app** as the application type.
4. Copy the **Client ID** and **Client Secret**. You'll need both in the next step.

## Connect your Gmail account

### CLI setup

```bash
mxr accounts add gmail
```

When prompted:
- Select **custom** for credential source.
- Paste your Client ID and Client Secret.
- Complete browser authorization when it opens.

Verify the account:

```bash
mxr status
mxr sync
```

### TUI setup

1. Press `4` to open the Accounts page.
2. Press `n` to add a new account.
3. Fill in your name and email, select Gmail as the sync and send provider.
4. Select **custom** for credential source and paste your Client ID and Client Secret.
5. Press `a` to authorize (opens browser for OAuth).
6. Press `t` to test the connection.
7. Press `s` to save.

## Using bundled credentials instead

If you want to skip creating your own project and accept the unverified app warning:

```bash
mxr accounts add gmail
# Select "bundled" when prompted for credential source
# Browser opens — click Advanced > Go to mxr (unsafe)
# Complete authorization
```

This works but we recommend switching to your own credentials when you have 5 minutes. To switch later, just re-run `mxr accounts add gmail` with custom credentials — it will replace the existing account.

## Troubleshooting

### "Gmail API has not been used in project X" or "accessNotConfigured"

You need to enable the Gmail API. Go to **APIs & Services > Library** in Google Cloud Console, search for Gmail API, and click **Enable**.

### "Access blocked: This app's request is invalid" or "invalid_client"

Your Client ID or Client Secret is wrong. Double-check you copied them correctly. Make sure you created a **Desktop app** credential, not a Web application.

### "Access blocked: mxr has not completed the Google verification process"

This is the unverified app warning when using bundled credentials. Click **Advanced** at the bottom left, then **Go to mxr (unsafe)**. If you're using your own credentials, you shouldn't see this.

### "This app is blocked" (no Advanced option)

Your OAuth consent screen may not have your email as a test user. Go to **OAuth consent screen > Test users** and add your Gmail address.

### Test passes but sync doesn't start

Make sure you saved the account after testing. In the TUI: press `t` to test, then `s` to save. The test validates credentials but doesn't persist the account until you save.

### Token expired or revoked

Re-authorize by running `mxr accounts add gmail` again, or in the TUI press `a` on the account to re-authorize.
