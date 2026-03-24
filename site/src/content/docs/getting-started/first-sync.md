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
