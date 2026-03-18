use chrono::{DateTime, Utc};
use mxr_core::AccountId;
use sqlx::Row;

pub struct SyncLogEntry {
    pub id: i64,
    pub account_id: AccountId,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: SyncStatus,
    pub messages_synced: u32,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncStatus {
    Running,
    Success,
    Error,
}

impl SyncStatus {
    fn as_str(&self) -> &str {
        match self {
            SyncStatus::Running => "running",
            SyncStatus::Success => "success",
            SyncStatus::Error => "error",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "running" => SyncStatus::Running,
            "success" => SyncStatus::Success,
            _ => SyncStatus::Error,
        }
    }
}

impl super::Store {
    pub async fn insert_sync_log(
        &self,
        account_id: &AccountId,
        status: &SyncStatus,
    ) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp();
        let result =
            sqlx::query("INSERT INTO sync_log (account_id, started_at, status) VALUES (?, ?, ?)")
                .bind(account_id.as_str())
                .bind(now)
                .bind(status.as_str())
                .execute(self.writer())
                .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn complete_sync_log(
        &self,
        log_id: i64,
        status: &SyncStatus,
        messages_synced: u32,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "UPDATE sync_log SET finished_at = ?, status = ?, messages_synced = ?, error_message = ? WHERE id = ?",
        )
        .bind(now)
        .bind(status.as_str())
        .bind(messages_synced)
        .bind(error_message)
        .bind(log_id)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_last_sync(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncLogEntry>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT * FROM sync_log WHERE account_id = ? ORDER BY started_at DESC LIMIT 1",
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;

        Ok(row.as_ref().map(|r| {
            let aid_str: String = r.get("account_id");
            let started_at_ts: i64 = r.get("started_at");
            let finished_at_ts: Option<i64> = r.get("finished_at");
            let status_str: String = r.get("status");
            let messages_synced: u32 = r.get::<u32, _>("messages_synced");

            SyncLogEntry {
                id: r.get("id"),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&aid_str).unwrap()),
                started_at: DateTime::from_timestamp(started_at_ts, 0).unwrap_or_default(),
                finished_at: finished_at_ts.and_then(|ts| DateTime::from_timestamp(ts, 0)),
                status: SyncStatus::from_str(&status_str),
                messages_synced,
                error_message: r.get("error_message"),
            }
        }))
    }
}
