---
title: TUI
description: Reference for screens, panes, modals, and interaction model in the mxr terminal UI.
---

## Screens

The TUI has five top-level screens:

- Mailbox
- Search
- Rules
- Accounts
- Diagnostics

Open them with `1`-`5` or from the command palette with `Ctrl-p`.

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

- Fixed query input at the top
- Result list on the left
- Preview pane on the right
- Live search against the full local index
- `Ctrl-f` is separate and only filters the current mailbox
- `Enter`, `o`, or `l` opens the selected result in preview
- `Esc` moves preview -> results -> mailbox

## Rules screen

- Rule list on the left
- Guided workspace on the right
- Overview, history, dry-run, and edit states
- Textarea-driven condition/action editing for supported rule fields

## Diagnostics screen

- Status summary
- Doctor output
- Recent events
- Recent logs
- Bug-report generation
- Config edit and log-open shortcuts from inside the page

## Accounts screen

- Details on the left
- Runtime account list on the right
- Add IMAP/SMTP account
- Test connectivity
- Set default account
- Edit config without leaving the TUI
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

- First-run onboarding walkthrough
- Command palette: broad action surface
- Help modal: context-aware keybinding reference
- Hint bar: context-sensitive shortcuts, including selection-aware actions
- `gc`: edit config globally
