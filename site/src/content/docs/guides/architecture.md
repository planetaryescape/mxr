---
title: Architecture
description: Why mxr is daemon-backed, local-first, and built around one internal mail model.
---

mxr is a local mail runtime. Its clients share daemon-backed mail operations over a local store, search index and provider adapters.

## The short version

```text
TUI / CLI / web / MCP / scripts
              |
              v
            daemon
       /       |       \
   SQLite   Tantivy   provider adapters
 canonical  rebuildable       |
                              v
                         mail providers
```

The daemon coordinates shared local mail state and operations. SQLite is
canonical; Tantivy is rebuildable from it. Provider adapters map provider
mail into mxr's internal model and handle provider I/O. Clients can reuse
daemon operations without duplicating provider behavior, while exposing
different capabilities and authority.

## IPC buckets

mxr keeps one flat IPC wire format, but the contract is easier to maintain if you sort it mentally into four buckets:

1. `core-mail`
2. `mxr-platform`
3. `admin-maintenance`
4. `client-specific`

Only the first three belong in daemon IPC.

- `core-mail`: search, sync, envelopes, bodies, threads, labels, drafts, send, mutations, attachments, export
- `mxr-platform`: accounts, rules, saved searches, subscriptions, semantic runtime
- `admin-maintenance`: status, events, logs, doctor, bug reports, local reset, repair/inspection
- `client-specific`: sidebar grouping, pane state, right-rail shaping, selection state; keep this in TUI/web/CLI layers

The daemon serves reusable truth and workflows, not screen payloads.

## Why it is shaped this way

### Local-first

Mail should stay useful when the network is flaky, when you want to script against it, and when you do not want another hosted layer in the middle.

### Daemon-backed

The TUI should not be the whole system. Background sync, indexing, snooze, and rules should keep running without an open UI.

### One model in the middle

Gmail labels, IMAP folders, flags, drafts, and bodies all have to meet the same internal model. That is what keeps the rest of the codebase sane.

### Search as navigation

Search is not a fallback. It is how power users move through a mailbox. That is why mxr uses Tantivy and saved searches instead of treating search as an afterthought.

### `$EDITOR` for writing

Compose opens the editor you already use. mxr handles parsing, reply headers, markdown-to-multipart conversion, and sending.

## Principles

1. Local-first
2. Provider-agnostic internal model
3. Daemon-backed architecture
4. `$EDITOR` for writing
5. Fast search is first-class
6. Saved searches are a core primitive
7. Rules are deterministic first
8. Shell hooks over plugin systems
9. Adapters are swappable
10. Correctness beats cleverness

## Read more

- Root summary: [ARCHITECTURE.md](https://github.com/planetaryescape/mxr/blob/main/ARCHITECTURE.md)
- Full blueprint: [docs/blueprint/README.md](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/README.md)

## Stored asks share one answer across clients

The deterministic asks facet covers received and sent mail. It stores
who acts, what they were asked to do, document state and authored source
spans. To do, reply turns, waits and relationship commitments project that
answer. Home placement remains an inbound-only facet; both facets commit
under the same thread-generation check. Missing or stale asks stay pending.
No model decides this facet.
