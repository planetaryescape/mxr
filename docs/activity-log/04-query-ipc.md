# Phase 3 — Query & Filter IPC

Goal: every read and admin operation on the activity log is reachable via IPC. CLI / TUI / web all consume the same verbs.

## Deliverables

1. New `Request` variants under `AdminMaintenance`:
   - `ListActivity { filter, limit, cursor }`
   - `CountActivity { filter }`
   - `ActivityStats { since, until, group_by }`
   - `ExportActivity { filter, format }`
   - `RedactActivity { ids? filter? dry_run }`
   - `PruneActivity { before_ts, tier?, dry_run }`
   - `PauseActivity { until_ts? }`
   - `ResumeActivity`
2. `ActivityFilter`, `ActivityCursor`, `ActivityStatGroupBy`, `ActivityExportFormat` types in `crates/protocol/src/types.rs`.
3. Handler module `crates/daemon/src/handler/activity.rs`.
4. Dispatcher switch (`crates/daemon/src/handler/mod.rs:289-304`) routes new verbs.
5. New verbs explicitly excluded from the mapper (Phase 2 stub already did so for the `List*`/`Count*` family; verify mutating verbs route through synthesized markers in their handlers).
6. Snapshot tests for CLI help (will fill once Phase 4 ships, but the IPC contract is fixed here).

## Out of scope

- CLI / TUI / Web surfaces (Phases 4-6).
- Saved activity filters (Phase 8).

## Type definitions

```rust
// crates/protocol/src/types.rs

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivityFilter {
    pub since: Option<i64>,                 // unix ms inclusive
    pub until: Option<i64>,                 // unix ms exclusive
    pub account_id: Option<String>,
    pub sources: Vec<ClientKind>,           // empty = any
    pub actions: Vec<String>,
    pub action_prefix: Option<String>,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tiers: Vec<Tier>,                   // protocol-level enum (mirrors store Tier)
    pub query: Option<String>,              // FTS5 expression
    pub include_redacted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ActivityCursor { pub ts: i64, pub id: i64 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatGroupBy { Action, Day, Source, TargetKind, Hour }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityExportFormat { Csv, Json, Ndjson }

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier { Ephemeral, Standard, Important }     // ← mirror of store Tier, lives in protocol
```

`Tier` in protocol is a duplicate of the store-side enum. We accept a small amount of redundancy to keep `protocol` a leaf crate (it cannot depend on `store`). Convert at the boundary.

## Request / Response shape

```rust
// crates/protocol/src/types.rs (add to existing Request enum)

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    // ... existing variants ...
    ListActivity { filter: ActivityFilter, limit: u32, cursor: Option<ActivityCursor> },
    CountActivity { filter: ActivityFilter },
    ActivityStats { since: i64, until: i64, group_by: ActivityStatGroupBy },
    ExportActivity { filter: ActivityFilter, format: ActivityExportFormat, path: Option<String> },
    RedactActivity { ids: Vec<i64>, filter: Option<ActivityFilter>, dry_run: bool },
    PruneActivity { before_ts: i64, tier: Option<Tier>, dry_run: bool },
    PauseActivity { until_ts: Option<i64> },
    ResumeActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {                  // wire shape of a row
    pub id: i64,
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: ClientKind,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: Tier,
    pub context: Option<serde_json::Value>, // parsed; clients don't see the raw string
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseData {
    // ... existing ...
    ActivityEntries {
        entries: Vec<ActivityEntry>,
        next_cursor: Option<ActivityCursor>,
        approx_total: Option<i64>,
    },
    Count { count: i64 },
    ActivityStatBuckets { buckets: Vec<ActivityStatBucket> },
    ExportResult { format: ActivityExportFormat, path: Option<String>, size_bytes: u64, count: i64 },
    Affected { count: i64, dry_run: bool },
    Acknowledged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStatBucket {
    pub key: String,              // 'action token' / 'YYYY-MM-DD' / 'tui' / 'thread' / '00'..'23'
    pub count: i64,
}
```

## Filter semantics (canonical, copy into help text)

- All filter fields are AND-combined.
- Empty `Vec<_>` filters mean "any" — explicit absence, not "match nothing".
- `actions` and `action_prefix` both apply; intersect.
- `query` runs against the FTS5 mirror. Standard SQLite FTS5 syntax (`"phrase"`, `term1 AND term2`, `term*`).
- `include_redacted=false` is the default. Redacted rows have `context=None` and `redacted=true`.
- `ListActivity` is paginated. `limit` clamped server-side to `[1, 500]`. Default 50 if `0`.

## Handler module

```rust
// crates/daemon/src/handler/activity.rs

use mxr_protocol::*;
use mxr_store::user_activity::{self as ua, Tier as StoreTier};
use crate::activity::Recorder;
use std::sync::Arc;

pub struct ActivityHandler {
    pub store: Arc<mxr_store::Store>,
    pub recorder: Arc<Recorder>,
}

impl ActivityHandler {
    pub async fn list(&self, filter: ActivityFilter, limit: u32, cursor: Option<ActivityCursor>) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn count(&self, filter: ActivityFilter) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn stats(&self, since: i64, until: i64, group: ActivityStatGroupBy) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn export(&self, filter: ActivityFilter, format: ActivityExportFormat, path: Option<String>) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn redact(&self, ids: Vec<i64>, filter: Option<ActivityFilter>, dry_run: bool) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn prune(&self, before_ts: i64, tier: Option<Tier>, dry_run: bool) -> Result<ResponseData, HandlerError> { /* ... */ }
    pub async fn pause(&self, until_ts: Option<i64>) -> Result<ResponseData, HandlerError> {
        self.recorder.pause(until_ts);
        // synthesized marker — daemon source
        self.recorder.record(OwnedEntry {
            action: "activity.paused".into(),
            tier: StoreTier::Important,
            source: ClientKind::Daemon,
            context: Some(serde_json::json!({ "until": until_ts })),
            // ... ts, etc
        });
        Ok(ResponseData::Acknowledged)
    }
    pub async fn resume(&self) -> Result<ResponseData, HandlerError> { /* ... */ }
}
```

