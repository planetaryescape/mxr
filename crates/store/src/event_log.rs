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
        let mut sql = String::from("SELECT * FROM event_log WHERE 1=1");
        if level.is_some() {
            sql.push_str(" AND level = ?");
        }
        if category.is_some() {
            sql.push_str(" AND category = ?");
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");

        let mut query = sqlx::query(&sql);
        if let Some(l) = level {
            query = query.bind(l);
        }
        if let Some(c) = category {
            query = query.bind(c);
        }
        query = query.bind(limit);

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

    pub async fn prune_events_before(&self, cutoff_timestamp: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
            .bind(cutoff_timestamp)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
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
