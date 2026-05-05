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

---

## A010: Outlook OAuth2 — Device Code Flow & Bundled Client ID

**Affects**: 03-providers.md, 12-config.md

**What was missing**: The Outlook provider uses a different OAuth2 flow (device code) and a different credential distribution mechanism than Gmail. This section documents it.

---

## Bundled Outlook Client ID

mxr can ship a bundled Azure `client_id` compiled into the binary at build time using a compile-time environment variable:

```bash
OUTLOOK_CLIENT_ID=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx cargo build --release
```

The value is embedded via `option_env!("OUTLOOK_CLIENT_ID")` in `crates/provider-outlook/src/auth.rs`. If the env var is not set at compile time, the constant is `None` and users must supply their own `client_id`.

### Checking whether a bundled ID is present

At runtime, if no bundled client ID is compiled in and no `client_id` is set in config, mxr will print an error and refuse to authenticate.

### Release builds

For official releases, set `OUTLOOK_CLIENT_ID` in the CI environment:

```yaml
# GitHub Actions example
- name: Build
  env:
    OUTLOOK_CLIENT_ID: ${{ secrets.OUTLOOK_CLIENT_ID }}
  run: cargo build --release
```

---

## Azure App Registration Requirements

Unlike Google, Microsoft does not enforce a strict verified/unverified user cap for device code flow apps. However, an Azure app registration is required.

### Creating the app registration

1. Go to [portal.azure.com](https://portal.azure.com) > Azure Active Directory > App registrations > New registration.
2. Name: `mxr` (or any name).
3. Supported account types: choose based on target audience:
   - **Personal + work/school**: "Accounts in any organizational directory and personal Microsoft accounts" — use the `/common` endpoint (not recommended; personal device code was unreliable; use separate registrations).
   - **Personal only** (`outlook` variant): "Personal Microsoft accounts only" — uses `/consumers` endpoint.
   - **Work/school only** (`outlook-work` variant): "Accounts in any organizational directory only" — uses `/organizations` endpoint.
4. Redirect URI: leave blank (device code flow does not use redirects).
5. After creation, copy the **Application (client) ID** — this is the value for `OUTLOOK_CLIENT_ID`.

### API permissions

Under the app registration, add delegated permissions:
- `IMAP.AccessAsUser.All` (under Office 365 Exchange Online)
- `SMTP.Send` (under Office 365 Exchange Online)
- `offline_access` (under Microsoft Graph)

Grant admin consent if required by the tenant.

### Tenant-specific endpoints

| Variant | Endpoint | Allowed accounts |
|---|---|---|
| `outlook` (personal) | `/consumers` | `@outlook.com`, `@hotmail.com`, `@live.com` |
| `outlook-work` | `/organizations` | M365, Exchange Online work/school |

Separate app registrations (one per tenant type) are the recommended approach. A single `/common` registration can work but may behave inconsistently for personal accounts with device code flow.

---

## BYOC: Bring Your Own Client ID (Outlook)

Users can provide their own Azure `client_id` per account:

```toml
[accounts.work.sync]
provider = "outlook-work"
token_ref = "mxr/work-outlook"
client_id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

### Resolution order

1. If `client_id` is set in the account's sync config, use it.
2. Otherwise use the `OUTLOOK_CLIENT_ID` value compiled into the binary.
3. If neither is set, authentication fails with a clear error message.

---

## Token Storage

Outlook tokens are stored as JSON at:

```
~/.local/share/mxr/tokens/<token_ref>.json
```

File permissions: `0600`.

The token file contains:
- `access_token` — short-lived (~1 hour)
- `refresh_token` — long-lived
- `expires_at` — RFC 3339 timestamp

Both sync (IMAP) and send (SMTP) share the same token file via the same `token_ref`. The access token is auto-refreshed when within 5 minutes of expiry.
