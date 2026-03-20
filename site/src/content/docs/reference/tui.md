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

- sidebar
- mail list
- message or thread pane

Behavior:

- thread-first list by default
- optional message-list mode
- focused-thread-message targeting for reply and mutations
- explicit right-pane dismissal with `Esc`
- bulk selection with confirmation modals

## Search screen

- query input
- result list
- preview pane
- `Enter` or `o` opens the selected result into the normal mailbox flow

## Rules screen

- rule list on the left
- details, history, dry-run, or form panel on the right
- form-driven create/edit for supported rule fields

## Diagnostics screen

- status summary
- doctor output
- recent events
- recent logs
- bug-report generation

## Accounts screen

- runtime account inventory
- add IMAP/SMTP account
- test connectivity
- set default account
- inspect runtime-only accounts such as browser-auth Gmail setups

## Modals and overlays

- command palette
- help modal
- label picker
- compose confirmation
- bulk confirmation
- attachment modal
- snooze modal

## Mailbox semantics

- actions in thread view target the focused message
- labels appear in message headers
- attachment indicators appear in the mail list and message/thread header
- thread counts are styled separately from sender names
- label and saved-search scopes drive the mail list header and query state

## Discovery model

- command palette: broad action surface
- help modal: context-aware keybinding reference
- hint bar: context-sensitive shortcuts, including selection-aware actions
