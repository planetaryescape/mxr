---
title: Labels and saved searches
description: Organize mail with labels, sidebar filters, and saved inbox lenses.
---

## Labels

Labels are first-class in both CLI and TUI.

CLI:

```bash
mxr labels
mxr labels create FollowUp --color "#ff6600"
mxr labels rename FollowUp Waiting
mxr labels delete Waiting
```

Per-message mutations:

```bash
mxr label Work MESSAGE_ID
mxr unlabel Work MESSAGE_ID
mxr move Archive MESSAGE_ID
```

Search-driven bulk mutations:

```bash
mxr label FollowUp --search "from:recruiter@example.com"
mxr move Done --search "label:inbox from:billing@example.com"
```

## TUI label flows

- `l`: apply label
- `v`: move to label
- Sidebar labels change mailbox scope
- Open messages and thread headers show label chips

The sidebar separates:

- System labels
- User labels
- Saved searches

## Saved searches

Saved searches are reusable mailbox scopes.

CLI:

```bash
mxr saved
mxr saved add recruiters "label:inbox from:recruiter@example.com"
mxr saved delete recruiters
mxr saved run recruiters
```

TUI:

- Saved searches appear in the sidebar
- Saved searches are reachable through the command palette
- Selecting one changes the mail list to that query scope

## Why saved searches matter

Folders are not enough for high-volume mail. Saved searches let you treat search as a stable view:

- Waiting for reply
- Unread invoices
- Recruiter follow-up
- Production alerts
- Travel receipts
