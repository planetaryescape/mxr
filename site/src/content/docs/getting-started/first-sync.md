---
title: First Sync
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

## What to expect in the TUI

- left sidebar: system labels, user labels, saved searches
- center list: threads by default
- right pane: preview or thread view
- `Ctrl-p`: command palette
- `?`: full help modal
- `/`: inline mailbox search

## If something looks wrong

```bash
mxr doctor --check
mxr events --format json
mxr logs --level error
mxr bug-report --stdout
```

## Next

- [Mailbox Workflow](/guides/mailbox/)
- [Compose](/guides/compose/)
- [Accounts](/guides/accounts/)
