# Phase 1 — Storage

Goal: a working `user_activity` table with FTS5 mirror, retention prune, and a repo module with insert/list/count/redact/prune/stats APIs. No clients yet — verified via integration tests only.

## Deliverables

1. Migration `crates/store/migrations/0NN_user_activity.sql` (next free number).
2. Migration `crates/store/migrations/0NN_user_activity_fts.sql` (next free number, after the base).
3. `crates/store/src/user_activity.rs` — repo module.
4. Wire-up in `crates/store/src/lib.rs` (mod, re-exports).
5. `Tier` enum (`crates/store`) and `ClientKind` enum (`crates/protocol`).
6. Unit tests (in-process sqlite, `:memory:`).
7. Integration test against a real DB file (matches `event_log` test style).
8. Bench harness scaffold (Criterion or simple timing test) — actual perf gate lives in Phase 9.

## Out of scope this phase

- IPC verbs.
- Dispatcher instrumentation.
- CLI / TUI / web surfaces.

## Files

### Created

```
crates/store/migrations/0NN_user_activity.sql
crates/store/migrations/0NN_user_activity_fts.sql
crates/store/src/user_activity.rs
crates/protocol/src/types.rs        (additions: ClientKind enum)
```

### Modified

```
crates/store/src/lib.rs                # `pub mod user_activity;` + re-exports
crates/store/Cargo.toml                # no new deps expected — uses sqlx + chrono
```

## Migration content

