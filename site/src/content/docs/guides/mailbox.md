---
title: Mailbox workflow
description: How to navigate, triage, and work through mail in the mxr TUI.
---

## Mailbox model

The mailbox screen is the default TUI workspace:

- Left: sidebar
- Center: mail list
- Right: preview, single-message, or thread view

The list is thread-first by default. mxr can also show message rows, but the default is one row per conversation.

## Sidebar

The sidebar has three sections:

- System labels
- User labels
- Saved searches

Selecting a label or saved search changes the active mailbox scope. The mail-list header reflects the current scope.

## Mail list

A thread row shows:

- Unread/star state
- Sender summary
- Subject and snippet
- Date metadata
- Attachment marker when any message in the row has attachments
- Thread count with distinct styling when the conversation has multiple messages

Long subjects are truncated so date and metadata remain visible.

## Open and close the right pane

- `Enter` or `o`: open selected row
- `Esc`: dismiss the right pane back to the two-pane layout
- `Tab`: switch panes
- `F`: toggle fullscreen

## Thread view

Inside a thread:

- `j` / `k` move the focused message
- Reply, reply-all, forward, archive, label, snooze, and other actions target the focused message
- Message headers show label chips and attachment metadata
- Reader mode and attachment actions work from the focused message

## Search in the TUI

Two search flows:

- `/`: inline mailbox search
- Dedicated Search page: global search workspace with results + preview

From Search:

- `Enter` or `o` opens the selected result into the normal mailbox/thread flow

## Selection and bulk actions

- `x`: toggle one row into selection
- `V`: visual line selection mode
- `Esc`: clear selection

When selection is active:

- Hint bar switches to selection-aware actions
- Command palette still works
- Destructive or broad mutations use a confirmation modal first

Common bulk actions:

- Archive
- Trash
- Spam
- Mark read/unread
- Star
- Apply label
- Move to label

## Attachments

When a message has attachments:

- The mail list shows an attachment marker
- The thread/message header shows attachment info
- `A` opens the attachment modal
- `Enter` / `o` opens the selected attachment
- `d` downloads it

## Snooze

`Z` opens the snooze modal with presets such as tomorrow morning, tonight, weekend, and next Monday.

Snoozing is local-first but also updates provider state where supported. For Gmail that means removing `INBOX` when snoozed and restoring it when the snooze wakes.

## Help and discovery

- `?`: help modal with all keybindings
- `Ctrl-p`: command palette

The help modal is context-aware. The command palette exposes mailbox, search, rules, diagnostics, and account actions.
