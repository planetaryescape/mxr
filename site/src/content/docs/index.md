---
title: mxr
description: Local-first terminal email. Daemon-backed, keyboard-native, searchable, scriptable.
---

## What mxr is

`mxr` is a terminal email client built around one local engine:

- SQLite is the canonical mail store.
- Tantivy provides fast search over synced mail.
- The daemon owns sync, rules, snooze wakeups, logs, and indexing.
- The TUI and CLI are both clients of that daemon.
- Compose happens in `$EDITOR`, not in a tiny built-in text box.

## Philosophy

### Local-first

Your mail lives on your machine. Search is rebuildable from SQLite. You can read and work offline.

### Daemon-backed

The daemon is the system. The TUI is not the system. The CLI is not the system. That separation is what makes background sync, shell automation, and multiple frontends possible.

### Provider-agnostic core

Gmail, IMAP, and future providers map into one internal model. Core logic should not care where a message came from.

### Search as navigation

Search is not a bolt-on filter. It is one of the primary navigation models. Saved searches are first-class sidebar entries.

### `$EDITOR` for writing

`mxr` does not compete with your editor. Compose, reply, reply-all, and forward all open an editable draft with frontmatter plus body.

### Deterministic automation first

Rules are data first: inspectable, dry-runnable, auditable, replayable. Shell hooks exist as an escape hatch, not the foundation.

### Plain text first

HTML mail is rendered into readable text. Reader mode strips quotes, signatures, and boilerplate. The browser is the escape hatch when you need the original rich view.

## Architecture

```text
TUI / CLI / scripts  <->  daemon  <->  SQLite + Tantivy  <->  providers
```

- TUI screens: mailbox, search, rules, diagnostics, accounts
- CLI commands: search, thread, export, labels, rules, events, logs, doctor, bug-report, compose, mutations, attachments
- Providers: Gmail, IMAP, SMTP, fake provider for tests

## What makes mxr different

- daemon-backed instead of monolithic
- SQLite store instead of direct-online-only mailbox views
- Tantivy search instead of weak folder-only navigation
- saved searches as a core primitive
- reply/compose in `$EDITOR`
- deterministic rules with dry-run and history
- export for markdown, json, mbox, and LLM context

## Start here

1. Read [Installation](/getting-started/install/).
2. If you use Gmail, follow [Gmail Setup](/getting-started/gmail-setup/).
3. Walk through [First Sync](/getting-started/first-sync/).
4. Learn the [Mailbox Workflow](/guides/mailbox/) and [Keybindings](/reference/keybindings/).

## Current product surface

- TUI: thread-first mailbox, dedicated search page, rules page, diagnostics page, accounts page, command palette, attachment modal, help modal, bulk actions, snooze, labels
- CLI: full daemon-backed search, export, labels, rules, observability, attachments, compose, reply, forward, saved searches, account operations
- Runtime: background sync, logs, events, doctor checks, bug-report generation, rule execution, snooze wake loop
