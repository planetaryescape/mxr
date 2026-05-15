use mxr_core::AccountId;
use sqlx::Row;

pub struct EventLogEntry {
    pub id: i64,
    pub timestamp: i64,
    pub level: String,
    pub category: String,
    pub account_id: Option<AccountId>,
    pub message_id: Option<String>,
    pub rule_id: Option<String>,
    pub summary: String,
    pub details: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EventLogRefs<'a> {
    pub account_id: Option<&'a AccountId>,
    pub message_id: Option<&'a str>,
    pub rule_id: Option<&'a str>,
}

/// Filter shape for paged event-log queries. All fields optional. Empty =
/// no constraint on that field.
#[derive(Clone, Copy, Debug, Default)]
pub struct EventLogFilter<'a> {
    pub limit: u32,
    pub offset: u32,
    pub level: Option<&'a str>,
    pub category: Option<&'a str>,
    pub category_prefix: Option<&'a str>,
    /// Unix-seconds inclusive lower bound on `timestamp`.
    pub since: Option<i64>,
    /// Unix-seconds exclusive upper bound on `timestamp`.
    pub until: Option<i64>,
    /// Free-text substring against `summary` and `details`. Case-insensitive
    /// at the SQL layer (we lower-case at the call site to keep callers honest).
    pub search: Option<&'a str>,
}

impl super::Store {
    pub async fn insert_event_refs(
        &self,
        level: &str,
        category: &str,
        summary: &str,
        refs: EventLogRefs<'_>,
        details: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let aid = refs.account_id.map(|account_id| account_id.as_str());
        sqlx::query(
            "INSERT INTO event_log (
                timestamp,
                level,
                category,
                account_id,
                message_id,
                rule_id,
                summary,
                details
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(now)
        .bind(level)
        .bind(category)
        .bind(aid)
        .bind(refs.message_id)
        .bind(refs.rule_id)
        .bind(summary)
        .bind(details)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn insert_event(
        &self,
        level: &str,
        category: &str,
        summary: &str,
        account_id: Option<&AccountId>,
        details: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.insert_event_refs(
            level,
            category,
            summary,
            EventLogRefs {
                account_id,
                ..EventLogRefs::default()
            },
            details,
        )
        .await
    }

    // Dynamic SQL — kept as runtime query since the WHERE clause is conditionally built
    pub async fn list_events(
        &self,
        limit: u32,
        level: Option<&str>,
        category: Option<&str>,
    ) -> Result<Vec<EventLogEntry>, sqlx::Error> {
        self.list_events_filtered(EventLogFilter {
            limit,
            level,
            category,
            ..EventLogFilter::default()
        })
        .await
    }

    /// Richer event-log query. Use the named `EventLogFilter` struct so
    /// callers don't need positional arg sprawl as fields grow.
    pub async fn list_events_filtered<'a>(
        &self,
        f: EventLogFilter<'a>,
    ) -> Result<Vec<EventLogEntry>, sqlx::Error> {
        let mut sql = String::from("SELECT * FROM event_log WHERE 1=1");
        if f.level.is_some() {
            sql.push_str(" AND level = ?");
        }
        if f.category.is_some() {
            sql.push_str(" AND category = ?");
        }
        if f.category_prefix.is_some() {
            sql.push_str(" AND category LIKE ?");
        }
        if f.since.is_some() {
            sql.push_str(" AND timestamp >= ?");
        }
        if f.until.is_some() {
            sql.push_str(" AND timestamp < ?");
        }
        if f.search.is_some() {
            sql.push_str(" AND (summary LIKE ? OR details LIKE ?)");
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

        let mut query = sqlx::query(&sql);
        if let Some(l) = f.level {
            query = query.bind(l);
        }
        if let Some(c) = f.category {
            query = query.bind(c);
        }
        if let Some(p) = f.category_prefix {
            query = query.bind(format!("{p}%"));
        }
        if let Some(since) = f.since {
            query = query.bind(since);
        }
        if let Some(until) = f.until {
            query = query.bind(until);
        }
        if let Some(s) = f.search {
            let like = format!("%{s}%");
            query = query.bind(like.clone()).bind(like);
        }
        query = query.bind(f.limit).bind(f.offset);

        let rows = query.fetch_all(self.reader()).await?;

        rows.iter()
            .map(|r| {
                let aid: Option<String> = r.get("account_id");
                Ok(EventLogEntry {
                    id: r.get("id"),
                    timestamp: r.get("timestamp"),
                    level: r.get("level"),
                    category: r.get("category"),
                    account_id: aid.map(|value| decode_id(&value)).transpose()?,
                    message_id: r.get("message_id"),
                    rule_id: r.get("rule_id"),
                    summary: r.get("summary"),
                    details: r.get("details"),
                })
            })
            .collect()
    }

    /// Distinct list of categories seen in the event log, ordered by
    /// recency. Cheap (bounded result) — used to populate the category
    /// dropdown in the diagnostics view.
    pub async fn list_event_categories(&self) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT category FROM event_log
             GROUP BY category
             ORDER BY MAX(timestamp) DESC
             LIMIT 100",
        )
        .fetch_all(self.reader())
        .await?;
        Ok(rows)
    }

