# mxr — Addendum: OAuth2 Flow & Bundled Client ID

> Amendment A009. This document extends 03-providers.md and 12-config.md with the OAuth2 strategy for Gmail API access.

---

## A009: OAuth2 Flow & Bundled Client ID

**Affects**: 03-providers.md, 12-config.md, 13-open-source.md

**What was missing**: The blueprint specifies Gmail API access via yup-oauth2 but does not define how OAuth credentials are distributed, how Google verification works, or how users can bring their own credentials.

---

## Bundled OAuth Client ID

mxr ships a bundled OAuth `client_id` and `client_secret` using Google's "Desktop app" (installed application) flow. This is the standard approach for native/CLI applications that cannot keep a secret — Google explicitly supports this via the `urn:ietf:wg:oauth:2.0:oob` redirect and localhost loopback patterns.

### How it works

1. User runs `mxr account add gmail`.
2. mxr uses the bundled client ID to initiate the OAuth2 installed-app flow via yup-oauth2.
3. Browser opens to Google's consent screen.
4. User grants access. Token is returned to the localhost redirect.
5. yup-oauth2 persists the token to disk (`~/.config/mxr/tokens/<account>/oauth.json`).
6. Subsequent API calls use the cached token. yup-oauth2 handles refresh automatically.

### Scopes requested

- `https://www.googleapis.com/auth/gmail.readonly` — read messages and labels
- `https://www.googleapis.com/auth/gmail.labels` — manage labels
- `https://www.googleapis.com/auth/gmail.modify` — mark read/unread, archive, trash
- `https://www.googleapis.com/auth/gmail.send` — send email via Gmail API

Scopes are requested incrementally if possible. Read-only users who send via SMTP never need the `gmail.send` scope.

---

## Google Verification Requirements

Google imposes verification requirements on OAuth apps based on usage thresholds.

### Unverified app limits

- Up to 100 users can authorize an unverified app.
- Users see a "This app isn't verified" warning screen (click-through).
- No restrictions on functionality — all scopes work.

### Verification requirements

When the app exceeds 100 users, Google requires:

1. **Privacy policy** — hosted at a publicly accessible URL.
2. **Domain ownership** — verified domain for the privacy policy URL.
3. **CASA security assessment** — required for sensitive scopes (gmail.modify, gmail.send). This is a third-party security audit.
4. **App description and justification** — explanation of why each scope is needed.

### Launch strategy

**Phase 1 (launch)**: Ship unverified. The 100-user cap is sufficient for early adopters. The click-through warning is acceptable for a technical audience installing a terminal email client.

**Phase 2 (growth)**: Apply for Google verification when approaching the 100-user threshold. Requirements:

- Privacy policy hosted at the docs site (mirrors `PRIVACY.md`).
- Domain verified via Google Search Console.
- CASA assessment completed (timeline: 2-4 weeks typically).
- Submit via Google Cloud Console > API & Services > OAuth consent screen > Publish.

---

## Token Storage

### Current approach

yup-oauth2 persists tokens to disk at `~/.config/mxr/tokens/<account>/oauth.json`. The file contains:

- Access token (short-lived, ~1 hour)
- Refresh token (long-lived)
- Expiry timestamp
- Token type

File permissions are set to `0600` (owner read/write only).

### Future enhancement: keyring integration

A future version may support OS keyring storage (macOS Keychain, GNOME Keyring, Windows Credential Manager) via the `keyring` crate. This is a nice-to-have, not a launch blocker. The disk-based approach is standard for CLI tools (gcloud, gh, and aws-cli all store tokens on disk).

---

## BYOC: Bring Your Own Credentials

Users who prefer to use their own Google Cloud project (or who hit the 100-user cap before verification) can provide their own OAuth credentials.

### Configuration

In `~/.config/mxr/config.toml`:

```toml
[accounts.personal]
provider = "gmail"
email = "user@gmail.com"

[accounts.personal.oauth]
client_id = "YOUR_CLIENT_ID.apps.googleusercontent.com"
client_secret = "YOUR_CLIENT_SECRET"
```

### Resolution order

1. If `accounts.<name>.oauth.client_id` is set in config, use it.
2. Otherwise, use the bundled client ID.

### Documentation

The docs site will include a guide for creating your own Google Cloud project and OAuth credentials, covering:

- Creating a project in Google Cloud Console
- Enabling the Gmail API
- Creating OAuth 2.0 credentials (Desktop app type)
- Configuring the consent screen
- Adding the credentials to mxr config

This is optional — the bundled client ID is the default and recommended path.
