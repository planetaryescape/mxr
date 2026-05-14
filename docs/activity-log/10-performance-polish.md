# Phase 9 — Performance & Polish

Goal: prove the activity log holds up at scale, compact noisy patterns, document the feature, and update `ARCHITECTURE.md` so future contributors find the seam quickly.

## Deliverables

1. Insert benchmark: 100k rows under realistic load (10 concurrent writers).
2. Query benchmark: 10k-row filtered list returns < 100 ms p99.
3. FTS5 query benchmark over 100k rows < 250 ms p99.
4. Compaction pass: collapses rapid-fire duplicates within 250 ms windows into one row with `count`.
5. `PRAGMA optimize` scheduled monthly; `VACUUM` documented as a manual operation.
6. `ARCHITECTURE.md` updated with the activity seam.
7. User-facing release notes draft.
8. Cleanup pass on TODOs across earlier phases.

## Out of scope

- Partitioning by month (deferred until log size justifies it).
- Encryption at rest (own initiative, separate doc).

## 9.1 — Bench harness

`crates/store/benches/user_activity.rs` (Criterion):

```rust
fn bench_insert_serial(c: &mut Criterion) { ... }
fn bench_insert_concurrent_10w(c: &mut Criterion) { ... }
fn bench_list_unfiltered_p99(c: &mut Criterion) { ... }
fn bench_list_by_action_prefix(c: &mut Criterion) { ... }
fn bench_fts_query(c: &mut Criterion) { ... }
fn bench_cursor_pagination_through_10k(c: &mut Criterion) { ... }
```

Targets (hard pass/fail):

| Bench | Target | Notes |
|---|---|---|
| Insert serial | < 0.5 ms p99 | warm db, WAL, single writer |
| Insert concurrent (10 producers, 1 writer-pool) | < 1.5 ms p99 | producer wait time included |
| List 50 rows, unfiltered, fresh DB | < 5 ms p99 | |
| List 50 rows over 100k-row DB | < 25 ms p99 | indexed `(ts DESC)` |
| List 50 rows with `action_prefix='mail.'`, 100k rows | < 50 ms p99 | LIKE prefix; uses idx_action_ts |
| FTS query, 100k rows | < 250 ms p99 | unicode61 tokenizer |
| Stats by action, 100k rows | < 100 ms p99 | covered by idx_action_ts |
| Stats by day, 100k rows | < 150 ms p99 | bucket on ts; covered by idx_ts |

If a bench misses target, add the index it points at. Document any index addition in [01-schema-and-taxonomy.md](./01-schema-and-taxonomy.md).

## 9.2 — Compaction

Some user actions repeat in tight bursts:
- Holding `j` in the TUI doesn't emit activity (cursor moves aren't recorded), but rapid `mail.read` from auto-mark-read-on-scroll **does** — and would flood the log.
- Auto-archive shortcuts may fire repeatedly during cleanup sessions.

### Strategy: write-time coalescing

In the recorder, before insert:
1. Check the most-recent activity row for the same `(account_id, action, target_id)` triple within 250 ms.
2. If present, **update** that row in place:
   - `ts` → max(old, new) (most recent observation)
   - `context_json.count` → `(old.count ?? 1) + 1`
3. Else insert as new row.

Implementation: cache last 32 entries in-memory (per account) keyed by `(action, target_id)` with their `(id, ts)`. On match within window, issue an `UPDATE`. On miss, insert and update cache.

Eviction: bounded LRU; entries older than 1 s are dropped.

### Tradeoffs

- Compaction is **on by default** for `ephemeral`-tier actions only. `standard`/`important` actions are written as-is (audit fidelity > log compactness for those).
- The 250 ms window and 32-slot cache are configurable (env vars at first, config at Phase 10 if needed).

### Tests

- Burst-archive: 5 archive calls within 100 ms → 1 row with `count: 5`.
- Slow-archive: 5 archive calls 500 ms apart → 5 separate rows.
- Different targets: 5 archives of different threads within 100 ms → 5 rows (cache key is `(action, target_id)`).
- Important-tier action (`mail.send`): never coalesced.

## 9.3 — Scheduled maintenance

