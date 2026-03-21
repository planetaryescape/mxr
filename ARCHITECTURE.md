# Architecture

mxr is built as local-first email infrastructure.

That phrase matters. This project is not just a terminal UI. It is not just a Gmail script wrapper either. The daemon, local store, search index, CLI, TUI, and adapter model all come from the same decision: email should stay useful even when the browser is closed, the network is flaky, or you want to script against your own data.

For the full design record, read [docs/blueprint/README.md](docs/blueprint/README.md). This file is the short version.

## Core shape

```text
TUI / CLI / scripts / agents
          |
          v
        daemon
       /      \
   SQLite    Tantivy
       \
   provider adapters
```

The daemon is the system. Clients talk to it over a Unix socket. SQLite is the source of truth. Tantivy is rebuildable from SQLite. Providers map into the internal model instead of leaking their own shapes into the rest of the app.

## Principles

### 1. Local-first

Your email lives on your machine. SQLite is the canonical store. Search is local after sync. If the network disappears, your already-synced mail still works.

### 2. Provider-agnostic internal model

Core code speaks one model. Gmail, IMAP, and anything else have to map into that model instead of dragging provider-specific assumptions through the codebase.

### 3. Daemon-backed architecture

The TUI is a client. The CLI is a client. Scripts are clients. That keeps sync, indexing, snooze, and automation alive even when the UI is closed.

### 4. `$EDITOR` for writing

mxr does not try to replace the text editor you already like. Compose opens `$EDITOR`, uses markdown plus frontmatter, and lets the daemon handle the mail-specific plumbing.

### 5. Search is a first-class feature

Search is not the backup navigation path. It is the main one. That is why the project uses Tantivy instead of a folder-only mental model.

### 6. Saved searches are part of the product

Saved searches are not a bolt-on filter. They are user-defined inbox views and a big part of how the app is meant to feel.

### 7. Rules are deterministic first

Rules need to be inspectable, replayable, and dry-runnable before they are clever. Trust comes before ambition.

### 8. Shell hooks beat a premature plugin system

mxr prefers piping structured data to normal shell commands over inventing a big extension framework early.

### 9. Adapters are swappable

Provider code stays in adapter crates. If one provider changes, the core should not have to.

### 10. Correctness beats cleverness

Plain Rust. Explicit errors. Compile-time checked SQL. Boring is a feature here.

## What this means in practice

- The CLI is broad because it is the safest interface for scripts and agents today.
- The TUI is thin on purpose because the daemon owns the state.
- The search index can be rebuilt because SQLite is the source of truth.
- The adapter boundary matters because the project wants to outlive any one provider.
- The trust boundary matters because email is personal, messy, and easy to over-centralize.

## What mxr is not trying to be

- Not a hosted email middleware layer
- Not a browser in your terminal
- Not a giant plugin framework
- Not an AI product first

The project can still connect to agent workflows. It just does that from a local runtime instead of starting from a hosted control plane.
