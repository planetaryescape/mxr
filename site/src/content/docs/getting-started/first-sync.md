---
title: First sync
description: From installed binary to a working local mailbox.
---

## Start the daemon

For development:

```bash
mxr daemon --foreground
```

In another terminal:

```bash
mxr status
```

## Open the TUI

```bash
mxr
```

If the daemon is not already running, `mxr` starts it.

## Trigger sync

```bash
mxr sync
mxr sync --status
mxr status --watch
```

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