- Daily prune (Phase 7) already runs.
- Add monthly `PRAGMA optimize` (cheap; ~ms) — append to existing maintenance loop.
- Document `VACUUM` as manual (`mxr maintenance vacuum`) — it locks the DB and isn't appropriate for background runs.

## 9.4 — `ARCHITECTURE.md` update

Add a section after "Event log":

```markdown
## Activity log

User-initiated actions are recorded in `user_activity`, captured at the daemon's IPC dispatch seam (`crates/daemon/src/handler/mod.rs`). Every IPC request that mutates state or expresses user intent (search, view, mutation, draft action) produces one row, tagged with the originating client (`tui` | `cli` | `web` | `daemon`).

Storage: dedicated table + FTS5 mirror over `context_json`. Append-only; redaction is a tombstone, never a hard delete. Retention is tier-aware: 30 / 90 / 365 days for ephemeral / standard / important by default, configurable per tier.

Capture seam: a single `Recorder` writes through a bounded mpsc channel. Failures are observability-only and never propagate to user-facing responses. The list of capturable IPC verbs lives in `crates/daemon/src/activity/mapper.rs` — an exhaustive `match` so adding a new IPC verb forces a mapping decision at compile time.

Query: `AdminMaintenance` IPC bucket. CLI: `mxr activity`. TUI: `g a` chord. Web: `/activity`.

Invariants:
1. Never transmitted off-device.
2. `context_json` never holds credentials, tokens, or bodies.
3. `MXR_ACTIVITY=off` disables the recorder for the lifetime of the daemon.
4. The recorder is the only path that writes to `user_activity`. Enforced by structural test.

Detailed implementation plan: [`docs/activity-log/`](./docs/activity-log/README.md).
```

## 9.5 — Cleanup pass

For each earlier phase, sweep for:
- Unreachable code (e.g. stub handlers that returned `unimplemented!()`).
- Dead constants.
- TODOs marked `TODO(p9):` — resolve or convert into GH issues.
- `tracing::trace!` instrumentation added during dev — promote useful ones to `debug!`, delete the rest.

## 9.6 — Release notes draft

Draft a release-notes section (release-notes skill format):

```markdown
## Activity log (new)

mxr now keeps a local, queryable log of everything you do across TUI, CLI, and web — reads, searches, archives, sends, clicks (opt-in). Browse the last day, last week, or any custom range; filter by action, source, or full-text search across context.

- `mxr activity list --since 24h` — your last day at a glance
- `mxr activity replay --since 1h` — narrative of recent activity
- `mxr activity recall "yesterday afternoon"` — fuzzy time lookup
- `g a` in the TUI — open the activity screen
- `/activity` in the web app — DataTable with filters

Retention: 30 / 90 / 365 days for ephemeral / standard / important actions, configurable per tier. Never leaves your device — no sync, no telemetry, no transmission. Pause anytime with `mxr activity pause`. Scrub with `mxr activity clear --last 1h`.
```

## Tests / verification

- Bench harness green against targets.
- Compaction tests pass.
- `ARCHITECTURE.md` updated and reviewed.
- Release notes drafted in `docs/changes/` (or wherever the release-notes skill writes).

## Acceptance criteria

- All Phase 9 bench targets met or documented with index/compaction follow-up.
- Compaction never coalesces important-tier rows (proven by test).
- A reader of `ARCHITECTURE.md` finds the activity seam in under a minute.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Bench environment variability | Use Criterion's statistical thresholds; pin to a known machine for CI gates (or relax targets and assert *delta* against baseline). |
| Compaction corrupts audit trail | Important-tier excluded. Add a test that asserts no `mail.send` is ever coalesced. |
| `PRAGMA optimize` blocks writes | It doesn't (statistics update only). Verify with a smoke test. |

## Exit criteria

Phase 9 is done when:
- Bench harness CI gate is green.
- Compaction integration tests pass.
- `ARCHITECTURE.md` updated.
- Release notes shipped with the version that lands this work.
- `STATUS.md` Phase 9 boxes ticked.
- All cross-phase invariants in [STATUS.md](./STATUS.md) verified one final time.
