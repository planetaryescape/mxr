# Privacy Policy

**Effective date**: 2026-03-18
**Last updated**: 2026-05-31

mxr is a local-first, open-source email client. Your mail data is stored on your machine, and mxr does not run a hosted relay, analytics service, or remote database.

---

## Data Storage

mxr stores mail metadata, local draft state, activity history, and searchable indexes under the active local profile.

Default release-build locations:

| Data | Linux / XDG | macOS |
|---|---|---|
| Config | `$XDG_CONFIG_HOME/mxr/config.toml` | `~/Library/Application Support/mxr/config.toml` |
| SQLite database and local data | `$XDG_DATA_HOME/mxr/` | `~/Library/Application Support/mxr/` |
| Token fallback files | `$XDG_DATA_HOME/mxr/tokens/` | `~/Library/Application Support/mxr/tokens/` |

`MXR_CONFIG_DIR`, `MXR_DATA_DIR`, and `MXR_TOKEN_DIR` can override these paths.

The Tantivy search index and semantic model cache are local and rebuildable. Attachments opened or saved through mxr are written locally.

---

## Credentials

IMAP and SMTP passwords are stored in `secrets.toml` in the mxr config directory, a plaintext file with mode `0600` (owner read and write only). This is the same model as `~/.aws/credentials`: any process running as you can read it, and it is not encrypted at rest. mxr chose this because macOS revokes keychain access for the release binary on every upgrade, which used to stop the daemon from starting.

Gmail OAuth refresh tokens are stored in the OS secret store when it is available (macOS Keychain, or Secret Service such as GNOME Keyring or KWallet on Linux). A private disk fallback under the active token directory keeps an otherwise valid account working when a noninteractive keychain read fails. Outlook OAuth tokens are stored as JSON files under the active token directory. `config.toml` refers to credentials and never contains a password.

---

## No Telemetry

mxr does not collect telemetry, analytics, crash reports, tracking pixels, or anonymous usage statistics.

---

## Network Requests

mxr makes network requests only for configured user workflows:

- Gmail API calls to sync messages, send mail, and manage labels.
- Google OAuth calls to authorize and refresh Gmail access.
- IMAP connections to configured mail servers.
- SMTP connections to configured mail servers.
- Microsoft identity/OAuth calls for Outlook-style OAuth accounts.
- Optional model downloads or external LLM calls only when the user explicitly configures those features.

mxr does not contact an mxr-operated server.

---

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

---

## Optional AI Features

Local search, reading, and core mailbox operations do not require a hosted AI service. If you configure a nonlocal LLM provider, mxr sends only the prompts required for the enabled feature to that provider. Agent, MCP, and LLM workflows should be treated as user-directed exports of local mail context.

---

## Third-Party Services

mxr does not integrate with third-party analytics, advertising, or tracking services. The third-party services involved are the mail providers and optional AI/model providers you explicitly configure.

---

## Data Deletion

When an email is deleted from your mailbox, mxr deletes everything it derived from that email: search and semantic index entries, summaries, briefings, extracted commitments and decisions, and downloaded attachment files. Since data is local, you can delete all mxr data by removing the active config and data directories. To inspect them:

```bash
mxr status --format json
```

To revoke Gmail access, visit [Google Account Permissions](https://myaccount.google.com/permissions) and remove mxr or your custom OAuth app.

---

## Open Source

mxr is open source under the MIT and Apache-2.0 licenses. You can audit the code to verify these claims.

---

## Contact

For privacy-related questions, open an issue on the [GitHub repository](https://github.com/planetaryescape/mxr).
