# 00 — Overview

Always read this first when picking up activity-log work.

## What we're building

A user-facing, queryable, retention-bounded log of every action the user intentionally takes in mxr — across TUI, CLI, and web. Captured at the daemon's IPC dispatcher so all three clients are covered with one instrumentation point. Stored in a dedicated SQLite table. Queryable via a new `AdminMaintenance` IPC surface and a `mxr activity` subcommand tree. Browsable in the TUI and web.

This is **not**:
- A log of incoming email or sync events (that's `event_log`).
- A log of per-message state transitions (that's `message_events`).
- Telemetry sent to mxr or anyone else.

## Differentiation

The trade-off test: most email clients differentiate on *"smart inbox, AI triage, we decide for you."* mxr can credibly claim the opposite — **your inbox remembers what *you* did, not what it decided for you.**

Framing for users: *"Email diary, not surveillance. Local-only, append-only, queryable like `git reflog` for your inbox, shreddable on demand."*

Whitespace confirmed by research: Gmail/Workspace expose admin-only audit (compliance, 7-day default). Superhuman, Hey, Fastmail, Proton, Spark, Mimestream, Thunderbird have no documented end-user activity log. Adjacent patterns we steal from: Notion Updates, Linear activity feed, 1Password audit log, browser history (Arc/Chrome), ActivityWatch (local-first).

## Locked decisions

Do not re-debate. If something here genuinely needs to change, update this doc and call it out in the PR description.

1. **Naming**: table `user_activity`; CLI surface `mxr activity` (alias `mxr act`); IPC verbs `ListActivity`, `CountActivity`, `ActivityStats`, `ExportActivity`, `RedactActivity`, `PruneActivity`, `PauseActivity`, `ResumeActivity` — all under the `AdminMaintenance` bucket.
2. **Storage**: a new dedicated table. Do **not** extend `event_log` (system diagnostics, different schema) or `message_events` (per-message state, keyed by message id).
3. **Schema shape**: append-only single table + FTS5 virtual table over `context_json`. Redaction is a tombstone (`redacted=1`, `context_json=NULL`) — never a hard delete. Retention prune *is* a hard delete and runs daily.
4. **Action tiers**: `ephemeral` | `standard` | `important`. Default retention 30 / 90 / 365 days respectively. All three configurable per-tier.
5. **Capture point**: single seam — the daemon's IPC dispatcher at `crates/daemon/src/handler/mod.rs:263-287`. Per-request-kind mapping table decides what becomes an activity. **No** deep TUI-local hooks in v1 (no cursor moves, no scroll, no pane focus).
6. **Source tagging**: every `IpcMessage` carries `source: ClientKind` (`Tui` | `Cli` | `Web` | `Daemon`). Set by the client at request build time. Protocol change — see [03-capture.md](./03-capture.md#protocol-change).
7. **Link-click capture**: opt-in. Config `activity.track_link_clicks` default `false`. When enabled, URLs are stored verbatim; redactable on demand.
8. **Search query bodies**: stored verbatim, redactable. No automatic body-redaction in v1. Document plainly so users with sensitive search habits can disable or clear.
9. **Export formats**: CSV, JSON, NDJSON. All three required. NDJSON is for piping into other tools (`mxr activity export --format ndjson | jq …`).
10. **Cross-device sync**: strictly local. The activity table is never replicated, never exported automatically, never sent to a server. Hard invariant — codified in `AGENTS.md` after Phase 7.
11. **Encryption at rest**: out of scope for v1; relies on user's filesystem encryption (FileVault, LUKS). Documented as a known limitation. Revisit when account-key infra lands.
12. **Pause logging**: `mxr activity pause [--for DURATION]`. Writes one `activity.paused` marker, sets a daemon flag, then drops all activity writes until `resume` or the duration elapses.
13. **Browser-history-style clear**: `mxr activity clear --last 1h|1d|7d|all`. Tombstones matching rows. Doesn't hard-delete — retention prune still runs deterministically.
14. **Async write path**: activity writes are fire-and-forget (`tokio::spawn` inside the dispatcher). Failures are `tracing::warn!` only — never propagated to the user-facing IPC response.
15. **Ordering / pagination**: monotonic `(ts, id)` per writer. Cursor pagination keyed off `(ts DESC, id DESC)`.
16. **What activity does NOT capture**: heartbeats, internal getters used as plumbing (e.g. "fetch thread for render"), poll loops, sync ticks, FTS index rebuilds, doctor self-checks, reconciler passes. Capture only what a user *intends to do*. The mapper in [03-capture.md](./03-capture.md) is the canonical list.
17. **Failure mode**: if recorder errors, the underlying user action still succeeds. Activity is observability, not correctness.
18. **PII surface**: `context_json` may contain subjects, recipient handles, search queries, snippet text, draft prefixes (first 80 chars), URLs (opt-in only). Never bodies, never attachments, never credentials.

## Architecture

```
                       ┌──────────────────────────────────────┐
                       │  IpcMessage { id, source, payload }  │
                       └─────────────────┬────────────────────┘
                                         ▼
   ┌────────┐   ┌────────┐   ┌────────┐  │
   │  TUI   │   │  CLI   │   │  Web   │──┤
   └────────┘   └────────┘   └────────┘  │
                                         ▼
                ┌────────────────────────────────────────┐
                │  daemon dispatcher (handler/mod.rs)    │
                │  ─────────────────────────────────────  │
                │  1. tracing span                       │
                │  2. handle request → response          │
                │  3. activity::Recorder::record(...)    │  ◄── new seam
                │     (tokio::spawn, fire-and-forget)    │
                │  4. return response                    │
                └─────────────────┬──────────────────────┘
                                  ▼
                         ┌──────────────────┐
                         │  SQLite          │
                         │  user_activity   │
                         │  + FTS5 mirror   │
                         └──────────────────┘
                                  ▼
                         ┌──────────────────┐
                         │  daily prune     │  (per-tier retention)
                         └──────────────────┘
```

## File layout (target)

```
crates/store/migrations/
  0NN_user_activity.sql              # Phase 1
  0NN_user_activity_fts.sql          # Phase 1
crates/store/src/
  user_activity.rs                   # Phase 1
crates/protocol/src/
  types.rs                           # add ClientKind + ActivityFilter + verbs
crates/daemon/src/
  activity/
    mod.rs                           # Phase 2 — Recorder + spawn path
    mapper.rs                        # Phase 2 — Request → ActivityEntry
    tier.rs                          # Phase 2 — action → tier table
  handler/
    activity.rs                      # Phase 3 — query handlers
    mod.rs                           # Phase 2: wrap dispatcher
  cli/
    activity.rs                      # Phase 4 — subcommand tree
  commands/
    activity_prune.rs                # Phase 7 — extends prune loop
crates/tui/src/
  screens/activity.rs                # Phase 5
  action.rs                          # add Activity* variants
  keybindings.rs                     # `g a` chord, palette entry
crates/web/src/
  routes_v6.rs                       # add /v6/activity/* routes
apps/web/src/
  routes/activity.tsx                # Phase 6
  routes/activity.$id.tsx            # detail drawer
  components/activity/...
docs/activity-log/                   # this directory
```

## Information architecture (CLI / TUI / Web surfaces)

| Surface | Entry point | Note |
|---|---|---|
| CLI list | `mxr activity list [filters]` | reverse-chron table; `--json` for scripts |
| CLI tail | `mxr activity tail -f` | follow mode, polls daemon |
| CLI stats | `mxr activity stats --since 7d` | aggregates by action / day / source |
| CLI export | `mxr activity export --format csv|json|ndjson` | filter args same as `list` |
| CLI prune | `mxr activity prune --before 90d [--tier ephemeral] [--dry-run]` | hard-delete |
| CLI redact | `mxr activity redact [--ids ... | --filter ...] [--dry-run]` | tombstone |
| CLI clear | `mxr activity clear --last 1h|1d|7d|all` | tombstone convenience |
| CLI pause | `mxr activity pause [--for DURATION]` | stops new writes |
| CLI replay | `mxr activity replay --since 1h` | prose narrative |
| CLI recall | `mxr activity recall "before lunch"` | fuzzy-time lookup |
| TUI | `g a` from any screen / palette `View activity` | dedicated screen with filter bar |
| Web | `/activity` route | DataTable + filter sidebar + detail drawer |

## Principles

- **Single seam**: one place captures activity (the dispatcher). One place writes (the recorder). One place reads (the repository module).
- **Closed mapping**: every request kind has an explicit row in the mapper. Adding a new IPC verb forces a mapping decision (capture as `X` action / skip). Compile-time enforcement: exhaustive `match`.
- **No silent inference**: if a request doesn't have a mapping, log a `tracing::debug!` and skip — never guess.
- **Minimal blast radius**: this feature touches storage, dispatcher wrap, protocol enum addition, and per-client surfaces. Don't refactor adjacent code while adding it.
- **Pattern fidelity**: mirror `event_log.rs` for filter SQL, mirror `message_events.rs` for repo style, mirror existing `mxr` subcommand layout for CLI ergonomics.

## How to test

Reflecting feedback memory:
- Integration tests against a running daemon backed by `provider-fake`.
- Don't trust unit tests with fakes alone — every phase has an end-to-end smoke test against `mxr` itself.
- TUI and CLI both wired in the same PR. Web in a follow-up but during the same phase.

## Out of scope (v1)

- Cross-device sync of activity.
- Encryption at rest (FS-level only).
- Activity-driven recommendations / ML.
- Per-account separate activity stores (single table, `account_id` column).
- Activity-replay-as-undo (deferred to Phase 8 if at all).

## How to pick up work

1. Read this file.
2. Read [STATUS.md](./STATUS.md) — find the first unchecked box.
3. Read the phase doc owning that box.
4. Read the file paths it references.
5. Ship the smallest PR that ticks one or more boxes. Update `STATUS.md` in the same PR.
