# mxr — Addendum: OAuth2 Flow & Bundled Client ID

> Amendment A009. This document extends 03-providers.md and 12-config.md with the OAuth2 strategy for Gmail API access.

---

## A009: OAuth2 Flow & Bundled Client ID

**Affects**: 03-providers.md, 12-config.md, 13-open-source.md

**What was missing**: The blueprint specifies Gmail API access via yup-oauth2 but does not define how OAuth credentials are distributed, how Google verification works, or how users can bring their own credentials.

---

## Gmail OAuth client source

The official v1 recommendation is bring-your-own-client: users create a
Google Cloud OAuth Desktop app and configure mxr with that Client ID/Secret.
Release builds may also ship a bundled OAuth `client_id` and `client_secret`,
but that client is an unverified fallback for early adopters and may show
Google's warning screen or hit shared-client limits. Local interactive sessions
use Google's installed-app loopback redirect pattern; headless sessions may
fall back to device code flow when the configured Google client supports it.

### How it works

1. User runs `mxr accounts add gmail --gmail-bundled=false --gmail-client-id ...` or selects **custom** in the wizard.
2. mxr uses the configured custom client ID/Secret to initiate the OAuth2 installed-app flow via yup-oauth2. If the user explicitly chooses bundled credentials, mxr uses the compiled fallback client instead.
3. Browser opens to Google's consent screen.
4. User grants access. Token is returned to the localhost redirect.
5. mxr stores the yup-oauth2 token cache in the OS keychain/keyring under the Gmail OAuth service name, while retaining the legacy private on-disk cache as a fallback/migration source.
6. Subsequent API calls use the cached token. yup-oauth2 handles refresh automatically through mxr's storage adapter.

### Scopes requested

- `https://www.googleapis.com/auth/gmail.readonly` — read messages and labels
- `https://www.googleapis.com/auth/gmail.labels` — manage labels
- `https://www.googleapis.com/auth/gmail.modify` — mark read/unread, archive, trash, and perform Gmail API sends under the authorized Gmail client

mxr does not request `gmail.send` as a separate scope today. SMTP sending remains a separate provider path.

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
3. **CASA security assessment** — required if Google classifies the final Gmail scope set as restricted. This is a third-party security audit.
4. **App description and justification** — explanation of why each scope is needed.

### Release strategy

**V1 official setup** uses user-created OAuth clients. This is the documented safe path for Gmail because each user controls their own consent screen, test users, and quota.

**Bundled fallback builds** may use an unverified bundled client while the audience is small and technically tolerant of Google's warning screen. To make that bundled fallback broadly safe, complete:

- Privacy policy hosted at the docs site (mirrors `PRIVACY.md`).
- Domain verified via Google Search Console.
- Google OAuth app verification for the requested Gmail scopes.
- CASA assessment if Google requires it for the final scope set.
- Submit via Google Cloud Console > API & Services > OAuth consent screen > Publish.

Release artifacts include bundled Gmail credentials whenever both
`GMAIL_CLIENT_ID` and `GMAIL_CLIENT_SECRET` are configured at build time. If
either value is omitted, the release falls back to BYOC-only Gmail setup.

---

## Token Storage

### Current approach

mxr wraps yup-oauth2 with `KeychainTokenStorage` (`crates/provider-gmail/src/auth_storage.rs`). The primary store is the OS credential store:

- macOS: Keychain Access via Security.framework
- Linux: Secret Service via the `keyring` crate

The keychain service is `mxr-gmail-oauth` for the production instance, scoped per non-production instance by `mxr_config::gmail_oauth_keychain_service()`.

The token cache contains:

- Access token (short-lived, ~1 hour)
- Refresh token (long-lived)
- Expiry timestamp
- Token type

Legacy token files under the mxr data-dir token directory may still exist. On load, mxr can mirror a legacy disk cache into the keychain and keeps the disk cache available as a controlled fallback so noninteractive keychain failures do not strand an otherwise valid account.

---

## BYOC: Bring Your Own Credentials

Users should use their own Google Cloud project for the official v1 Gmail path. This gives them their own consent screen, quota, and token control while mxr still stores provider tokens locally.

### Configuration

In the file printed by `mxr config path`:

```toml
[accounts.personal]
name = "Personal"
email = "user@gmail.com"

[accounts.personal.sync]
type = "gmail"
credential_source = "custom"
client_id = "YOUR_CLIENT_ID.apps.googleusercontent.com"
client_secret = "YOUR_CLIENT_SECRET"
token_ref = "gmail:personal"

[accounts.personal.send]
type = "gmail"
```

### Resolution order

1. If `accounts.<name>.sync.credential_source = "custom"`, use the configured `client_id` and `client_secret`.
2. Otherwise, use the bundled client ID and secret compiled into the binary.

### Documentation

The docs site includes a guide for creating your own Google Cloud project and OAuth credentials, covering:

- Creating a project in Google Cloud Console
- Enabling the Gmail API
- Creating OAuth 2.0 credentials (Desktop app type)
- Configuring the consent screen
- Adding the credentials to mxr config

For v1, BYOC is primary. The bundled client is the fallback whenever a user explicitly accepts the unverified-client tradeoff.

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
<token_dir>/<sanitized-token-ref>.json
```

`<token_dir>` defaults to `<data_dir>/tokens` for the active runtime
identity and can be overridden with `MXR_TOKEN_DIR`. File permissions:
`0600`.

The token file contains:
- `access_token` — short-lived (~1 hour)
- `refresh_token` — long-lived
- `expires_at` — RFC 3339 timestamp

Both sync (IMAP) and send (SMTP) share the same token file via the same `token_ref`. The access token is auto-refreshed when within 5 minutes of expiry.
