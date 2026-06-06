---
title: Gmail setup
description: Connect a Gmail account to mxr.
---

mxr connects to Gmail through the Gmail API using OAuth. The official v1 path is **bring your own Google OAuth client**: create a Google Cloud project, paste its desktop-app Client ID/Secret into mxr, and authorize your own Gmail account. This avoids depending on mxr's shared client and keeps the consent screen under your control.

Release builds may also include mxr's bundled OAuth client. Treat it as an unverified fallback for early adopters: it can show Google's "app has not completed verification" warning, can be quota-limited, and should not be the primary setup plan for production Gmail access.

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
4. On the **Scopes** page, add:
   - `https://www.googleapis.com/auth/gmail.readonly`
   - `https://www.googleapis.com/auth/gmail.modify`
   - `https://www.googleapis.com/auth/gmail.labels`

mxr does not request `gmail.send` as a separate scope today; Gmail API sends use the authorized Gmail client under the `gmail.modify` grant.
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
MXR_GMAIL_CLIENT_SECRET="YOUR_CLIENT_SECRET" \
  mxr accounts add gmail \
    --gmail-bundled=false \
    --gmail-client-id "YOUR_CLIENT_ID.apps.googleusercontent.com"
```

You can also run `mxr accounts add gmail` interactively, select **custom** for credential source, paste the Client ID and Client Secret, and complete browser authorization when it opens.

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

## Bundled credentials fallback

If you explicitly accept Google's unverified-app warning, you can try the bundled client in a release build:

```bash
mxr accounts add gmail
# Select "bundled" when prompted for credential source, or omit --gmail-bundled=false.
# Browser opens — Google may require Advanced > Go to mxr (unsafe).
# Complete authorization.
```

Use this for small-scale testing only. To switch later, re-run `mxr accounts add gmail` with `--gmail-bundled=false`; mxr replaces the account config after authorization succeeds.

## Archived mail and Gmail over IMAP

The Gmail API path syncs Gmail labels directly. If you connect Gmail through the generic IMAP adapter instead, mxr now detects Gmail's IMAP extension and syncs canonical `[Gmail]/All Mail` / `\\All` when available. That matters for archived mail: Gmail archives messages by removing `INBOX`, so a folder-by-folder IMAP sync that skipped All Mail could miss archived-only messages. With the All Mail path, archived messages stay visible in mxr search and All Mail views.

If the server does not advertise Gmail All Mail, mxr falls back to normal folder sync and documents IMAP folder semantics honestly: folders are not Gmail labels, and archive/move operations map to provider folder operations.

## Working over SSH or in a container

The OAuth flow opens a browser on the same machine the daemon is running on. If you're SSH'd into a server, that browser opens *on the server*, not your laptop, and the localhost callback won't reach you.

mxr auto-detects this case (no TTY / no `DISPLAY` / `SSH_CONNECTION` set) and switches to the [Limited Input Device flow (RFC 8628)](https://datatracker.ietf.org/doc/html/rfc8628): it prints a code and a `https://www.google.com/device` URL; you open that URL in any browser and paste the code. To see the prompt, run the daemon in the foreground while you add the account:

```bash
# Terminal 1: keep this running
mxr daemon --foreground

# Terminal 2:
mxr accounts add gmail --account-name personal --email you@gmail.com
```

The bundled Gmail OAuth client may be configured as a Desktop-app type, which Google does **not** allow for device flow. If you see `invalid_request: device_id` from Google, drop down to one of:

- **Bring your own credentials** of OAuth client type *TV and Limited Input devices* (see [Create your Google Cloud project](#create-your-google-cloud-project)) — this gives you a stable custom client that supports device flow.
- **IMAP + app password** — the simplest SSH-friendly path:

  ```bash
  MXR_IMAP_PASSWORD="$APP_PASSWORD" MXR_SMTP_PASSWORD="$APP_PASSWORD" \
    mxr accounts add imap-smtp --account-name personal \
      --email you@gmail.com \
      --imap-host imap.gmail.com --imap-username you@gmail.com \
      --smtp-host smtp.gmail.com --smtp-port 587 \
      --smtp-username you@gmail.com
  ```

  Generate the app password at <https://myaccount.google.com/apppasswords> (requires 2FA).

## Troubleshooting

### "Gmail API has not been used in project X" or "accessNotConfigured"

You need to enable the Gmail API. Go to **APIs & Services > Library** in Google Cloud Console, search for Gmail API, and click **Enable**.

### "Access blocked: This app's request is invalid" or "invalid_client"

Your Client ID or Client Secret is wrong. Double-check you copied them correctly. Make sure you created a **Desktop app** credential, not a Web application.

### "Access blocked: mxr has not completed the Google verification process"

This is Google's unverified-app warning, usually from the bundled fallback client. Use your own OAuth client (`--gmail-bundled=false`) for the official v1 path.

### "This app is blocked" (no Advanced option)

Your OAuth consent screen may not have your email as a test user. Go to **OAuth consent screen > Test users** and add your Gmail address.

### Test passes but sync doesn't start

Make sure you saved the account after testing. In the TUI: press `t` to test, then `s` to save. The test validates credentials but doesn't persist the account until you save.

### Token expired or revoked

Re-authorize by running `mxr accounts add gmail` again, or in the TUI press `a` on the account to re-authorize.
