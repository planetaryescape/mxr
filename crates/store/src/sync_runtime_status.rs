use chrono::{DateTime, Utc};
use mxr_core::AccountId;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRuntimeStatus {
    pub account_id: AccountId,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub failure_class: Option<String>,
    pub consecutive_failures: u32,
    pub backoff_until: Option<DateTime<Utc>>,
    pub sync_in_progress: bool,
    pub current_cursor_summary: Option<String>,
    pub last_synced_count: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct SyncRuntimeStatusUpdate {
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<Option<String>>,
    pub failure_class: Option<Option<String>>,
    pub consecutive_failures: Option<u32>,
    pub backoff_until: Option<Option<DateTime<Utc>>>,
    pub sync_in_progress: Option<bool>,
    pub current_cursor_summary: Option<Option<String>>,
    pub last_synced_count: Option<u32>,
}

fn ts_or_default(ts: Option<i64>) -> Option<DateTime<Utc>> {
    ts.and_then(|value| DateTime::from_timestamp(value, 0))
}

fn row_to_sync_runtime_status(row: &sqlx::sqlite::SqliteRow) -> SyncRuntimeStatus {
    SyncRuntimeStatus {
        account_id: AccountId::from_uuid(
            uuid::Uuid::parse_str(&row.get::<String, _>(0)).expect("valid account uuid"),
        ),
        last_attempt_at: ts_or_default(row.get(1)),
        last_success_at: ts_or_default(row.get(2)),
        last_error: row.get(3),
        failure_class: row.get(4),
        consecutive_failures: row.get::<i64, _>(5) as u32,
        backoff_until: ts_or_default(row.get(6)),
        sync_in_progress: row.get::<i64, _>(7) != 0,
        current_cursor_summary: row.get(8),
        last_synced_count: row.get::<i64, _>(9) as u32,
        updated_at: DateTime::from_timestamp(row.get(10), 0).unwrap_or_default(),
    }
}

impl super::Store {
    pub async fn upsert_sync_runtime_status(
        &self,
        account_id: &AccountId,
        update: &SyncRuntimeStatusUpdate,
    ) -> Result<(), sqlx::Error> {
        let existing = self.get_sync_runtime_status(account_id).await?;
        let now = Utc::now();
        let merged = SyncRuntimeStatus {
            account_id: account_id.clone(),
            last_attempt_at: update
                .last_attempt_at
                .or(existing.as_ref().and_then(|row| row.last_attempt_at)),
            last_success_at: update
                .last_success_at
                .or(existing.as_ref().and_then(|row| row.last_success_at)),
            last_error: update
                .last_error
                .clone()
                .unwrap_or_else(|| existing.as_ref().and_then(|row| row.last_error.clone())),
            failure_class: update
                .failure_class
                .clone()
                .unwrap_or_else(|| existing.as_ref().and_then(|row| row.failure_class.clone())),
            consecutive_failures: update.consecutive_failures.unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|row| row.consecutive_failures)
                    .unwrap_or(0)
            }),
            backoff_until: update
                .backoff_until
                .unwrap_or_else(|| existing.as_ref().and_then(|row| row.backoff_until)),
            sync_in_progress: update.sync_in_progress.unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|row| row.sync_in_progress)
                    .unwrap_or(false)
            }),
            current_cursor_summary: update.current_cursor_summary.clone().unwrap_or_else(|| {
                existing
                    .as_ref()
                    .and_then(|row| row.current_cursor_summary.clone())
            }),
            last_synced_count: update.last_synced_count.unwrap_or_else(|| {
                existing
                    .as_ref()
                    .map(|row| row.last_synced_count)
                    .unwrap_or(0)
            }),
            updated_at: now,
        };

        sqlx::query(
            r#"
            INSERT INTO sync_runtime_status (
                account_id,
                last_attempt_at,
                last_success_at,
                last_error,
                failure_class,
                consecutive_failures,
                backoff_until,
                sync_in_progress,
                current_cursor_summary,
                last_synced_count,
                updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(account_id) DO UPDATE SET
                last_attempt_at = excluded.last_attempt_at,
                last_success_at = excluded.last_success_at,
                last_error = excluded.last_error,
                failure_class = excluded.failure_class,
                consecutive_failures = excluded.consecutive_failures,
                backoff_until = excluded.backoff_until,
                sync_in_progress = excluded.sync_in_progress,
                current_cursor_summary = excluded.current_cursor_summary,
                last_synced_count = excluded.last_synced_count,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(account_id.as_str())
        .bind(merged.last_attempt_at.map(|dt| dt.timestamp()))
        .bind(merged.last_success_at.map(|dt| dt.timestamp()))
        .bind(merged.last_error)
        .bind(merged.failure_class)
        .bind(merged.consecutive_failures as i64)
        .bind(merged.backoff_until.map(|dt| dt.timestamp()))
        .bind(merged.sync_in_progress)
        .bind(merged.current_cursor_summary)
        .bind(merged.last_synced_count as i64)
        .bind(merged.updated_at.timestamp())
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_sync_runtime_status(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncRuntimeStatus>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                account_id,
                last_attempt_at,
                last_success_at,
                last_error,
                failure_class,
                consecutive_failures,
                backoff_until,
                sync_in_progress,
                current_cursor_summary,
                last_synced_count,
                updated_at
            FROM sync_runtime_status
            WHERE account_id = ?
            "#,
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(|row| row_to_sync_runtime_status(&row)))
    }

    pub async fn list_sync_runtime_statuses(&self) -> Result<Vec<SyncRuntimeStatus>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                account_id,
                last_attempt_at,
                last_success_at,
                last_error,
                failure_class,
                consecutive_failures,
                backoff_until,
                sync_in_progress,
                current_cursor_summary,
                last_synced_count,
                updated_at
            FROM sync_runtime_status
            ORDER BY updated_at DESC, account_id ASC
            "#,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| row_to_sync_runtime_status(&row))
            .collect())
    }
}