    /// Count events matching a filter, ignoring limit/offset. Powers
    /// the "showing N of M" rendering in diagnostics surfaces.
    pub async fn count_events_filtered<'a>(
        &self,
        f: EventLogFilter<'a>,
    ) -> Result<i64, sqlx::Error> {
        let mut sql = String::from("SELECT COUNT(*) FROM event_log WHERE 1=1");
        if f.level.is_some() {
            sql.push_str(" AND level = ?");
        }
        if f.category.is_some() {
            sql.push_str(" AND category = ?");
        }
        if f.category_prefix.is_some() {
            sql.push_str(" AND category LIKE ?");
        }
        if f.since.is_some() {
            sql.push_str(" AND timestamp >= ?");
        }
        if f.until.is_some() {
            sql.push_str(" AND timestamp < ?");
        }
        if f.search.is_some() {
            sql.push_str(" AND (summary LIKE ? OR details LIKE ?)");
        }
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        if let Some(l) = f.level {
            query = query.bind(l);
        }
        if let Some(c) = f.category {
            query = query.bind(c);
        }
        if let Some(p) = f.category_prefix {
            query = query.bind(format!("{p}%"));
        }
        if let Some(since) = f.since {
            query = query.bind(since);
        }
        if let Some(until) = f.until {
            query = query.bind(until);
        }
        if let Some(s) = f.search {
            let like = format!("%{s}%");
            query = query.bind(like.clone()).bind(like);
        }
        query.fetch_one(self.reader()).await
    }

    pub async fn prune_events_before(&self, cutoff_timestamp: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
            .bind(cutoff_timestamp)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }

    /// Returns true when at least one event with this `category` exists
    /// for the given `message_id` whose `summary` contains the supplied
    /// substring. Used by idempotency-sensitive mutations (e.g.
    /// unsubscribe) to short-circuit when the action has already
    /// succeeded for the same target.
    pub async fn has_event_for_message_with_summary(
        &self,
        message_id: &str,
        category: &str,
        summary_substr: &str,
    ) -> Result<bool, sqlx::Error> {
        let row: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM event_log
             WHERE message_id = ?
               AND category = ?
               AND summary LIKE ?
             LIMIT 1",
        )
        .bind(message_id)
        .bind(category)
        .bind(format!("%{summary_substr}%"))
        .fetch_optional(self.reader())
        .await?;
        Ok(row.is_some())
    }

    pub async fn latest_event_timestamp(
        &self,
        category: &str,
        summary_prefix: Option<&str>,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
        let mut sql = String::from("SELECT timestamp FROM event_log WHERE category = ?");
        if summary_prefix.is_some() {
            sql.push_str(" AND summary LIKE ?");
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT 1");

        let mut query = sqlx::query_scalar::<_, i64>(&sql).bind(category);
        if let Some(prefix) = summary_prefix {
            query = query.bind(format!("{prefix}%"));
        }

        Ok(query
            .fetch_optional(self.reader())
            .await?
            .and_then(|timestamp| chrono::DateTime::from_timestamp(timestamp, 0)))
    }
}
use crate::decode_id;
