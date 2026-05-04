---
title: Troubleshooting
description: Common gotchas and how to fix them.
---

## OAuth on SSH wedges

The default OAuth flow needs a localhost browser callback. When you're SSH'd into a remote box, that browser opens *on the server* and never reaches you.

mxr auto-detects this case (no TTY / `SSH_CONNECTION` set / no `DISPLAY`) and switches to the [Limited Input Device flow (RFC 8628)](https://datatracker.ietf.org/doc/html/rfc8628). Run the daemon in the foreground to see the device code:

```bash
# Terminal 1
mxr daemon --foreground

# Terminal 2
mxr accounts add gmail --account-name personal --email you@gmail.com
```

If the bundled OAuth client is configured as a Desktop app on Google's side, device flow may fail with `device_id` errors. Drop down to IMAP+SMTP with an app password:

```bash
mxr accounts add imap \
  --email you@gmail.com \
  --imap-host imap.gmail.com \
  --imap-username you@gmail.com \
  --imap-password "$APP_PASSWORD" \
  --smtp-host smtp.gmail.com \
  --smtp-username you@gmail.com \
  --smtp-password "$APP_PASSWORD"
```

Generate the app password at <https://myaccount.google.com/apppasswords> (requires 2FA on the account).

## Sync hangs or never completes

```bash
mxr sync --wait --wait-timeout-secs 120
```

If it times out, check the daemon logs:

```bash
mxr logs --level error --since 10m --format json | jq .
```

Common causes:

- **Provider rate-limit.** The daemon backs off automatically; just wait. `mxr status --format json` will show `last_error` if it's a rate-limit retry.
- **Stale Gmail history cursor.** mxr falls back to a full resync automatically. If it doesn't, force one with `mxr doctor --reindex`.
- **Stale OAuth token.** Run `mxr accounts repair <name>` for IMAP+SMTP, or `mxr accounts add gmail --account-name <existing-name>` to re-auth (overwrites the token).

## Sent message isn't searchable

In v1+ this should never happen — the daemon inserts a synthetic Sent envelope immediately on send. If you upgraded from `0.4.x` and a message is missing, force a resync:

```bash
mxr sync --wait
```

For SMTP+IMAP accounts: the synthetic Sent envelope is keyed differently from what IMAP-side discovery will produce on the next sync, which can leave a transient duplicate. This is a known v1 follow-up; the duplicate will be resolved by the next IMAP-side reconciler pass.

## `cargo install --locked mxr` says "package not found"

Until v0.5.0 lands on crates.io, install via Homebrew or build from source:

```bash
brew install planetaryescape/mxr/mxr
# or
cargo install --git https://github.com/planetaryescape/mxr --tag v0.4.54 --locked mxr
```

## Search returns nothing for a query that should match

Tantivy index can drift if a sync was interrupted before commit. Rebuild it from SQLite:

```bash
mxr doctor --reindex
```

Then verify:

```bash
mxr count --search "your query"
```

## Daemon won't start

```bash
mxr daemon --foreground
```

Foreground mode prints startup errors to your terminal. If it complains about a stale socket:

```bash
# Mac/Linux
rm "$(mxr config get socket_path)"
mxr daemon
```

If it complains about a missing migration on the SQLite database, the local store schema is older than the binary. Either run `mxr doctor` (which applies pending migrations) or, as a last resort:

```bash
mxr reset --hard --dry-run        # preview
mxr reset --hard                  # destructive; preserves config + credentials
```

`mxr reset --hard` wipes local cache and the search index but keeps your account config and credentials. Re-run `mxr sync --wait` after.
