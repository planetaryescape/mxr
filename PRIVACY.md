# Privacy Policy

**Effective date**: 2026-03-18
**Last updated**: 2026-03-18

mxr is a local-first, open-source email client. Your privacy is not a feature we had to add — it is a consequence of how mxr is built.

---

## Data Storage

All email data is stored locally on your machine in a SQLite database. The search index (Tantivy) is also local and rebuildable from the SQLite database. There is no cloud component, no remote database, and no server-side storage.

**Data locations** (default):

- Database: `~/.local/share/mxr/`
- Search index: `~/.local/share/mxr/`
- OAuth tokens: `~/.config/mxr/tokens/`
- Configuration: `~/.config/mxr/`

---

## No Telemetry

mxr does not collect telemetry, analytics, crash reports, or usage data of any kind. There are no tracking pixels, no phone-home requests, and no anonymous usage statistics. Zero.

---

## Network Requests

mxr makes network requests only when you explicitly trigger them:

- **Gmail API calls** — to sync messages, send email, and manage labels. These go directly to Google's servers (`gmail.googleapis.com`).
- **IMAP connections** — to sync messages from IMAP servers. These go directly to your configured mail server.
- **SMTP connections** — to send email. These go directly to your configured SMTP server.
- **OAuth token refresh** — periodic token refresh requests to Google's OAuth endpoint (`oauth2.googleapis.com`).

No other network requests are made. mxr does not contact any mxr-operated server.

---

## OAuth Tokens

When you authenticate with Gmail, OAuth tokens are stored locally on disk by yup-oauth2 at `~/.config/mxr/tokens/<account>/oauth.json`. File permissions are set to `0600` (owner read/write only). Tokens are never transmitted anywhere other than directly to Google's API endpoints.

---

## Gmail API Scopes

mxr requests the following Gmail API scopes:

| Scope | Purpose |
|---|---|
| `gmail.readonly` | Read messages and metadata |
| `gmail.labels` | Manage labels |
| `gmail.modify` | Mark read/unread, archive, trash |
| `gmail.send` | Send email via Gmail API |

These scopes are requested based on your usage. If you send email via SMTP instead of Gmail API, the `gmail.send` scope is not requested.

---

## Third-Party Services

mxr does not integrate with any third-party analytics, advertising, or data processing services. The only third-party services involved are the email providers you explicitly configure (Gmail, IMAP servers, SMTP servers).

---

## Open Source

mxr is open source under the MIT and Apache-2.0 licenses. You can audit every line of code to verify these claims. The source code is the privacy policy's proof.

---

## Data Deletion

Since all data is local, deleting your data is as simple as removing the mxr data directories:

```bash
rm -rf ~/.local/share/mxr/
rm -rf ~/.config/mxr/
```

To revoke Gmail access, visit [Google Account Permissions](https://myaccount.google.com/permissions) and remove mxr.

---

## Contact

For privacy-related questions, open an issue on the [GitHub repository](https://github.com/planetaryescape/mxr).
