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

impl super::Store {
    pub async fn insert_event(
        &self,
        level: &str,
        category: &str,
        summary: &str,
        account_id: Option<&AccountId>,
        details: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let aid = account_id.map(|a| a.as_str());
        sqlx::query!(
            "INSERT INTO event_log (timestamp, level, category, account_id, summary, details)
             VALUES (?, ?, ?, ?, ?, ?)",
            now,
            level,
            category,
            aid,
            summary,
            details,
        )
        .execute(self.writer())
        .await?;
        Ok(())
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

        Ok(rows
            .iter()
            .map(|r| {
                let aid: Option<String> = r.get("account_id");
                EventLogEntry {
                    id: r.get("id"),
                    timestamp: r.get("timestamp"),
                    level: r.get("level"),
                    category: r.get("category"),
                    account_id: aid
                        .map(|s| AccountId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
                    message_id: r.get("message_id"),
                    rule_id: r.get("rule_id"),
                    summary: r.get("summary"),
                    details: r.get("details"),
                }
            })
            .collect())
    }

    pub async fn prune_events_before(&self, cutoff_timestamp: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
            .bind(cutoff_timestamp)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }
}
