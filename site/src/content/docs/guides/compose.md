---
title: Compose
description: Compose, reply, reply-all, and forward through $EDITOR.
---

## Core model

`mxr` writes drafts in your editor. The daemon handles parsing, validation, send, save-draft, and provider delivery.

This applies to:

- new compose
- reply
- reply-all
- forward
- draft editing

## CLI

```bash
mxr compose
mxr compose --to alice@example.com --subject "hello"
mxr reply MESSAGE_ID
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID --to team@example.com
mxr drafts
mxr send DRAFT_ID
```

## TUI

- `c`: compose
- `r`: reply
- `a`: reply all
- `f`: forward

If you start from a thread view, reply actions target the focused message, not just the latest message in the thread.

## Draft format

Drafts use YAML frontmatter plus body text:

```md
---
to:
  - alice@example.com
cc: []
bcc: []
subject: Example
---

Hello from mxr.
```

## Reply context

Reply and forward drafts include message context. If the original message only had HTML, `mxr` uses the rendered reader output, not raw HTML tags.

## Send confirmation

After the editor closes, `mxr` shows a confirmation modal:

- changed draft: send, save draft, edit again, discard
- unchanged draft: edit again or discard

## Account selection

The sender address comes from the selected/default runtime account, not from a static status snapshot. This matters for multi-account setups.

## Attachments

CLI compose supports:

```bash
mxr compose --attach ./invoice.pdf --attach ./notes.txt
```

TUI message viewing supports attachment open/download. Compose-side attachment management remains editor and CLI oriented.
