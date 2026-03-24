use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use chrono::{DateTime, Utc};

impl super::Store {
    pub async fn insert_snooze(&self, snoozed: &Snoozed) -> Result<(), sqlx::Error> {
        let mid = snoozed.message_id.as_str();
        let aid = snoozed.account_id.as_str();
        let original_labels = serde_json::to_string(&snoozed.original_labels).unwrap();
        let snoozed_at = snoozed.snoozed_at.timestamp();
        let wake_at = snoozed.wake_at.timestamp();

        sqlx::query!(
            "INSERT INTO snoozed (message_id, account_id, snoozed_at, wake_at, original_labels)
             VALUES (?, ?, ?, ?, ?)",
            mid,
            aid,
            snoozed_at,
            wake_at,
            original_labels,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_due_snoozes(&self, now: DateTime<Utc>) -> Result<Vec<Snoozed>, sqlx::Error> {
        let now_ts = now.timestamp();
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!", account_id as "account_id!",
                      snoozed_at as "snoozed_at!", wake_at as "wake_at!",
                      original_labels as "original_labels!"
               FROM snoozed WHERE wake_at <= ?"#,
            now_ts,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Snoozed {
                message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&r.message_id).unwrap()),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
                snoozed_at: DateTime::from_timestamp(r.snoozed_at, 0).unwrap_or_default(),
                wake_at: DateTime::from_timestamp(r.wake_at, 0).unwrap_or_default(),
                original_labels: serde_json::from_str(&r.original_labels).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn list_snoozed(&self) -> Result<Vec<Snoozed>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!", account_id as "account_id!",
                      snoozed_at as "snoozed_at!", wake_at as "wake_at!",
                      original_labels as "original_labels!"
               FROM snoozed ORDER BY wake_at ASC"#,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Snoozed {
                message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&r.message_id).unwrap()),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
                snoozed_at: DateTime::from_timestamp(r.snoozed_at, 0).unwrap_or_default(),
                wake_at: DateTime::from_timestamp(r.wake_at, 0).unwrap_or_default(),
                original_labels: serde_json::from_str(&r.original_labels).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn get_snooze(&self, message_id: &MessageId) -> Result<Option<Snoozed>, sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query_as::<_, (String, String, i64, i64, String)>(
            r#"SELECT message_id, account_id, snoozed_at, wake_at, original_labels
               FROM snoozed WHERE message_id = ?"#,
        )
        .bind(mid)
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(
            |(message_id, account_id, snoozed_at, wake_at, original_labels)| Snoozed {
                message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&message_id).unwrap()),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id).unwrap()),
                snoozed_at: DateTime::from_timestamp(snoozed_at, 0).unwrap_or_default(),
                wake_at: DateTime::from_timestamp(wake_at, 0).unwrap_or_default(),
                original_labels: serde_json::from_str(&original_labels).unwrap_or_default(),
            },
        ))
    }

    pub async fn remove_snooze(&self, message_id: &MessageId) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        sqlx::query!("DELETE FROM snoozed WHERE message_id = ?", mid)
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
