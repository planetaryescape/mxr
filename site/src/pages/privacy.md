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
| HTTP bridge bearer token | `$XDG_CONFIG_HOME/mxr/bridge-token` | `~/Library/Application Support/mxr/bridge-token` |
| Daemon IPC bearer token | `$XDG_CONFIG_HOME/mxr/daemon-token` | `~/Library/Application Support/mxr/daemon-token` |

`MXR_CONFIG_DIR`, `MXR_SECRETS_PATH`, `MXR_DATA_DIR`, `MXR_TOKEN_DIR`,
`MXR_BRIDGE_TOKEN_PATH`, and `MXR_DAEMON_TOKEN_PATH` can override these paths.

The two token files hold local secrets, not mail. The daemon writes each at
mode `0600` the first time it needs one. `bridge-token` authorizes callers of
the local HTTP bridge, which `mxr daemon` starts by default; its path comes from
`bridge.token_path` in `config.toml` when that is set, otherwise from
`MXR_BRIDGE_TOKEN_PATH`, otherwise from the config directory. `daemon-token`
authorizes raw IPC over the loopback TCP transport, so it only exists once you
set `[transports.tcp] enabled = true`; `MXR_DAEMON_TOKEN_PATH` moves it, and
`MXR_DAEMON_TOKEN` supplies the token directly with no file involved.

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

mxr does not collect telemetry, analytics, crash reports, or anonymous usage statistics.

## Network Requests

mxr makes network requests for the following:

- Gmail API calls to sync messages, send mail, and manage labels.
- Google OAuth calls to authorize and refresh Gmail access.
- IMAP connections to configured mail servers.
- SMTP connections to configured mail servers.
- Microsoft identity/OAuth calls for Outlook-style OAuth accounts.
- Remote images in HTML messages, fetched from whatever URLs the message carries.
- Unsubscribe requests to the endpoint or link a message supplies.
- Embedding model downloads from Hugging Face for semantic search.
- External LLM calls, when you configure a nonlocal provider.

mxr does not contact an mxr-operated server. Remote-image fetching and semantic
model downloads are on by default in shipped release builds. Each has its own
section below.

### Remote Images in HTML Messages

`render.html_remote_content` defaults to `true`. With it on, viewing a message's
HTML makes the daemon fetch every remote image that message points at. The TUI's
HTML view and `mxr cat --assets` both do this. Those URLs were chosen by whoever
sent the mail, so each fetch tells that server your IP address, when you opened
the message, and whatever per-recipient identifier they put in the URL. That is
how an open-tracking pixel works, and nothing on this path filters those pixels
out. Fetched images are cached under `_html_assets/` in the attachment
directory.

Turn the fetches off:

```bash
mxr config set render.html_remote_content false
```

Remote images then come back marked `blocked` and nothing is requested. `M` in
the TUI flips the same switch for the running session without touching the
config.

The web app is a separate case. It renders message HTML in your browser, so your
browser issues the image requests, and its "Remote images" toggle starts on
regardless of `render.html_remote_content`. It does drop some pixels before
rendering: images whose `width` or `height` attribute is two pixels or less, and
images from a short list of known tracker hosts. A pixel sized in CSS, or served
from a host outside that list, loads like any other image. Turn the toggle off
there as well.

### Unsubscribing

`mxr unsubscribe` acts on the `List-Unsubscribe` header the sender put in the
message. For a one-click subscription mxr POSTs `List-Unsubscribe=One-Click` to
the sender's endpoint. For the link methods it opens the sender's URL in your
browser. Either way the destination comes from the message, and the request
tells the sender your IP address and that the address they mailed is live.
`mxr unsubscribe <sender> --dry-run` shows what it would act on without
contacting anyone, and `mxr subscriptions` lists the stored method for every
sender.

### Semantic Search and Model Downloads

The prebuilt binaries from Homebrew, `install.sh`, and the GitHub release
archives are built with the `semantic-local` feature, and the shipped defaults
have semantic indexing on:

```toml
[search.semantic]
enabled = true
auto_download_models = true
active_profile = "bge-small-en-v1.5"
```

Embeddings are computed on your machine, and `lexical` is still the default
search mode. Indexing, though, runs in the background after every sync. If the
active profile's model files are not already in `models/` under the data
directory, `auto_download_models = true` lets that background pass download them
from `https://huggingface.co` without asking first. `HF_ENDPOINT` sends the
download to a mirror instead.

Turning semantic work off stops that:

```bash
mxr semantic disable    # or: mxr config set search.semantic.enabled false
```

Sync still prepares text chunks locally, so nothing is downloaded and turning
semantic search back on later stays cheap.

To keep semantic search and refuse downloads, edit `config.toml`:

```toml
[search.semantic]
auto_download_models = false
```

A model you already have keeps working; a missing one becomes an error instead
of a download. `mxr config set` does not expose that key, so this one has to be
edited by hand.

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

mxr does not integrate with third-party analytics, advertising, or tracking services. The services it contacts are the mail providers and optional AI or model providers you configure, plus Hugging Face for embedding model downloads. Remote images and unsubscribe requests also go to whatever hosts a message names; those destinations come from the sender, and mxr does not integrate with them.

## Data Deletion

Since data is local, you can delete mxr data by removing the active config and data directories. To find them:

```bash
mxr status --format json
```

`config_path` in that output is the `config.toml` file, not a directory; the config directory is its parent. `data_dir` is the data directory itself. At the default paths, deleting the config directory removes `config.toml`, `secrets.toml`, and the `bridge-token` and `daemon-token` files together, and deleting the data directory removes the OAuth token files in `tokens/`, the downloaded embedding models in `models/`, and the cached remote images under `attachments/_html_assets/` along with the rest of your local mail data. Deleting `config.toml` on its own leaves your IMAP and SMTP passwords sitting in `secrets.toml`. `MXR_SECRETS_PATH`, `MXR_TOKEN_DIR`, `MXR_BRIDGE_TOKEN_PATH`, `MXR_DAEMON_TOKEN_PATH`, and `bridge.token_path` move those files outside the two directories, and status does not report where they went, so delete whatever paths you set as well. `general.attachment_dir` moves the attachment cache, `_html_assets/` included, out of the data directory, and `MXR_ATTACHMENT_DIR` overrides that setting for any process that reads it. Neither shows up in status, so run `mxr config get general.attachment_dir` in the same environment the daemon runs under and delete the path it prints.

Any IMAP or SMTP password copied to the OS-native secret store, and any Gmail OAuth token held there, has to be deleted separately with Keychain Access on macOS or your Secret Service tool on Linux.

To revoke Gmail access, visit [Google Account Permissions](https://myaccount.google.com/permissions) and remove mxr or your custom OAuth app.

## Open Source

mxr is open source under the MIT and Apache-2.0 licenses. You can audit the code to verify these claims.

## Contact

For privacy-related questions, open an issue on the [GitHub repository](https://github.com/planetaryescape/mxr).
