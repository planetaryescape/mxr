---
title: Privacy Policy
layout: ../layouts/legal.astro
---

**Effective date**: 2026-03-18
**Last updated**: 2026-07-29

mxr is a local-first, open-source email client. Your mail data is stored on your machine, and mxr does not run a hosted relay, analytics service, or remote database.

## Data Storage

mxr stores mail metadata, local draft state, activity history, and searchable indexes under the active local profile.

Default release-build locations:

| Data | Linux / XDG | macOS |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/mxr/config.toml` | `~/Library/Application Support/mxr/config.toml` |
| IMAP and SMTP passwords | `$XDG_CONFIG_HOME/mxr/secrets.toml` | `~/Library/Application Support/mxr/secrets.toml` |
| SQLite database and local data | `$XDG_DATA_HOME/mxr/` | `~/Library/Application Support/mxr/` |
| OAuth token files | `$XDG_DATA_HOME/mxr/tokens/` | `~/Library/Application Support/mxr/tokens/` |

`MXR_CONFIG_DIR`, `MXR_SECRETS_PATH`, `MXR_DATA_DIR`, and `MXR_TOKEN_DIR` can
override these paths.

The Tantivy search index and semantic model cache are local and rebuildable. Attachments opened or saved through mxr are written locally.

## Credentials

IMAP and SMTP passwords are stored on disk, in `secrets.toml` in the config directory. On macOS and Linux that file is plain TOML at mode `0600`: your user account can read and write it, other users have no access. Root and anything else running under your own account can still read it. The passwords are not encrypted at rest, so the file mode is the only protection. This is the arrangement `~/.aws/credentials` and `~/.config/gh/hosts.yml` use.

The OS-native secret store is an optional fallback for those passwords:

- macOS: Keychain
- Linux: Secret Service, such as GNOME Keyring or KWallet

When a password is missing from `secrets.toml`, mxr looks in the secret store, copies what it finds back to `secrets.toml`, and reads it from disk after that. Set `MXR_KEYCHAIN=off` to keep IMAP and SMTP password handling away from the secret store entirely.

Gmail OAuth refresh tokens are written to the OS-native secret store and to a private file at mode `0600` under the active token directory. mxr reads the secret store first and falls back to the file, so a keychain read that fails without a prompt does not strand a working account. Outlook OAuth tokens are JSON files at mode `0600` under the active token directory. `MXR_KEYCHAIN=off` does not change OAuth token storage.

`config.toml` refers to credentials by reference name. It never holds an IMAP or SMTP password.

## No Telemetry

mxr does not collect telemetry, analytics, crash reports, tracking pixels, or anonymous usage statistics.

## Network Requests

mxr makes network requests only for configured user workflows:

- Gmail API calls to sync messages, send mail, and manage labels.
- Google OAuth calls to authorize and refresh Gmail access.
- IMAP connections to configured mail servers.
- SMTP connections to configured mail servers.
- Microsoft identity/OAuth calls for Outlook-style OAuth accounts.
- Optional model downloads or external LLM calls only when the user explicitly configures those features.

mxr does not contact an mxr-operated server.

## Gmail API Scopes

mxr may request these Gmail API scopes:

| Scope | Purpose |
|---|---|
| `gmail.readonly` | Read messages and metadata |
| `gmail.labels` | Read and manage labels |
| `gmail.modify` | Mark read/unread, archive, trash, and apply labels |

Gmail API sending currently uses the authorized Gmail client under the
`gmail.modify` grant. mxr does not request `gmail.send` as a separate scope
today.

mxr uses Google user data only to provide local mail sync, search, display, drafting, sending, and user-requested mailbox actions. mxr does not sell Google user data, use it for advertising, or transfer it to third parties except as necessary to provide user-directed email functionality. mxr's use and transfer of information received from Google APIs adheres to the [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy), including the Limited Use requirements.

## Optional AI Features

Local search, reading, and core mailbox operations do not require a hosted AI service. If you configure a nonlocal LLM provider, mxr sends only the prompts required for the enabled feature to that provider. Agent, MCP, and LLM workflows should be treated as user-directed exports of local mail context.

## Third-Party Services

mxr does not integrate with third-party analytics, advertising, or tracking services. The third-party services involved are the mail providers and optional AI/model providers you explicitly configure.

## Data Deletion

Since data is local, you can delete mxr data by removing the active config and data directories. To find them:

```bash
mxr status --format json
```

`config_path` in that output is the `config.toml` file, not a directory; the config directory is its parent. `data_dir` is the data directory itself. At the default paths, deleting the config directory removes `config.toml` and `secrets.toml` together, and deleting the data directory removes the OAuth token files in `tokens/` along with the rest of your local mail data. Deleting `config.toml` on its own leaves your IMAP and SMTP passwords sitting in `secrets.toml`. `MXR_SECRETS_PATH` and `MXR_TOKEN_DIR` move those files outside the two directories, and status does not report where they went, so delete whatever paths you set as well.

Any IMAP or SMTP password copied to the OS-native secret store, and any Gmail OAuth token held there, has to be deleted separately with Keychain Access on macOS or your Secret Service tool on Linux.

To revoke Gmail access, visit [Google Account Permissions](https://myaccount.google.com/permissions) and remove mxr or your custom OAuth app.

## Open Source

mxr is open source under the MIT and Apache-2.0 licenses. You can audit the code to verify these claims.

## Contact

For privacy-related questions, open an issue on the [GitHub repository](https://github.com/planetaryescape/mxr).
