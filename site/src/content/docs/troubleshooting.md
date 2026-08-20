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

`mxr sync` acks as soon as the sync has started, so "hanging" is really "the account never goes idle". Watch it with a wait long enough to outlast a backfill:

```bash
mxr sync --wait --wait-timeout-secs 900
```

`--wait-timeout-secs` bounds the wait, not the sync: on expiry the command exits non-zero and the sync keeps running. A moving progress line (`personal: 3,000/50,000 — Stored 3000 messages`) means it is working; a stuck count means it is not.

If it times out with no movement, check the daemon logs:

```bash
mxr logs --level error --since 10m --format json | jq .
```

Common causes:

- **Provider rate-limit.** The daemon backs off automatically; just wait. `mxr status --format json` will show `last_error` if it's a rate-limit retry.
- **Stale Gmail history cursor.** mxr falls back to a full resync automatically. If it doesn't, force one with `mxr doctor --reindex`.
- **Stale IMAP/SMTP password.** Run `mxr accounts repair <name>`. It re-prompts for password-backed IMAP/SMTP credentials and writes them to `secrets.toml`. The command can run directly from config when the daemon is unavailable, so a broken legacy keychain credential does not block its own repair.
- **Stale OAuth credential.** Run `mxr accounts reauth <name>` for Gmail or Outlook. OAuth tokens do not use the IMAP/SMTP repair path.

### A sync that runs for hours

A long sync (a large initial backfill, a slow IMAP server) is never cancelled mid-flight — cancellation could abandon the connection mid-command. It just keeps going:

- `mxr sync --status` keeps showing `In progress: true`, and `--format json` carries a `progress` object with the running count.
- A second `mxr sync` while one is running joins the running sync instead of queueing another pass behind it, and acks immediately.
- When the sync finishes, the status row is finalized normally (success or the real error) and the next sync can start.

No action needed — check again later:

```bash
mxr sync --status --format json | jq '.[] | {account_name, sync_in_progress, progress, last_error}'
```

### `sync interrupted by daemon restart`

If the daemon dies mid-sync (crash, `kill -9`, reboot), the next daemon start clears the stale in-progress flag and records `failure_class: interrupted`. The next sync cycle resumes from the last persisted cursor — no data is lost and no cleanup is needed.

## Sent message isn't searchable

In v1+ this should never happen — the daemon inserts a synthetic Sent envelope immediately on send. If you upgraded from `0.4.x` and a message is missing, force a resync:

```bash
mxr sync --wait
```

For SMTP+IMAP accounts: the synthetic Sent envelope is keyed differently from what IMAP-side discovery will produce on the next sync, which can leave a transient duplicate. This is a known v1 follow-up; the duplicate will be resolved by the next IMAP-side reconciler pass.

## `cargo install --locked mxr` says "package not found"

mxr is intentionally not published to crates.io — the workspace's
internal `mxr-*` crates are organizational seams, not library APIs, and
publishing 22 crates per release was a poor fit for what mxr ships.
Install via Homebrew (recommended) or `cargo install --git`:

```bash
brew install planetaryescape/mxr/mxr
# or (replace vX.Y.Z with the latest release tag)
cargo install --git https://github.com/planetaryescape/mxr --tag vX.Y.Z --locked mxr
```

## Search returns nothing for a query that should match

First rule out the address operators. `from:`, `to:`, `cc:`, `bcc:` and `deliveredto:` match the whole address, case-insensitively — there is no partial or domain matching, and a partial still parses and returns zero:

```bash
mxr count "from:alice"              # 0 — not an address
mxr count "from:alice@example.com"  # what you meant
```

Otherwise the Tantivy index can drift if a sync was interrupted before commit. Rebuild it from SQLite:

```bash
mxr doctor --reindex
```

Then verify:

```bash
mxr count "your query"
```

## Daemon won't start

```bash
mxr daemon --foreground
```

Foreground mode prints startup errors to your terminal. If it complains about a stale socket, the simplest fix is:

```bash
mxr restart
```

`mxr restart` reaps the existing daemon, removes any stale socket, and brings a fresh one up against the same binary.

If it complains about a missing migration on the SQLite database, the local store schema is older than the binary. Either run `mxr doctor` (which applies pending migrations) or, as a last resort:

```bash
mxr reset --hard --dry-run        # preview
mxr reset --hard                  # destructive; preserves config + credentials
```

`mxr reset --hard` wipes local cache and the search index but keeps your account config and credentials. Re-run `mxr sync --wait --wait-timeout-secs 900` after — that resync is a full backfill.

## Daemon unreachable after an upgrade

When you run a newly upgraded `mxr` binary, the first command restarts the daemon to match the new build. The restart waits for the old daemon to fully exit before starting its successor, and a shutting-down daemon never removes a socket it no longer owns — so the replacement daemon is reachable as soon as the restart message finishes.

On versions up to 0.5.62 this handoff could race: the old daemon's exit cleanup deleted the new daemon's socket, leaving a daemon that was alive and syncing but that no client could reach (the TUI would show an action status like `Archiving...` forever). If you hit that state on an old version, run any `mxr` command — it detects the missing socket and restarts the daemon — then retry the stuck action, which was never applied.
