---
title: Mailbox Workflow
description: How to navigate, triage, and work through mail in the mxr TUI.
---

## Mailbox model

The mailbox screen is the default TUI workspace:

- left: sidebar
- center: mail list
- right: preview, single-message, or thread view

The list is thread-first by default. `mxr` can also show message rows, but the default mode is one row per conversation.

## Sidebar

The sidebar is split into:

- system labels
- user labels
- saved searches

Selecting a label or saved search changes the active mailbox scope. The mail-list header reflects the current scope.

## Mail list

A thread row shows:

- unread/star state
- sender summary
- subject and snippet
- date metadata
- attachment marker when any message in the row has attachments
- thread count inline with distinct styling when the conversation has multiple messages

Long subjects are truncated so date and metadata remain visible.

## Open and close the right pane

- `Enter` or `o`: open selected row
- `Esc`: dismiss the right pane back to the two-pane layout
- `Tab`: switch panes
- `F`: toggle fullscreen

## Thread view

Inside a thread:

- `j` / `k` move the focused message
- reply, reply-all, forward, archive, label, snooze, and other actions target the focused message
- message headers show label chips and attachment metadata
- reader mode and attachment actions work from the focused message

## Search in the TUI

Two search flows exist:

- `/`: inline mailbox search
- dedicated Search page: global search workspace with results + preview

From Search:

- `Enter` or `o` opens the selected result into the normal mailbox/thread flow

## Selection and bulk actions

- `x`: toggle one row into selection
- `V`: visual line selection mode
- `Esc`: clear selection

When selection is active:

- hint bar switches into selection-aware actions
- command palette still works
- destructive or broad mutations use a confirmation modal first

Common bulk actions:

- archive
- trash
- spam
- mark read/unread
- star
- apply label
- move to label

## Attachments

When a message has attachments:

- the mail list shows an attachment marker
- the thread/message header shows attachment info
- `A` opens the attachment modal
- `Enter` / `o` opens the selected attachment
- `d` downloads it

## Snooze

`Z` opens the snooze modal with presets such as tomorrow morning, tonight, weekend, and next Monday.

Snoozing is local-first but also updates provider state where supported. For Gmail that means removing `INBOX` when snoozed and restoring it when the snooze wakes.

## Help and discovery

- `?`: exhaustive help modal
- `Ctrl-p`: command palette

The help modal is context-aware and the command palette exposes mailbox, search, rules, diagnostics, and account actions.
