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

SQLite is the source of truth. Tantivy is rebuildable from SQLite. Provider adapters map Gmail and IMAP into the internal model instead of leaking provider semantics upward. Provider-agnostic at the app layer does not mean flattening away real differences: labels-vs-folders is the main seam, and threading uses native IDs when available plus reconstruction otherwise.

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
   Status, events, logs, doctor, bug-report, local reset, repair/inspection surfaces.
   These stay in IPC, but are conceptually fenced off from the core mail contract.
4. `client-specific`
   Pane state, selection state, sidebar collapse, grouped rows, right-rail payloads, widget-specific shaping.
   These stay in clients, not in the daemon.

Daemon rule: serve reusable truth and workflows, not screen payloads.

Provider rule: provider weirdness is handled below this layer in adapter crates, but capability differences stay visible where behavior actually differs.

## Semantic retrieval

Semantic search is an `mxr-platform` feature, not a core mail requirement.

- mail still fundamentally works without semantic retrieval
- embeddings stay local
- sync may prepare semantic chunks even while semantic retrieval is disabled
- embedding generation happens only when semantic is enabled
- hybrid search keeps lexical BM25 and fuses in dense recall with RRF
- fielded dense queries intentionally respect chunk source kinds
- OCR is not part of active semantic indexing

That boundary matters. Do not blur exact lexical behavior and semantic recall into one fuzzy system.

## Activity log

User-initiated actions are recorded in `user_activity`, captured at the daemon's IPC dispatch seam (`crates/daemon/src/handler/mod.rs::handle_request`). Every IPC request that mutates state or expresses user intent (search, view, mutation, draft action) produces one row, tagged with the originating client (`tui` | `cli` | `web` | `daemon`).

Storage: dedicated table + FTS5 mirror over `context_json`. Append-only; redaction is a tombstone, never a hard delete. Retention is tier-aware (30 / 90 / 365 days for ephemeral / standard / important by default, configurable per tier in `[activity.retention]`).

Capture seam: a single `Recorder` writes through a bounded mpsc channel. Failures are observability-only and never propagate to user-facing responses. The list of capturable IPC verbs lives in `crates/daemon/src/activity/mapper.rs` — explicit per-variant mapping for the ~40 user-intent verbs; everything else returns `None` with a `tracing::debug!` for visibility. New IPC verbs default to "not captured" until someone decides what to log; this keeps the table from accumulating noise as the protocol grows.

Compaction: write-time coalescing folds rapid-fire duplicates (same `action+target_id` within 250 ms) into a single row with an incremented `count`. Applies only to `ephemeral`- and `standard`-tier rows; `important`-tier mutations are always written as-is to preserve audit fidelity.

Query: `AdminMaintenance` IPC bucket with verbs `ListActivity` / `CountActivity` / `ActivityStats` / `ExportActivity` / `RedactActivity` / `PruneActivity` / `PauseActivity` / `ResumeActivity`. CLI: `mxr activity` (alias `mxr act`). TUI: `g a` chord. Web: `/activity` route.

Invariants:
1. Never transmitted off-device. No sync, no telemetry, no remote logging.
2. `context_json` never holds credentials, tokens, password hashes, attachment bytes, or full mail bodies.
3. `MXR_ACTIVITY=off` disables the recorder for the lifetime of the daemon. `mxr activity pause` is the runtime equivalent.
4. The recorder is the only path that writes to `user_activity`.

Detailed implementation plan: [`docs/activity-log.md`](./docs/activity-log.md). User-facing privacy guide: [`site/.../guides/activity-log.md`](./site/src/content/docs/guides/activity-log.md).

## Lifecycle guarantees

Current runtime story:

1. sync writes envelopes + bodies to SQLite immediately
2. sync updates Tantivy immediately and commits lexical freshness per batch
3. sync maintains labels, counts, threading, and cursor state
4. daemon post-sync work persists semantic chunks for the newly upserted messages
5. embedding generation + ANN refresh happen only when semantic is enabled or explicitly reindexed/profile-switched

Repair boundary:

- lexical search is repairable from SQLite at daemon startup
- semantic readiness is optional platform state layered on top

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
- The product/install/package surface is the repo-root package `mxr`.
- Internal crates under `crates/` are real workspace crates and are private by default (`publish = false`).
- The IMAP adapter depends on the published `mxr-async-imap` fork from crates.io; vendored source is not part of the workspace boundary model.
- Architectural seams are enforced with Cargo dependencies. `#[path]` pseudo-crates are not allowed.

## What this means in practice

- CLI, TUI, and web should reuse daemon workflows instead of inventing separate mail logic.
- Web/TUI should shape their own views from reusable daemon data.
- Providers may use shared mail utility crates like `mail-parse` and `outbound`, but never `compose`.
- Clients may use local utility crates like `config`, `compose`, `reader`, and `mail-parse`, but they must not depend on daemon/store/search/sync/provider crates.
- Search/status/doctor/events are all available over IPC, but only mail workflows define the core contract.
- `mxr reset --hard` / `mxr burn` are CLI-only maintenance commands that wipe rebuildable local runtime state while preserving config and credentials by default.
- Future contributors should classify new IPC first, then add it. Do not grow a junk drawer.
