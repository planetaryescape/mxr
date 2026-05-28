---
name: mxr-development
description: "Use when changing mxr source, architecture, docs, tests, IPC, daemon handlers, store/search/sync/semantic behavior, provider adapters, TUI/web clients, release flow, or repo-level development process."
---

# mxr Development

This skill holds the context that used to bloat always-on agent files. Load it only for source/docs/process work.

## Product shape

- Local-first: SQLite is canonical, search indexes are rebuildable, and core mail works offline.
- Daemon-backed: TUI, CLI, web, and scripts are clients over Unix-socket IPC.
- CLI-first: new capabilities land in CLI at the same time as daemon support, with stable JSON/JSONL.
- Provider-agnostic core: Gmail/IMAP/SMTP/Outlook behavior maps into the internal model in adapters.
- Compose uses `$EDITOR` with YAML frontmatter plus markdown body.
- Reader mode is plain text first; no inline images in the terminal rendering path.

## Build and verify loop

For feature work:

1. Implement in the narrowest crate surface.
2. Run `scripts/cargo-test -p <crate> --tests`.
3. Run `cargo build -p mxr`.
4. Restart stale daemons: `pkill -f 'mxr daemon' 2>/dev/null`, then `cargo run --bin mxr -- daemon --foreground`.
5. Drive the feature through the CLI, preferably with `--format json`.
6. Inspect daemon state with `mxr events`, `mxr logs`, `mxr doctor`, and `mxr activity`.
7. Query persisted state back through the CLI, such as `mxr search ... --format json`.

If the daemon will not start, check `cargo build -p mxr`, `pgrep -af 'mxr|cargo'`, `mxr daemon --foreground`, and stale socket files.

## Client and mutation contract

- TUI and CLI must use the same daemon request; avoid client-only capabilities.
- Every reusable daemon capability needs a CLI surface. TUI/web support should layer on it.
- Destructive or batch mutations need a dry-run/preview path before commit.
- Dry-run selection/query logic must match the real mutation path.
- Read/list/status/search/export surfaces must keep structured output pipeable.

## Crate boundaries

- `core` depends on no internal crates.
- `protocol` depends only on `core`.
- Provider crates depend on `core` plus shared mail utility crates such as `mail-parse` and `outbound`.
- `store` and `search` depend only on `core`.
- `semantic` owns embeddings/dense retrieval and must not depend on daemon, TUI, or provider crates.
- `llm` owns provider clients and prompt/result DTOs; higher layers pass plain data in.
- `relationship` may depend on `core`, `store`, `reader`, and `llm`; not protocol/daemon/client/sync/search/semantic/provider crates.
- `safety` may depend on `core`, `reader`, and `relationship`; not store/protocol/daemon/clients/sync/search/semantic/provider crates.
- `sync` depends on `core`, `store`, and `search`.
- `daemon` is the integration point, but talks to providers only through `MailSyncProvider` and `MailSendProvider`.
- `tui` and `web` are clients; they must not depend on daemon, store, search, sync, semantic, or provider crates.
- Use Cargo dependencies for architecture seams, never `#[path]`.

See `docs/blueprint/01-architecture.md` for longer rationale.

## Domain invariants

- Semantic search is optional platform behavior layered above core mail runtime.
- Sync stores envelopes/bodies and commits lexical search before a sync batch finishes.
- Semantic chunks may be persisted during sync, but embeddings/ANN refresh only happen when semantic is enabled or explicitly reindexed.
- Hybrid search keeps BM25 exactness and adds dense recall with RRF.
- Activity log rows are local-only personal data. Never sync or transmit them, never store credentials/full bodies/attachment bytes, and write only through `state.activity.record(...)`.
- `INSERT OR REPLACE` can trigger `ON DELETE CASCADE`; prefer `INSERT ... ON CONFLICT UPDATE` for parent rows with dependents.
- `mxr reset --hard` / `mxr burn` wipe runtime state only by default; preserve config and credentials unless explicitly requested.

## Release shorthand

If the user says `ship it`, run the full release flow:

1. Commit release-ready changes.
2. If the version/tag exists, bump first; never overwrite tags or GitHub releases.
3. Push `main`.
4. Create and push `v{version}`.
5. Wait for the tag-driven release workflow.
6. Verify Homebrew and `cargo install --git ... --tag v{version} --locked mxr`.

Source of truth: `docs/blueprint/17-release-pipeline.md` and checked-in GitHub workflows.

## Useful docs

- `docs/blueprint/` - requirements, architecture, decisions.
- `docs/blueprint/15-decision-log.md` - settled decisions.
- `docs/activity-log.md` - activity table lifecycle and privacy rules.
- `docs/implementation-journey.md` - historical context and superseded plans.
