# Architecture

mxr is local-first email infrastructure. The daemon is the system. TUI, CLI, web, scripts, and agents are clients.

For the full design record, read [docs/blueprint/README.md](docs/blueprint/README.md). This file is the short version.

## Core shape

```text
TUI / CLI / web / scripts / agents
               |
               v
             daemon
          /     |     \
      SQLite  Tantivy  runtime features
               |
         provider adapters
```

SQLite is the source of truth. Tantivy is rebuildable from SQLite. Provider adapters map Gmail and IMAP into the internal model instead of leaking provider semantics upward.

## IPC contract

Transport stays simple: length-delimited JSON over a Unix socket using `IpcMessage { id, payload }`.

The contract has four buckets:

1. `core-mail`
   Search, sync, envelopes, bodies, threads, labels, drafts, send, mutations, attachments, export.
   This is the most stable bucket.
2. `mxr-platform`
   Accounts, rules, saved searches, subscriptions, semantic runtime/profile management.
   These are real mxr product/runtime features, not mail timelessness and not client-only convenience.
3. `admin-maintenance`
   Status, events, logs, doctor, bug-report, repair/inspection surfaces.
   These stay in IPC, but are conceptually fenced off from the core mail contract.
4. `client-specific`
   Pane state, selection state, sidebar collapse, grouped rows, right-rail payloads, widget-specific shaping.
   These stay in clients, not in the daemon.

Daemon rule: serve reusable truth and workflows, not screen payloads.

Provider rule: provider weirdness is handled below this layer in adapter crates.

## Principles

1. Local-first
2. Provider-agnostic internal model
3. Daemon-backed architecture
4. `$EDITOR` for writing
5. Search is first-class
6. Saved searches are product primitives
7. Rules are deterministic first
8. Shell hooks over premature plugin systems
9. Adapters are swappable
10. Correctness beats cleverness

## Repo reality

- First-party adapters are live for Gmail, IMAP, SMTP, and Fake.
- `crates/web` is a current client/bridge, not future work.
- The repo is one Cargo package named `mxr` that path-mounts conceptual subcrates under `crates/`.
- Some older blueprint prose still describes a cleaner future workspace split; when docs disagree with code, code wins.

## What this means in practice

- CLI, TUI, and web should reuse daemon workflows instead of inventing separate mail logic.
- Web/TUI should shape their own views from reusable daemon data.
- Search/status/doctor/events are all available over IPC, but only mail workflows define the core contract.
- Future contributors should classify new IPC first, then add it. Do not grow a junk drawer.
