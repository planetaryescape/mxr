---
title: First sync
description: From installed binary to a working local mailbox.
---

## Choose a client

```bash
mxr
```

`mxr` starts the daemon automatically. Use the TUI, web app, CLI or an
[existing agent](/getting-started/agent-quick-start/); clients connected
to the same runtime use its shared mailbox. Start the human workflow on
[Now](/guides/now/), then follow [Your day](/guides/your-day/) through the
five modes.

## Trigger sync

`mxr sync` hands the work to the daemon and returns as soon as the sync has started, so you can start using mxr while the backfill runs. Leave the daemon running.

```bash
mxr sync                                  # trigger and return
mxr sync --wait --wait-timeout-secs 900   # trigger, then wait for the account to go idle
mxr sync --status                         # check progress at any time
mxr status --watch                        # live daemon status
```

`--wait` prints sync progress on stderr and reports detected sync failures with a non-zero exit. Failure detection depends on reading status before starting the sync; if that read fails, a later sync failure can be missed. Check `mxr sync --status` for the account's last error.

Search can use only mail already synced while the backfill continues. Message bodies are stored with their envelopes; file attachments may be fetched later when you open or save them. Reaching idle ends the current sync pass for the selected account, not sync for every account or attachment caching.

`--wait-timeout-secs` (default 60) bounds only the wait, not the sync. If the wait expires, check status later; the sync continues.

<details>
<summary>For developers: run the daemon in the foreground</summary>

If you're debugging the daemon itself, run it explicitly so its logs print to your terminal:

```bash
# Terminal 1: keep this running
mxr daemon --foreground

# Terminal 2: any other mxr command will use this daemon
mxr status
```

In normal use you don't need this: `mxr`, `mxr sync`, and friends auto-start a background daemon if one isn't already running.

</details>

## Verify local data

```bash
mxr search "label:inbox" --format ids
mxr labels
mxr thread THREAD_ID
```

## What happens after sync finishes

Current runtime behavior:

1. envelopes + bodies are already in SQLite
2. lexical search is already fresh for that sync batch
3. semantic chunks are persisted for the changed messages
4. embeddings are generated only if semantic search is enabled

Useful checks:

```bash
mxr search "deployment" --mode lexical
mxr semantic status
mxr doctor --semantic-status
```

## Continue with the TUI

The TUI opens the mailbox; its sidebar also links to [Now and the five
modes](/reference/tui/#now-and-the-modes). Use the [TUI reference](/reference/tui/)
for current screens and keybindings. The mailbox view remains useful for
arrival-order work; [Your day](/guides/your-day/) describes the mode-based
human workflow.

## If something looks wrong

If an account still has messages but all its local label associations are
missing, trigger sync for that account:

```bash
mxr sync --account personal --wait --wait-timeout-secs 900
mxr search "label:inbox" --account personal --format json
```

For label-capable providers, sync can refetch the account's mail to restore
associations even when other accounts have healthy labels. An ordinary poll can
start one full refetch. The refetch runs through its pages and finishes even if
labels remain unresolved, rather than restarting again on its final page. A
later poll can retry. Recovery does not detect partial label loss or invent
labels the provider cannot resolve. If labels remain missing, inspect diagnostics:

```bash
mxr doctor --check
mxr events --format json
mxr logs --level error
mxr bug-report --stdout
```

## Next

- [Mailbox workflow](/guides/mailbox/)
- [Compose](/guides/compose/)
- [Accounts](/guides/accounts/)
