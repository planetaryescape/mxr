---
title: First sync
description: From installed binary to a working local mailbox.
---

## Open the TUI

```bash
mxr
```

`mxr` auto-starts the daemon on first run. No separate process to manage.

## Trigger sync

```bash
mxr sync --wait     # blocks until first sync finishes
mxr sync --status   # check progress at any time
mxr status --watch  # live daemon status
```

<details>
<summary>For developers: run the daemon in the foreground</summary>

If you're debugging the daemon itself, run it explicitly so its logs print to your terminal:

```bash
# Terminal 1: keep this running
mxr daemon --foreground

# Terminal 2: any other mxr command will use this daemon
mxr status
```

In normal use you don't need this — `mxr`, `mxr sync`, and friends auto-start a background daemon if one isn't already running.

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

## What to expect in the TUI

- Left sidebar: system labels, user labels, saved searches
- Center list: threads by default
- Right pane: preview or thread view
- First-run walkthrough after account setup, plus `o` from Help to reopen it later
- `1`-`5`: switch Mailbox, Search, Rules, Accounts, Diagnostics
- `Ctrl-p`: command palette
- `?`: full help modal
- `/`: full-index Search
- `Ctrl-f`: filter only the current mailbox
- `gc`: edit config from anywhere

## If something looks wrong

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
