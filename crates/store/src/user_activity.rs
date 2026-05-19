//! User activity log. Append-only record of user-initiated actions across
//! TUI / CLI / web. The design lives in `docs/activity-log.md`. See
//! `01-schema-and-taxonomy.md` for the table shape and `02-storage.md` for
//! this module's contract.
//!
//! Public surface, in plain English:
//! - `record_activity` — insert one row. Used by the recorder.
//! - `record_activity_batch` — insert many rows (test/backfill helper).
//! - `list_activity` — paginated read with stable `(ts DESC, id DESC)` cursor.
//! - `count_activity` — total matching rows for a filter (UI counts).
//! - `redact_activity_by_ids` / `redact_activity_by_filter` — tombstone, not delete.
//! - `prune_activity_before` — hard delete; retention pruner only.
//! - `activity_stats_*` — grouped counts for the stats command.
//!
//! Errors come back as `sqlx::Result<T>` to match the rest of the crate.
//! Recorder callers swallow the error with a warn log; this module does
//! not propagate observability concerns.

use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use sqlx::Row;
use std::str::FromStr;

use crate::Store;

/// Retention tier assigned by the mapper based on the action token. Drives
/// the prune schedule (30 / 90 / 365 days for ephemeral / standard /
/// important by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Ephemeral,
    Standard,
    Important,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Standard => "standard",
            Self::Important => "important",
        }
    }

    fn parse_str(s: &str) -> Option<Self> {
        match s {
            "ephemeral" => Some(Self::Ephemeral),
            "standard" => Some(Self::Standard),
            "important" => Some(Self::Important),
            _ => None,
        }
    }
}

impl FromStr for Tier {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s).ok_or(())
    }
}

/// On-disk shape. `redacted=1` means the row is a tombstone — `context_json`
/// is `NULL`, the rest of the columns survive for audit-trail purposes.
#[derive(Debug, Clone, Serialize)]
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
    pub redacted: bool,
}

/// Borrowed insert payload. The recorder owns the data; we don't clone.
#[derive(Debug, Clone)]
pub struct ActivityInsert<'a> {
    pub ts: i64,
    pub account_id: Option<&'a str>,
    pub source: &'a str,
    pub action: &'a str,
    pub target_kind: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub tier: Tier,
    pub context: Option<&'a serde_json::Value>,
}

/// Filter shape shared with the protocol crate (which duplicates it as a
/// leaf to avoid a store dep). Empty vectors mean "any" — the caller did
/// not narrow that field.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActivityFilter {
    pub since: Option<i64>, // unix ms inclusive
    pub until: Option<i64>, // unix ms exclusive
    pub account_id: Option<String>,
    pub sources: Vec<String>,
    pub actions: Vec<String>,
    pub action_prefix: Option<String>,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tiers: Vec<String>,
    pub query: Option<String>, // FTS5 expression
    pub include_redacted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ActivityCursor {
    pub ts: i64,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivityPage {
    pub rows: Vec<ActivityRow>,
    pub next_cursor: Option<ActivityCursor>,
}

/// Persisted shape of a named filter preset. Mirrors the
/// `saved_activity_filters` migration. Like activity rows, these are
/// strictly local — never synced, never transmitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedActivityFilter {
    pub slug: String,
    pub name: String,
    /// Serialized `ActivityFilter` JSON. Stored as a string so the
    /// repository module stays generic — the protocol crate owns the
    /// parsed shape.
    pub filter_json: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
}

const MAX_LIMIT: u32 = 500;
const DEFAULT_LIMIT: u32 = 50;

fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_LIMIT
    } else {
        limit.min(MAX_LIMIT)
    }
}

fn row_to_activity(r: &sqlx::sqlite::SqliteRow) -> sqlx::Result<ActivityRow> {
    let redacted: i64 = r.try_get("redacted")?;
    Ok(ActivityRow {
        id: r.try_get("id")?,
        ts: r.try_get("ts")?,
        account_id: r.try_get("account_id")?,
        source: r.try_get("source")?,
        action: r.try_get("action")?,
        target_kind: r.try_get("target_kind")?,
        target_id: r.try_get("target_id")?,
        tier: r.try_get("tier")?,
        context_json: r.try_get("context_json")?,
        redacted: redacted != 0,
    })
}