See [01-schema-and-taxonomy.md](./01-schema-and-taxonomy.md#table-schema) and [#fts5-mirror](./01-schema-and-taxonomy.md#fts5-mirror). Migrations are split: base + FTS in separate files so anyone wanting to rebuild the FTS table can replay the second migration in isolation.

Migrations are checked into `crates/store/migrations/` and run by sqlx-migrate at daemon start. Verify the next-free number locally before naming.

## Module structure

```rust
// crates/store/src/user_activity.rs

use crate::Store;             // existing store handle (single writer + reader pool)
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier { Ephemeral, Standard, Important }

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Ephemeral => "ephemeral", Self::Standard => "standard", Self::Important => "important" }
    }
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ActivityRow {
    pub id: i64,
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: String,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: String,
    pub context_json: Option<String>,
    pub redacted: i64,
}

#[derive(Debug, Clone)]
pub struct ActivityInsert<'a> {
    pub ts: i64,
    pub account_id: Option<&'a str>,
    pub source: &'a str,                  // 'tui' | 'cli' | 'web' | 'daemon'
    pub action: &'a str,
    pub target_kind: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub tier: Tier,
    pub context: Option<&'a serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActivityFilter {
    pub since: Option<i64>,                // unix ms inclusive
    pub until: Option<i64>,                // unix ms exclusive
    pub account_id: Option<String>,
    pub sources: Vec<String>,              // empty = any
    pub actions: Vec<String>,              // exact match; empty = any
    pub action_prefix: Option<String>,     // e.g. "mail." matches all mail.*
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tiers: Vec<String>,                // empty = any
    pub query: Option<String>,             // FTS5 over context_json
    pub include_redacted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ActivityCursor { pub ts: i64, pub id: i64 }

#[derive(Debug, Clone, Serialize)]
pub struct ActivityPage {
    pub rows: Vec<ActivityRow>,
    pub next_cursor: Option<ActivityCursor>,
    pub total_estimate: Option<i64>,       // present only when caller asks
}

impl Store {
    /// Single-row insert. Used by Recorder. Errors logged, never propagated to caller.
    pub async fn record_activity(&self, e: ActivityInsert<'_>) -> sqlx::Result<i64> { /* ... */ }

    /// Batch insert (used for backfill / migrations / tests).
    pub async fn record_activity_batch(&self, entries: &[ActivityInsert<'_>]) -> sqlx::Result<usize> { /* ... */ }

    /// Paginated list with stable cursor (ts DESC, id DESC).
    pub async fn list_activity(
        &self,
        filter: &ActivityFilter,
        limit: u32,
        cursor: Option<ActivityCursor>,
    ) -> sqlx::Result<ActivityPage> { /* ... */ }

    pub async fn count_activity(&self, filter: &ActivityFilter) -> sqlx::Result<i64> { /* ... */ }

    /// Tombstone: set redacted=1 and context_json=NULL. FTS trigger handles index sync.
    pub async fn redact_activity_by_ids(&self, ids: &[i64]) -> sqlx::Result<u64> { /* ... */ }
    pub async fn redact_activity_by_filter(&self, filter: &ActivityFilter) -> sqlx::Result<u64> { /* ... */ }

    /// Hard delete. Used by retention prune.
    pub async fn prune_activity_before(&self, before_ts: i64, tier: Option<Tier>) -> sqlx::Result<u64> { /* ... */ }

    /// Per-(action, day) counts for the stats command.
    pub async fn activity_stats_by_action(&self, since: i64, until: i64) -> sqlx::Result<Vec<(String, i64)>> { /* ... */ }
    pub async fn activity_stats_by_day(&self, since: i64, until: i64) -> sqlx::Result<Vec<(i64, i64)>> { /* ... */ }
    pub async fn activity_stats_by_source(&self, since: i64, until: i64) -> sqlx::Result<Vec<(String, i64)>> { /* ... */ }
    pub async fn activity_stats_by_target_kind(&self, since: i64, until: i64) -> sqlx::Result<Vec<(String, i64)>> { /* ... */ }
}
```

## Filter SQL pattern

Mirror `crates/store/src/event_log.rs:81-123` — dynamic `WHERE` builder with bind params. Build a `QueryBuilder` so we can compose `AND` clauses conditionally without `format!()` string concat. Sqlx provides `sqlx::QueryBuilder` for this.

Key clauses:

- `since` / `until` → `ts >= ?` / `ts < ?`
- `account_id` → `account_id = ?`
- `sources` → `source IN (...)`
- `actions` → `action IN (...)`
- `action_prefix` → `action LIKE ? || '%'`
- `target_kind` → `target_kind = ?`
- `target_id` → `target_id = ?`
- `tiers` → `tier IN (...)`
- `include_redacted=false` → `redacted = 0`
- `query` → `id IN (SELECT rowid FROM user_activity_fts WHERE user_activity_fts MATCH ?)`

Cursor predicate (descending order, stable):

```
ORDER BY ts DESC, id DESC
LIMIT ?

-- with cursor (ts_c, id_c):
WHERE (ts < ts_c OR (ts = ts_c AND id < id_c))
```

Return `next_cursor = Some((last_row.ts, last_row.id))` only when `rows.len() == limit`.

## Source enum

Add to `crates/protocol/src/types.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientKind {
    Tui,
    Cli,
    Web,
    Daemon,                                // for synthesized activity (e.g. activity.pruned)
}

impl ClientKind {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Tui => "tui", Self::Cli => "cli", Self::Web => "web", Self::Daemon => "daemon" }
    }
}
```

Don't yet add `source` to `IpcMessage` — that's Phase 2's protocol change. Phase 1 only needs the type.

## Unit tests

```rust
// crates/store/src/user_activity.rs (#[cfg(test)] mod tests)

#[tokio::test] async fn insert_and_list_round_trip() { ... }
#[tokio::test] async fn filter_by_action_prefix() { ... }
#[tokio::test] async fn filter_by_date_range() { ... }
#[tokio::test] async fn filter_by_source_multi() { ... }
#[tokio::test] async fn cursor_pagination_stable() { ... }   // insert N+1 rows, page through, assert no dupes/skips
#[tokio::test] async fn redact_by_id_tombstones_and_clears_context() { ... }
#[tokio::test] async fn redact_excluded_from_default_list() { ... }
#[tokio::test] async fn redact_included_with_flag() { ... }
#[tokio::test] async fn prune_by_tier_only_deletes_matching_tier() { ... }
#[tokio::test] async fn fts_query_finds_subject() { ... }
#[tokio::test] async fn fts_after_redaction_does_not_match() { ... }
#[tokio::test] async fn stats_by_action_groups_correctly() { ... }
#[tokio::test] async fn stats_by_day_buckets_by_local_day() { ... } // doc the timezone choice — see below
```

### Timezone for `stats_by_day`

Group by **UTC day** in SQL. CLI/TUI/web formatters convert to the user's local timezone for display. Document plainly so analytics consumers know what they're getting. Mirrors the `analytics` crate convention (verify before coding).

## Integration test

`crates/store/tests/user_activity_integration.rs`:

1. Open a real temp-file SQLite DB (not `:memory:`).
2. Run all migrations.
3. Insert 1,000 rows across 3 tiers and 4 sources.
4. List with no filter — assert order and pagination.
5. List with `action_prefix="mail."` — assert filter.
6. FTS query for a known subject substring — assert one match.
7. Redact 10 rows by id — assert tombstones and FTS exclusion.
8. Prune `tier=ephemeral` before `now - 30d` — assert count and remaining rows.
9. Stats by action — assert sums match.

Reuse helpers from existing `event_log` integration tests where they exist.

## Acceptance criteria

- All migrations apply cleanly on a fresh DB and on an existing DB with prior migrations.
- All unit tests pass; integration test passes.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Insert latency on a warm DB is < 1 ms p99 in the bench scaffold (hard gate moved to Phase 9, but record baseline now).
- Pagination is stable under concurrent inserts (test by inserting between pages and asserting no row is skipped or duplicated).

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| FTS5 not compiled into sqlx's bundled sqlite | sqlx-sqlite uses the system or bundled SQLite — verify build feature includes FTS5 (default for modern sqlite). Add a test that asserts `SELECT * FROM user_activity_fts` works on a fresh DB. |
| Triggers cause write amplification | Acceptable — FTS write cost is part of the budget. Bench it. |
| Concurrent writers (we have a single writer though) | Reuse `Store`'s single-writer pool — same as `event_log`. |
| Migration number race with parallel work | Last-second rename + a single fix-up commit in the PR. Note in PR description. |

## Exit criteria

Phase 1 is done when:
- All deliverables above ship.
- STATUS.md Phase 1 boxes ticked.
- An ad-hoc `sqlite3` session shows the table populated by an integration test run.
- Reviewer can run `cargo test -p mxr-store user_activity` and see it green.
