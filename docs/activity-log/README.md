# mxr — Activity Log

Local-first, append-only, queryable log of **user-initiated actions** across TUI, CLI, and web. The git-reflog of your inbox.

This is **not** an email-event log (incoming mail, sync, rules firing). That's `event_log`. This captures what *you* did: read, searched, archived, clicked, drafted, sent.

## Why this exists

Email clients don't ship a user-visible activity log. Gmail shows IPs. Workspace admins get compliance dumps. Nobody gives the end user a browseable "what did I do yesterday?" surface. That's the whitespace mxr fills. See [00-overview.md](./00-overview.md#differentiation) for the framing.

## Read order

1. [00-overview.md](./00-overview.md) — context, locked decisions, architecture, principles. **Always read first.**
2. [01-schema-and-taxonomy.md](./01-schema-and-taxonomy.md) — DB schema + action taxonomy + context JSON shapes.
3. [02-storage.md](./02-storage.md) — Phase 1: migration + `crates/store/src/user_activity.rs`.
4. [03-capture.md](./03-capture.md) — Phase 2: dispatcher instrumentation, recorder, mapper, client-source plumbing.
5. [04-query-ipc.md](./04-query-ipc.md) — Phase 3: IPC verbs and filter struct.
6. [05-cli.md](./05-cli.md) — Phase 4: `mxr activity` subcommand tree.
7. [06-tui.md](./06-tui.md) — Phase 5: TUI activity screen and keybinds.
8. [07-web.md](./07-web.md) — Phase 6: bridge routes + React page.
9. [08-privacy.md](./08-privacy.md) — Phase 7: retention, redaction, pause, clear.
10. [09-power-features.md](./09-power-features.md) — Phase 8: saved filters, recall, replay, stats.
11. [10-performance-polish.md](./10-performance-polish.md) — Phase 9: FTS5, indices, compaction, vacuum, docs polish.
12. [APPENDIX-action-catalog.md](./APPENDIX-action-catalog.md) — canonical action token catalog with tier + emitters.
13. [APPENDIX-context-schemas.md](./APPENDIX-context-schemas.md) — context_json JSON-schemas per action.

[STATUS.md](./STATUS.md) tracks progress — update as work lands.

## Working rules

- Every phase ships user-visible value or measurable infrastructure. No half-finished phases.
- Decisions in [00-overview.md](./00-overview.md#locked-decisions) are locked. Don't re-debate.
- Integration tests against a running daemon, not just unit tests with fakes. Reuse `provider-fake`.
- Wire TUI **and** CLI together when adding daemon features. Web follows.
- Activity write failures **never** propagate to user-facing actions. This is observability, not correctness.
- Activity **never** leaves the user's device. No sync. No telemetry. Hard invariant.
- Reuse existing patterns: `EventSource` enum, `event_retention_days` config, `AdminMaintenance` IPC bucket, dynamic-SQL filter style from `event_log.rs`.

## Quick map to existing code

| Concept | Existing reference to mirror |
|---|---|
| Migration style | `crates/store/migrations/006_message_events.sql` |
| Repo module | `crates/store/src/event_log.rs` (dynamic filter SQL) |
| Retention pattern | `crates/daemon/src/commands/logs.rs::prune_events()` |
| IPC bucket | `AdminMaintenance` (`crates/protocol/src/types.rs:10-18`) |
| Dispatcher seam | `crates/daemon/src/handler/mod.rs:263-287` |
| Source enum | `EventSource::User` already used in `handler/mutations.rs` |
| Web bridge route | `crates/web/src/routes_v6.rs:92-100` (events route) |
| CLI subcommand style | `crates/daemon/src/cli/mod.rs` (clap-based) |