impl Store {
    /// Single-row insert. Returns the new row id. Errors are not propagated
    /// to user-facing IPC by the recorder; this method just surfaces them
    /// for observability.
    pub async fn record_activity(&self, e: ActivityInsert<'_>) -> sqlx::Result<i64> {
        let context_str: Option<String> = match e.context {
            Some(value) => Some(
                serde_json::to_string(value).map_err(|err| sqlx::Error::Encode(Box::new(err)))?,
            ),
            None => None,
        };
        let result = sqlx::query(
            "INSERT INTO user_activity (
                ts, account_id, source, action, target_kind, target_id, tier, context_json
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(e.ts)
        .bind(e.account_id)
        .bind(e.source)
        .bind(e.action)
        .bind(e.target_kind)
        .bind(e.target_id)
        .bind(e.tier.as_str())
        .bind(context_str)
        .execute(self.writer())
        .await?;
        Ok(result.last_insert_rowid())
    }

    /// Coalesce: update an existing row with a new timestamp and a
    /// bumped `count` in its `context_json`. Used by the recorder's
    /// write-time compaction path for rapid-fire ephemeral/standard
    /// duplicates (same action+target within ~250ms). Never used for
    /// important-tier rows — those are always written as-is.
    pub async fn coalesce_activity(
        &self,
        id: i64,
        new_ts: i64,
        new_count: u64,
    ) -> sqlx::Result<()> {
        // Read existing context_json, bump `count`, write back.
        let existing: Option<String> =
            sqlx::query_scalar("SELECT context_json FROM user_activity WHERE id = ?")
                .bind(id)
                .fetch_optional(self.writer())
                .await?
                .flatten();
        let mut value: serde_json::Value = match existing.as_deref() {
            Some(s) => serde_json::from_str(s).unwrap_or(serde_json::json!({})),
            None => serde_json::json!({}),
        };
        if !value.is_object() {
            value = serde_json::json!({});
        }
        value["count"] = serde_json::Value::from(new_count);
        let new_json =
            serde_json::to_string(&value).map_err(|err| sqlx::Error::Encode(Box::new(err)))?;
        sqlx::query("UPDATE user_activity SET ts = ?, context_json = ? WHERE id = ?")
            .bind(new_ts)
            .bind(new_json)
            .bind(id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Batch insert. Used by tests, backfill, and any future bulk-ingest
    /// path. Returns the count of rows written.
    pub async fn record_activity_batch(
        &self,
        entries: &[ActivityInsert<'_>],
    ) -> sqlx::Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }
        let mut tx = self.writer().begin().await?;
        for e in entries {
            let context_str: Option<String> = match e.context {
                Some(value) => Some(
                    serde_json::to_string(value)
                        .map_err(|err| sqlx::Error::Encode(Box::new(err)))?,
                ),
                None => None,
            };
            sqlx::query(
                "INSERT INTO user_activity (
                    ts, account_id, source, action, target_kind, target_id, tier, context_json
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(e.ts)
            .bind(e.account_id)
            .bind(e.source)
            .bind(e.action)
            .bind(e.target_kind)
            .bind(e.target_id)
            .bind(e.tier.as_str())
            .bind(context_str)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(entries.len())
    }

    /// Paginated list. Returns up to `limit` rows in `(ts DESC, id DESC)`
    /// order. `next_cursor` is `Some(_)` only when the page filled — i.e.
    /// more rows likely exist below.
    pub async fn list_activity(
        &self,
        filter: &ActivityFilter,
        limit: u32,
        cursor: Option<ActivityCursor>,
    ) -> sqlx::Result<ActivityPage> {
        let limit = clamp_limit(limit);
        let mut qb: QueryBuilder<sqlx::Sqlite> =
            QueryBuilder::new("SELECT * FROM user_activity WHERE 1=1");
        push_filter_clauses(&mut qb, filter);
        if let Some(c) = cursor {
            qb.push(" AND (ts < ");
            qb.push_bind(c.ts);
            qb.push(" OR (ts = ");
            qb.push_bind(c.ts);
            qb.push(" AND id < ");
            qb.push_bind(c.id);
            qb.push("))");
        }
        qb.push(" ORDER BY ts DESC, id DESC LIMIT ");
        qb.push_bind(limit as i64);

        let rows = qb.build().fetch_all(self.reader()).await?;
        let rows: Vec<ActivityRow> = rows
            .iter()
            .map(row_to_activity)
            .collect::<sqlx::Result<_>>()?;

        let next_cursor = if rows.len() as u32 == limit {
            rows.last().map(|r| ActivityCursor { ts: r.ts, id: r.id })
        } else {
            None
        };

        Ok(ActivityPage { rows, next_cursor })
    }

    pub async fn count_activity(&self, filter: &ActivityFilter) -> sqlx::Result<i64> {
        let mut qb: QueryBuilder<sqlx::Sqlite> =
            QueryBuilder::new("SELECT COUNT(*) FROM user_activity WHERE 1=1");
        push_filter_clauses(&mut qb, filter);
        let row = qb.build().fetch_one(self.reader()).await?;
        Ok(row.try_get::<i64, _>(0)?)
    }

    /// Tombstone rows by id. `context_json` is cleared so the FTS trigger
    /// removes the row from the index too.
    pub async fn redact_activity_by_ids(&self, ids: &[i64]) -> sqlx::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "UPDATE user_activity SET redacted = 1, context_json = NULL WHERE id IN (",
        );
        let mut sep = qb.separated(", ");
        for id in ids {
            sep.push_bind(*id);
        }
        qb.push(")");
        let result = qb.build().execute(self.writer()).await?;
        Ok(result.rows_affected())
    }

    /// Tombstone rows by filter. Useful for `mxr activity clear --last 1h`.
    pub async fn redact_activity_by_filter(&self, filter: &ActivityFilter) -> sqlx::Result<u64> {
        let mut qb: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
            "UPDATE user_activity SET redacted = 1, context_json = NULL WHERE 1=1",
        );
        push_filter_clauses(&mut qb, filter);
        let result = qb.build().execute(self.writer()).await?;
        Ok(result.rows_affected())
    }

    /// Hard delete. Retention pruner only.
    pub async fn prune_activity_before(
        &self,
        before_ts: i64,
        tier: Option<Tier>,
    ) -> sqlx::Result<u64> {
        let result = match tier {
            Some(t) => {
                sqlx::query("DELETE FROM user_activity WHERE ts < ? AND tier = ?")
                    .bind(before_ts)
                    .bind(t.as_str())
                    .execute(self.writer())
                    .await?
            }
            None => {
                sqlx::query("DELETE FROM user_activity WHERE ts < ?")
                    .bind(before_ts)
                    .execute(self.writer())
                    .await?
            }
        };
        Ok(result.rows_affected())
    }

    pub async fn activity_stats_by_action(
        &self,
        since: i64,
        until: i64,
    ) -> sqlx::Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT action, COUNT(*) AS c
             FROM user_activity
             WHERE ts >= ? AND ts < ? AND redacted = 0
             GROUP BY action
             ORDER BY c DESC, action ASC",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<String, _>("action")?, r.try_get::<i64, _>("c")?)))
            .collect()
    }