Conversion between `protocol::Tier` and `store::user_activity::Tier`:

```rust
impl From<mxr_protocol::Tier> for mxr_store::user_activity::Tier {
    fn from(t: mxr_protocol::Tier) -> Self {
        use mxr_protocol::Tier::*;
        match t { Ephemeral => Self::Ephemeral, Standard => Self::Standard, Important => Self::Important }
    }
}
```

## Export

Export is a daemon-side operation that:
- Reads matching rows from the store (no `limit`, paged internally).
- Streams to either a `path` (if provided) or returns a base64 / inline `String` for short exports (cap inline at, say, 1 MiB; over that require `path`).
- Emits a synthesized `activity.exported` marker on success.

Format details:
- **CSV**: `id,ts_iso,ts_unix_ms,account_id,source,action,target_kind,target_id,tier,context_json,redacted` — RFC 4180, comma-separated, double-quoted strings with escaped quotes.
- **JSON**: top-level array of `ActivityEntry`. Pretty-printed (2-space indent).
- **NDJSON**: one `ActivityEntry` per line, newline-delimited. Best for piping into `jq`, `awk`, etc.

CSV serialization uses the `csv` crate (already widely depended on; pull in if not present). JSON / NDJSON use `serde_json`.

## Redact

- If `ids` is non-empty, redacts by id list.
- Else if `filter` is `Some`, redacts by filter.
- If both empty / None, return error `"redact requires ids or filter"`.
- `dry_run=true` returns `Affected { count, dry_run: true }` without mutating.

Emits a synthesized `activity.redacted` marker after success with `context: { count, by: "ids"|"filter" }`. The marker itself **cannot be redacted by the same call** (it lands after the redaction completes). To remove it, the user runs a follow-up filter that targets it.

## Prune

- Hard delete. `before_ts` is a unix-ms cutoff.
- `tier=None` prunes all tiers.
- `dry_run=true` returns expected affected count.
- Emits `activity.pruned`.

## Pause / Resume

- `Pause { until_ts: None }` → indefinite. `until_ts: Some(t)` → auto-resume at `t`.
- `Resume` → clears pause flag.
- Both emit synthesized markers (`activity.paused` / `activity.resumed`). The marker is written **even when paused** — pause does not silence its own announcement, otherwise users would never know it took effect.

Implementation detail: the recorder skips records when paused, but the pause/resume markers go through a special channel that bypasses the pause check. Encode this as a `RecorderMessage::ForceRecord(OwnedEntry)` variant or as a direct DB write at the handler.

## Dispatcher switch

`crates/daemon/src/handler/mod.rs:289-304` currently routes `Request::ListEvents` to its handler. Add identical branches for all new verbs, each calling into `ActivityHandler`. Keep the switch arms grouped (existing AdminMaintenance verbs first, activity verbs after them).

## Web bridge surface (preview — implemented in Phase 6)

These verbs map to HTTP routes:

| Verb | HTTP |
|---|---|
| `ListActivity` | `GET /v6/activity?...` |
| `CountActivity` | `GET /v6/activity/count?...` |
| `ActivityStats` | `GET /v6/activity/stats?...` |
| `ExportActivity` | `POST /v6/activity/export` (body: filter + format) |
| `RedactActivity` | `POST /v6/activity/redact` (body: ids + filter + dry_run) |
| `PruneActivity` | `POST /v6/activity/prune` (body: before_ts + tier + dry_run) |
| `PauseActivity` | `POST /v6/activity/pause` |
| `ResumeActivity` | `POST /v6/activity/resume` |

This phase only fixes the *contract*. Phase 6 implements the routes.

## Tests

- Unit: each handler with a mock store, round-trip filter shapes, response shapes match snapshots.
- Integration: send each verb over a real socket against a `provider-fake` daemon. Assert response.
- Pagination invariants: page through 1,000 rows in pages of 50; assert no duplicate ids, no gaps, monotonically descending `(ts, id)`.
- Redact-then-list: redact 10 ids, list with default filter, assert exclusion. List with `include_redacted=true`, assert presence with `redacted=true` and `context=None`.
- Pause-then-record-then-list: pause; trigger 5 mutations; resume; list. Assert pause/resume markers landed but the 5 mutations did not.

## Acceptance criteria

- Every new verb has a handler, a unit test, and an integration test.
- Cursor pagination is stable under concurrent writes (test: writer thread inserting while reader pages).
- `--dry-run` paths never mutate (test: assert row counts unchanged after dry-run).
- Synthesized markers land in the correct tier and with sensible context.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Filter combinatorial explosion | Reuse the dynamic SQL pattern from `event_log.rs`; keep clauses one-per-field. Don't pre-optimize. |
| Export of 100k rows ties up the writer | Reads use the reader pool; writers are unaffected. Stream rows in chunks of 1,000. |
| Pause races (request mid-flight when pause flips) | Acceptable — pause is a *future* contract, not a transactional one. Document plainly. |

## Exit criteria

Phase 3 is done when:
- All verbs round-trip through a real daemon.
- Web phase 6 can write its routes against the published OpenAPI without ambiguity.
- `STATUS.md` Phase 3 boxes ticked.
