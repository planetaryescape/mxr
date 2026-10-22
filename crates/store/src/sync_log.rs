use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_lookup};
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
            Self::Running => "running",
            Self::Success => "success",
            Self::Error => "error",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "success" => Self::Success,
            _ => Self::Error,
        }
    }
}

/// Rows ranked newest-first within each account, in the order
/// `list_recent_sync_logs` reads them, so ties resolve the same way for the
/// prune as for doctor's history window.
const RANKED_SYNC_LOGS: &str = "SELECT id, started_at, ROW_NUMBER() OVER (
    PARTITION BY account_id ORDER BY started_at DESC, id DESC) AS rn
    FROM sync_log";

/// Rows a prune may delete: older than the cutoff and outside each account's
/// newest `keep` rows. `?1` is the cutoff (unix seconds), `?2` is `keep`.
const PRUNABLE_SYNC_LOGS: &str = "rn > ?2 AND started_at < ?1";

/// Number of sync_log rows a prune with this cutoff and keep count would
/// delete. Shares `PRUNABLE_SYNC_LOGS` with `Store::prune_sync_logs_before`.
pub async fn count_prunable_sync_logs(
    conn: &mut sqlx::SqliteConnection,
    cutoff_timestamp: i64,
    keep_newest_per_account: u32,
) -> Result<u64, sqlx::Error> {
    let sql = format!("SELECT COUNT(*) FROM ({RANKED_SYNC_LOGS}) WHERE {PRUNABLE_SYNC_LOGS}");
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(cutoff_timestamp)
        .bind(i64::from(keep_newest_per_account))
        .fetch_one(&mut *conn)
        .await?;
    Ok(u64::try_from(count).unwrap_or(0))
}

impl super::Store {
    /// Deletes sync_log rows older than `cutoff_timestamp` (unix seconds)
    /// except each account's newest `keep_newest_per_account` rows, which stay
    /// regardless of age so a rarely syncing account keeps its history. Runs in
    /// batches of `PRUNE_BATCH_ROWS`. A sync that is
    /// running while this runs is either outside the window (newest rows are
    /// never prunable) or completes its own row by id, which is a no-op if
    /// that row was already deleted.
    pub async fn prune_sync_logs_before(
        &self,
        cutoff_timestamp: i64,
        keep_newest_per_account: u32,
    ) -> Result<u64, sqlx::Error> {
        let sql = format!(
            "DELETE FROM sync_log WHERE id IN \
             (SELECT id FROM ({RANKED_SYNC_LOGS}) WHERE {PRUNABLE_SYNC_LOGS} LIMIT ?3)"
        );
        let batch = crate::event_log::PRUNE_BATCH_ROWS;
        let mut total = 0;
        loop {
            let deleted = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(cutoff_timestamp)
                .bind(i64::from(keep_newest_per_account))
                .bind(batch)
                .execute(self.writer())
                .await?
                .rows_affected();
            total += deleted;
            if deleted < batch as u64 {
                return Ok(total);
            }
        }
    }

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

    /// Newest-first sync rows for one account, for the doctor's duration and
    /// error summary. Unchecked query (like `list_message_ids_by_account`) so
    /// the `.sqlx` cache does not need regenerating for a diagnostics read.
    pub async fn list_recent_sync_logs(
        &self,
        account_id: &AccountId,
        limit: u32,
    ) -> Result<Vec<SyncLogEntry>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, account_id, started_at, finished_at, status, messages_synced, error_message
             FROM sync_log WHERE account_id = ?1
             ORDER BY started_at DESC, id DESC LIMIT ?2",
        )
        .bind(account_id.as_str())
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(SyncLogEntry {
                    id: row.try_get("id")?,
                    account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                    started_at: decode_timestamp(row.try_get("started_at")?)?,
                    finished_at: decode_optional_timestamp(row.try_get("finished_at")?)?,
                    status: SyncStatus::from_str(&row.try_get::<String, _>("status")?),
                    messages_synced: row.try_get::<i64, _>("messages_synced")? as u32,
                    error_message: row.try_get("error_message")?,
                })
            })
            .collect()
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
