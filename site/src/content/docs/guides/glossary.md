---
title: Glossary
description: Plain-language definitions for the terms scattered across mxr docs and code. One paragraph each, plus a link to where the concept lives.
---

mxr borrows vocabulary from email standards, Gmail, Hey, and IMAP. This page reconciles them so you don't have to guess.

## Architecture

**Daemon** — the background process that owns the SQLite store, the search index, the network connections, and the IPC socket. The TUI, the CLI, and the HTTP bridge are all clients of the same daemon. `mxr` autostarts it; `mxr daemon` runs it explicitly. See the [architecture guide](/guides/architecture/).

**Adapter** — the per-provider code that translates mxr's internal model to and from a provider's API or wire protocol. Today: `provider-gmail`, `provider-imap`, `provider-smtp`, `provider-fake`. New adapters live in their own crate and pass a [conformance test suite](/reference/conformance/).

**Internal model** — mxr's provider-agnostic types: `Envelope`, `Thread`, `Account`, `Address`, `Label`. Adapters map _into_ this model so application code never speaks Gmail-specific or IMAP-specific dialects.

## IPC buckets

Every IPC request the daemon serves falls into one of four buckets. The buckets are conceptual, not separate sockets — but they shape what's stable, what's user-facing, and what's an internal hatch.

- **`core-mail`** — read mail, send mail, mutate mail. The settled, stable surface.
- **`mxr-platform`** — accounts, rules, saved searches, subscriptions, semantic runtime. mxr-product features that aren't strictly _mail_.
- **`admin-maintenance`** — status, events, logs, doctor, bug reports, local reset, repair. Diagnostic and operational.
- **`client-specific`** — pane-shape, view-model, screen-state. Not part of the daemon contract; clients (TUI, web, desktop) shape these themselves.

See [Architecture](/guides/architecture/) for why this split matters.

## Account state

**Config-backed account** — an entry in `config.toml` under `[accounts.<key>]`. IMAP/SMTP accounts and Gmail-with-BYOC live here. Editable from the TUI's Accounts page or by hand.

**Runtime account** — what the daemon actually has connected. May include accounts that are config-backed (the common case) plus runtime-only entries (e.g. browser-auth Gmail sessions that don't have a TOML row). `mxr accounts` shows runtime; `mxr config show` shows config.

**Owned address** — a verified address belonging to an account, used for direction inference (inbound vs outbound). Manage with `mxr accounts addresses`.

## Search

**Lexical search** — Tantivy BM25 only. Exact, fast, deterministic. The default mode.

**Semantic search** — dense retrieval using local embeddings. Useful when you don't know the keywords. Disabled by default; enable in `[search.semantic] enabled = true`. Runs entirely on-device.

**Hybrid search** — lexical + semantic, fused with reciprocal-rank fusion. Best recall; the keyword-aware default for "I want it to find what I mean."

**Saved search** — a named, queryable inbox lens. Lives in the sidebar. Run from the CLI with `mxr saved run <name>`.

## Mutation flow

**Mutation** — any operation that changes provider state (archive, trash, spam, label, snooze, send, unsend, unsubscribe). All support `--dry-run`; see the [automation contract](/guides/automation-contract/).

**Mutation ID** — a short token printed by archive/trash/spam/read/read-archive. Copy it and pass to `mxr undo MUTATION_ID` within ~60 seconds to revert.

**Reply-later** (a.k.a. **bookmark**) — flag a thread to come back to. Hey's term is "Reply Later"; in the TUI the key is `b` for bookmark. The reply queue is browsed with `mxr replies` or `Ctrl-p → Reply Queue`.

**Snooze** — hide a thread until a time. Returns to inbox at the specified moment. Set with `mxr snooze --until '<grammar>'` or the `Z` key in the TUI.

**Screener** — Hey-borrowed term for triaging unknown senders into Allow / Deny / Feed / Paper Trail. Local-only consent metadata; never round-trips to the provider. CLI: `mxr screener`.

## Display

**Reader mode** — strips signatures, quoted text, tracking pixels, and remote-image references for distraction-free reading. Toggle with `R`.

**Plain text first** — mxr renders text/plain bodies if they exist, falling back to HTML→text only when needed. Remote content is off by default.

## Provider quirks (the seam)

**Label** — Gmail's primary classification primitive. A message can have many labels.

**Folder** — IMAP's primary classification primitive. A message lives in exactly one folder. mxr's `Label` type carries a `LabelKind::Folder` variant so IMAP folders don't get flattened into Gmail-style labels — preserving the semantic difference is important. `mxr move` is the verb that operates on folder semantics; `mxr label` is for label-style multi-membership.

**Provider ID** — the provider's own message identifier. Gmail's is stable; IMAP's is mailbox-scoped and may change across moves/copies. mxr stores it on `Envelope.provider_id` for round-trips back to the provider.

## Process state

**Sync** — pulling new mail and applying remote changes locally. Triggered automatically every `[general] sync_interval` seconds, or on demand with `mxr sync`.

**Indexing** — populating Tantivy from new SQLite records. Always runs as part of sync; lexical search is fresh as soon as a sync batch commits. Semantic chunks are persisted but not embedded unless `[search.semantic] enabled = true`.

**Reset / Burn** — `mxr reset --hard` and its alias `mxr burn` destroy local runtime state (the daemon, the database, the search index). They preserve `config.toml` and credentials by default. Use `--including-config` to also drop the config; pair with `--dry-run` to preview.

## See also

- [Architecture](/guides/architecture/) — the daemon-and-clients model in depth
- [CLI concepts](/reference/cli/concepts/) — query operators, search modes, output formats
- [Automation contract](/guides/automation-contract/) — the scriptable surface
- [Decision log](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/15-decision-log.md) — D001-D048, why mxr is the way it is
