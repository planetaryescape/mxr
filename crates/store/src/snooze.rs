use chrono::{DateTime, Utc};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn insert_snooze(&self, snoozed: &Snoozed) -> Result<(), sqlx::Error> {
        let original_labels = serde_json::to_string(&snoozed.original_labels).unwrap();

        sqlx::query(
            "INSERT INTO snoozed (message_id, account_id, snoozed_at, wake_at, original_labels)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(snoozed.message_id.as_str())
        .bind(snoozed.account_id.as_str())
        .bind(snoozed.snoozed_at.timestamp())
        .bind(snoozed.wake_at.timestamp())
        .bind(&original_labels)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_due_snoozes(&self, now: DateTime<Utc>) -> Result<Vec<Snoozed>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM snoozed WHERE wake_at <= ?")
            .bind(now.timestamp())
            .fetch_all(self.reader())
            .await?;

        Ok(rows.iter().map(row_to_snoozed).collect())
    }

    pub async fn remove_snooze(&self, message_id: &MessageId) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM snoozed WHERE message_id = ?")
            .bind(message_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }
}

fn row_to_snoozed(row: &sqlx::sqlite::SqliteRow) -> Snoozed {
    let msg_id_str: String = row.get("message_id");
    let account_id_str: String = row.get("account_id");
    let snoozed_at_ts: i64 = row.get("snoozed_at");
    let wake_at_ts: i64 = row.get("wake_at");
    let labels_json: String = row.get("original_labels");

    Snoozed {
        message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&msg_id_str).unwrap()),
        account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id_str).unwrap()),
        snoozed_at: DateTime::from_timestamp(snoozed_at_ts, 0).unwrap_or_default(),
        wake_at: DateTime::from_timestamp(wake_at_ts, 0).unwrap_or_default(),
        original_labels: serde_json::from_str(&labels_json).unwrap_or_default(),
    }
}
