---
title: TUI
description: Reference for screens, panes, modals, and interaction model in the mxr terminal UI.
---

## Screens

The TUI has five top-level screens:

- Mailbox
- Search
- Rules
- Diagnostics
- Accounts

Open them from the command palette with `Ctrl-p`.

## Mailbox screen

Layout:

- Sidebar
- Mail list
- Message or thread pane

Behavior:

- Thread-first list by default
- Optional message-list mode
- Focused-thread-message targeting for reply and mutations
- Explicit right-pane dismissal with `Esc`
- Bulk selection with confirmation modals

## Search screen

- Query input
- Result list
- Preview pane
- `Enter` or `o` opens the selected result into the normal mailbox flow

## Rules screen

- Rule list on the left
- Details, history, dry-run, or form panel on the right
- Form-driven create/edit for supported rule fields

## Diagnostics screen

- Status summary
- Doctor output
- Recent events
- Recent logs
- Bug-report generation

## Accounts screen

- Runtime account inventory
- Add IMAP/SMTP account
- Test connectivity
- Set default account
- Inspect runtime-only accounts such as browser-auth Gmail setups

## Modals and overlays

- Command palette
- Help modal
- Label picker
- Compose confirmation
- Bulk confirmation
- Attachment modal
- Snooze modal

## Mailbox semantics

- Actions in thread view target the focused message
- Labels appear in message headers
- Attachment indicators appear in the mail list and message/thread header
- Thread counts are styled separately from sender names
- Label and saved-search scopes drive the mail list header and query state

## Discovery model

- Command palette: broad action surface
- Help modal: context-aware keybinding reference
- Hint bar: context-sensitive shortcuts, including selection-aware actions