    pub async fn activity_stats_by_day(
        &self,
        since: i64,
        until: i64,
    ) -> sqlx::Result<Vec<(String, i64)>> {
        // Bucket by UTC day. Clients format to local timezone for display.
        let rows = sqlx::query(
            "SELECT strftime('%Y-%m-%d', ts/1000, 'unixepoch') AS day, COUNT(*) AS c
             FROM user_activity
             WHERE ts >= ? AND ts < ? AND redacted = 0
             GROUP BY day
             ORDER BY day ASC",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<String, _>("day")?, r.try_get::<i64, _>("c")?)))
            .collect()
    }

    pub async fn activity_stats_by_source(
        &self,
        since: i64,
        until: i64,
    ) -> sqlx::Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT source, COUNT(*) AS c
             FROM user_activity
             WHERE ts >= ? AND ts < ? AND redacted = 0
             GROUP BY source
             ORDER BY c DESC, source ASC",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<String, _>("source")?, r.try_get::<i64, _>("c")?)))
            .collect()
    }

    pub async fn activity_stats_by_target_kind(
        &self,
        since: i64,
        until: i64,
    ) -> sqlx::Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT COALESCE(target_kind, '') AS tk, COUNT(*) AS c
             FROM user_activity
             WHERE ts >= ? AND ts < ? AND redacted = 0 AND target_kind IS NOT NULL
             GROUP BY tk
             ORDER BY c DESC, tk ASC",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<String, _>("tk")?, r.try_get::<i64, _>("c")?)))
            .collect()
    }

    // ---- saved activity filters (Phase 8) ----

    pub async fn list_saved_activity_filters(&self) -> sqlx::Result<Vec<SavedActivityFilter>> {
        let rows = sqlx::query(
            "SELECT slug, name, filter_json, created_at, updated_at, last_used_at
             FROM saved_activity_filters
             ORDER BY last_used_at DESC, updated_at DESC",
        )
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| {
                Ok(SavedActivityFilter {
                    slug: r.try_get("slug")?,
                    name: r.try_get("name")?,
                    filter_json: r.try_get("filter_json")?,
                    created_at: r.try_get("created_at")?,
                    updated_at: r.try_get("updated_at")?,
                    last_used_at: r.try_get("last_used_at")?,
                })
            })
            .collect()
    }

    pub async fn get_saved_activity_filter(
        &self,
        slug: &str,
    ) -> sqlx::Result<Option<SavedActivityFilter>> {
        let row = sqlx::query(
            "SELECT slug, name, filter_json, created_at, updated_at, last_used_at
             FROM saved_activity_filters WHERE slug = ?",
        )
        .bind(slug)
        .fetch_optional(self.reader())
        .await?;
        row.map(|r| {
            Ok(SavedActivityFilter {
                slug: r.try_get("slug")?,
                name: r.try_get("name")?,
                filter_json: r.try_get("filter_json")?,
                created_at: r.try_get("created_at")?,
                updated_at: r.try_get("updated_at")?,
                last_used_at: r.try_get("last_used_at")?,
            })
        })
        .transpose()
    }

    pub async fn upsert_saved_activity_filter(
        &self,
        slug: &str,
        name: &str,
        filter_json: &str,
    ) -> sqlx::Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO saved_activity_filters (slug, name, filter_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(slug) DO UPDATE SET
                name = excluded.name,
                filter_json = excluded.filter_json,
                updated_at = excluded.updated_at",
        )
        .bind(slug)
        .bind(name)
        .bind(filter_json)
        .bind(now)
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn delete_saved_activity_filter(&self, slug: &str) -> sqlx::Result<u64> {
        let result = sqlx::query("DELETE FROM saved_activity_filters WHERE slug = ?")
            .bind(slug)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn mark_saved_activity_filter_used(&self, slug: &str) -> sqlx::Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE saved_activity_filters SET last_used_at = ? WHERE slug = ?")
            .bind(now)
            .bind(slug)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn activity_stats_by_hour(
        &self,
        since: i64,
        until: i64,
    ) -> sqlx::Result<Vec<(String, i64)>> {
        let rows = sqlx::query(
            "SELECT strftime('%H', ts/1000, 'unixepoch') AS hr, COUNT(*) AS c
             FROM user_activity
             WHERE ts >= ? AND ts < ? AND redacted = 0
             GROUP BY hr
             ORDER BY hr ASC",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|r| Ok((r.try_get::<String, _>("hr")?, r.try_get::<i64, _>("c")?)))
            .collect()
    }
}

fn push_filter_clauses<'a>(qb: &mut QueryBuilder<'a, sqlx::Sqlite>, filter: &'a ActivityFilter) {
    if let Some(since) = filter.since {
        qb.push(" AND ts >= ");
        qb.push_bind(since);
    }
    if let Some(until) = filter.until {
        qb.push(" AND ts < ");
        qb.push_bind(until);
    }
    if let Some(ref account_id) = filter.account_id {
        qb.push(" AND account_id = ");
        qb.push_bind(account_id.as_str());
    }
    if !filter.sources.is_empty() {
        qb.push(" AND source IN (");
        let mut sep = qb.separated(", ");
        for s in &filter.sources {
            sep.push_bind(s.as_str());
        }
        qb.push(")");
    }
    if !filter.actions.is_empty() {
        qb.push(" AND action IN (");
        let mut sep = qb.separated(", ");
        for a in &filter.actions {
            sep.push_bind(a.as_str());
        }
        qb.push(")");
    }
    if let Some(ref prefix) = filter.action_prefix {
        // SQLite LIKE supports `||` for runtime concat; pattern-bind safely.
        qb.push(" AND action LIKE ");
        qb.push_bind(format!("{prefix}%"));
    }
    if let Some(ref tk) = filter.target_kind {
        qb.push(" AND target_kind = ");
        qb.push_bind(tk.as_str());
    }
    if let Some(ref tid) = filter.target_id {
        qb.push(" AND target_id = ");
        qb.push_bind(tid.as_str());
    }
    if !filter.tiers.is_empty() {
        qb.push(" AND tier IN (");
        let mut sep = qb.separated(", ");
        for t in &filter.tiers {
            sep.push_bind(t.as_str());
        }
        qb.push(")");
    }
    if !filter.include_redacted {
        qb.push(" AND redacted = 0");
    }
    if let Some(ref q) = filter.query {
        qb.push(" AND id IN (SELECT rowid FROM user_activity_fts WHERE user_activity_fts MATCH ");
        qb.push_bind(q.as_str());
        qb.push(")");
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    async fn fresh_store() -> Store {
        Store::in_memory().await.unwrap()
    }

    fn ins<'a>(
        ts: i64,
        action: &'a str,
        tier: Tier,
        source: &'a str,
        target_kind: Option<&'a str>,
        target_id: Option<&'a str>,
        context: Option<&'a serde_json::Value>,
    ) -> ActivityInsert<'a> {
        ActivityInsert {
            ts,
            account_id: None,
            source,
            action,
            target_kind,
            target_id,
            tier,
            context,
        }
    }

    #[tokio::test]
    async fn record_returns_monotonic_ids_and_lists_newest_first() {
        let store = fresh_store().await;
        let ctx1 = json!({ "thread_id": "thr_1" });
        let ctx2 = json!({ "thread_id": "thr_2" });

        let id1 = store
            .record_activity(ins(
                1_000,
                "mail.archive",
                Tier::Important,
                "tui",
                Some("thread"),
                Some("thr_1"),
                Some(&ctx1),
            ))
            .await
            .unwrap();
        let id2 = store
            .record_activity(ins(
                2_000,
                "mail.archive",
                Tier::Important,
                "cli",
                Some("thread"),
                Some("thr_2"),
                Some(&ctx2),
            ))
            .await
            .unwrap();

        assert!(id2 > id1, "ids should be monotonic");

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 2);
        assert_eq!(page.rows[0].id, id2, "newest first");
        assert_eq!(page.rows[1].id, id1);
        assert_eq!(page.rows[0].source, "cli");
        assert_eq!(page.rows[1].source, "tui");
    }

    #[tokio::test]
    async fn filter_by_action_prefix_matches_only_the_prefix() {
        let store = fresh_store().await;
        store
            .record_activity_batch(&[
                ins(
                    100,
                    "mail.archive",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(200, "mail.send", Tier::Important, "cli", None, None, None),
                ins(300, "search.run", Tier::Standard, "tui", None, None, None),
                ins(
                    400,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "tui",
                    None,
                    None,
                    None,
                ),
            ])
            .await
            .unwrap();

        let mut filter = ActivityFilter::default();
        filter.action_prefix = Some("mail.".into());
        let page = store.list_activity(&filter, 10, None).await.unwrap();

        let actions: Vec<&str> = page.rows.iter().map(|r| r.action.as_str()).collect();
        assert_eq!(actions, vec!["mail.send", "mail.archive"]);
    }

    #[tokio::test]
    async fn filter_by_source_set_and_date_range() {
        let store = fresh_store().await;
        store
            .record_activity_batch(&[
                ins(
                    100,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(
                    200,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "web",
                    None,
                    None,
                    None,
                ),
                ins(
                    300,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "cli",
                    None,
                    None,
                    None,
                ),
                ins(
                    400,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "tui",
                    None,
                    None,
                    None,
                ),
            ])
            .await
            .unwrap();

        let mut filter = ActivityFilter {
            sources: vec!["tui".into(), "web".into()],
            since: Some(150),
            until: Some(450),
            ..Default::default()
        };
        let page = store.list_activity(&filter, 10, None).await.unwrap();
        let sources: Vec<&str> = page.rows.iter().map(|r| r.source.as_str()).collect();
        // ts=200(web), ts=400(tui) — both inside window. ts=100(tui) excluded by since; ts=300(cli) excluded by source.
        assert_eq!(sources, vec!["tui", "web"]);
        assert_eq!(page.rows[0].ts, 400);
        assert_eq!(page.rows[1].ts, 200);

        // count_activity should match
        filter.since = Some(150);
        let count = store.count_activity(&filter).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn cursor_pagination_yields_no_duplicates_and_no_gaps() {
        let store = fresh_store().await;
        // Insert 7 rows with strictly increasing ts.
        let entries: Vec<_> = (1..=7i64)
            .map(|i| {
                ins(
                    i * 100,
                    "mail.read",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                )
            })
            .collect();
        store.record_activity_batch(&entries).await.unwrap();

        let page1 = store
            .list_activity(&ActivityFilter::default(), 3, None)
            .await
            .unwrap();
        assert_eq!(page1.rows.len(), 3);
        assert!(page1.next_cursor.is_some());

        let page2 = store
            .list_activity(&ActivityFilter::default(), 3, page1.next_cursor)
            .await
            .unwrap();
        assert_eq!(page2.rows.len(), 3);
        assert!(page2.next_cursor.is_some());

        let page3 = store
            .list_activity(&ActivityFilter::default(), 3, page2.next_cursor)
            .await
            .unwrap();
        assert_eq!(page3.rows.len(), 1, "remainder");
        assert!(
            page3.next_cursor.is_none(),
            "partial page must clear cursor"
        );

        let mut all_ids: Vec<i64> = page1
            .rows
            .iter()
            .chain(page2.rows.iter())
            .chain(page3.rows.iter())
            .map(|r| r.id)
            .collect();
        all_ids.sort();
        all_ids.dedup();
        assert_eq!(all_ids.len(), 7, "no duplicates / no gaps across pages");
    }

    #[tokio::test]
    async fn redact_tombstones_clear_context_and_hide_by_default() {
        let store = fresh_store().await;
        let ctx = json!({ "thread_id": "thr_secret", "subject": "Private" });
        let id = store
            .record_activity(ins(
                1_000,
                "mail.read",
                Tier::Important,
                "tui",
                Some("thread"),
                Some("thr_secret"),
                Some(&ctx),
            ))
            .await
            .unwrap();

        let n = store.redact_activity_by_ids(&[id]).await.unwrap();
        assert_eq!(n, 1);

        // Default list excludes redacted rows.
        let default_page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert!(default_page.rows.is_empty());

        // Opt-in shows them, with cleared context.
        let mut filter = ActivityFilter::default();
        filter.include_redacted = true;
        let page = store.list_activity(&filter, 10, None).await.unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(page.rows[0].redacted);
        assert!(page.rows[0].context_json.is_none());
        // Audit-trail columns survive.
        assert_eq!(page.rows[0].action, "mail.read");
        assert_eq!(page.rows[0].target_id.as_deref(), Some("thr_secret"));
    }

    #[tokio::test]
    async fn redact_by_filter_targets_only_matching_rows() {
        let store = fresh_store().await;
        store
            .record_activity_batch(&[
                ins(100, "mail.read", Tier::Important, "tui", None, None, None),
                ins(
                    200,
                    "mail.archive",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(300, "search.run", Tier::Standard, "tui", None, None, None),
            ])
            .await
            .unwrap();

        let mut filter = ActivityFilter::default();
        filter.action_prefix = Some("mail.".into());
        let n = store.redact_activity_by_filter(&filter).await.unwrap();
        assert_eq!(n, 2);

        // search.run survives, unredacted.
        let remaining = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(remaining.rows.len(), 1);
        assert_eq!(remaining.rows[0].action, "search.run");
    }

    #[tokio::test]
    async fn prune_by_tier_only_deletes_matching_tier() {
        let store = fresh_store().await;
        store
            .record_activity_batch(&[
                ins(
                    100,
                    "view.open_screen",
                    Tier::Ephemeral,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(110, "mail.read", Tier::Important, "tui", None, None, None),
                ins(120, "search.run", Tier::Standard, "tui", None, None, None),
            ])
            .await
            .unwrap();

        let n = store
            .prune_activity_before(1_000, Some(Tier::Ephemeral))
            .await
            .unwrap();
        assert_eq!(n, 1);

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        let actions: Vec<&str> = page.rows.iter().map(|r| r.action.as_str()).collect();
        // Newest first: search.run(120), mail.read(110)
        assert_eq!(actions, vec!["search.run", "mail.read"]);
    }

    #[tokio::test]
    async fn fts_query_finds_subject_substring_and_excludes_after_redaction() {
        let store = fresh_store().await;
        let ctx_invoice = json!({ "query": "invoice 2026", "result_count": 12 });
        let ctx_other = json!({ "query": "vacation pictures" });
        let id = store
            .record_activity(ins(
                100,
                "search.run",
                Tier::Standard,
                "tui",
                Some("search"),
                None,
                Some(&ctx_invoice),
            ))
            .await
            .unwrap();
        store
            .record_activity(ins(
                200,
                "search.run",
                Tier::Standard,
                "tui",
                Some("search"),
                None,
                Some(&ctx_other),
            ))
            .await
            .unwrap();

        let mut filter = ActivityFilter::default();
        filter.query = Some("invoice".into());
        let page = store.list_activity(&filter, 10, None).await.unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].id, id);

        // After redaction the FTS row is removed.
        store.redact_activity_by_ids(&[id]).await.unwrap();
        let page2 = store.list_activity(&filter, 10, None).await.unwrap();
        assert!(page2.rows.is_empty(), "FTS no longer matches redacted row");
    }

    #[tokio::test]
    async fn stats_by_action_excludes_redacted_and_groups_correctly() {
        let store = fresh_store().await;
        store
            .record_activity_batch(&[
                ins(
                    100,
                    "mail.archive",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(
                    200,
                    "mail.archive",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(
                    300,
                    "mail.archive",
                    Tier::Important,
                    "tui",
                    None,
                    None,
                    None,
                ),
                ins(400, "mail.read", Tier::Important, "tui", None, None, None),
                ins(500, "mail.read", Tier::Important, "tui", None, None, None),
            ])
            .await
            .unwrap();
        // Redact one mail.archive so it doesn't get counted.
        store.redact_activity_by_ids(&[1]).await.unwrap();

        let buckets = store.activity_stats_by_action(0, 1_000).await.unwrap();
        // mail.archive=2 (after redaction), mail.read=2 — alphabetical tie-break for equal counts.
        assert_eq!(
            buckets,
            vec![
                ("mail.archive".to_string(), 2),
                ("mail.read".to_string(), 2),
            ]
        );
    }

    #[tokio::test]
    async fn empty_filter_with_no_rows_yields_empty_page_and_no_cursor() {
        let store = fresh_store().await;
        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert!(page.rows.is_empty());
        assert!(page.next_cursor.is_none());

        let n = store.redact_activity_by_ids(&[]).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn saved_filter_upsert_overwrites_on_slug_collision() {
        let store = fresh_store().await;
        store
            .upsert_saved_activity_filter("mail-week", "Mail this week", r#"{"prefix":"mail."}"#)
            .await
            .unwrap();
        store
            .upsert_saved_activity_filter("mail-week", "Renamed", r#"{"prefix":"mail.archive"}"#)
            .await
            .unwrap();

        let got = store
            .get_saved_activity_filter("mail-week")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.name, "Renamed");
        assert!(got.filter_json.contains("mail.archive"));
    }

    #[tokio::test]
    async fn saved_filter_mark_used_updates_last_used_at() {
        let store = fresh_store().await;
        store
            .upsert_saved_activity_filter("x", "X", "{}")
            .await
            .unwrap();
        let before = store.get_saved_activity_filter("x").await.unwrap().unwrap();
        assert!(before.last_used_at.is_none());

        store.mark_saved_activity_filter_used("x").await.unwrap();
        let after = store.get_saved_activity_filter("x").await.unwrap().unwrap();
        assert!(after.last_used_at.is_some());
    }

    #[tokio::test]
    async fn saved_filter_delete_is_idempotent() {
        let store = fresh_store().await;
        store
            .upsert_saved_activity_filter("y", "Y", "{}")
            .await
            .unwrap();
        let n = store.delete_saved_activity_filter("y").await.unwrap();
        assert_eq!(n, 1);
        let n2 = store.delete_saved_activity_filter("y").await.unwrap();
        assert_eq!(n2, 0);
    }

    #[tokio::test]
    async fn limit_zero_uses_default_and_max_caps_at_500() {
        let store = fresh_store().await;
        let entries: Vec<_> = (1..=520i64)
            .map(|i| ins(i, "mail.read", Tier::Important, "tui", None, None, None))
            .collect();
        store.record_activity_batch(&entries).await.unwrap();

        // limit=0 → default 50
        let page = store
            .list_activity(&ActivityFilter::default(), 0, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 50);

        // limit=1000 → caps at 500
        let page = store
            .list_activity(&ActivityFilter::default(), 1000, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 500);
    }
}
