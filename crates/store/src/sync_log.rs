use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_lookup};
use chrono::{DateTime, Utc};
use mxr_core::AccountId;

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
        let aid = account_id.as_str();
        let status_str = status.as_str();
        let result = sqlx::query!(
            "INSERT INTO sync_log (account_id, started_at, status) VALUES (?, ?, ?)",
            aid,
            now,
            status_str,
        )
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
        let status_str = status.as_str();
        sqlx::query!(
            "UPDATE sync_log SET finished_at = ?, status = ?, messages_synced = ?, error_message = ? WHERE id = ?",
            now,
            status_str,
            messages_synced,
            error_message,
            log_id,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_last_sync(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncLogEntry>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!", started_at as "started_at!",
                      finished_at, status as "status!", messages_synced as "messages_synced!",
                      error_message
               FROM sync_log WHERE account_id = ? ORDER BY started_at DESC LIMIT 1"#,
            aid,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("sync_log.get_last_sync", started_at, row.is_some());

        row.map(|r| {
            Ok(SyncLogEntry {
                id: r.id,
                account_id: decode_id(&r.account_id)?,
                started_at: decode_timestamp(r.started_at)?,
                finished_at: decode_optional_timestamp(r.finished_at)?,
                status: SyncStatus::from_str(&r.status),
                messages_synced: r.messages_synced as u32,
                error_message: r.error_message,
            })
        })
        .transpose()
    }
}
