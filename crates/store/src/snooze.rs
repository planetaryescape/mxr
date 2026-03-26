use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use crate::mxr_store::{
    decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query,
};
use chrono::{DateTime, Utc};

impl super::Store {
    pub async fn insert_snooze(&self, snoozed: &Snoozed) -> Result<(), sqlx::Error> {
        let mid = snoozed.message_id.as_str();
        let aid = snoozed.account_id.as_str();
        let original_labels = encode_json(&snoozed.original_labels)?;
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
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!", account_id as "account_id!",
                      snoozed_at as "snoozed_at!", wake_at as "wake_at!",
                      original_labels as "original_labels!"
               FROM snoozed WHERE wake_at <= ?"#,
            now_ts,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("snooze.get_due_snoozes", started_at, rows.len());

        rows
            .into_iter()
            .map(|r| {
                Ok(Snoozed {
                    message_id: decode_id(&r.message_id)?,
                    account_id: decode_id(&r.account_id)?,
                    snoozed_at: decode_timestamp(r.snoozed_at)?,
                    wake_at: decode_timestamp(r.wake_at)?,
                    original_labels: decode_json(&r.original_labels)?,
                })
            })
            .collect()
    }

    pub async fn list_snoozed(&self) -> Result<Vec<Snoozed>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!", account_id as "account_id!",
                      snoozed_at as "snoozed_at!", wake_at as "wake_at!",
                      original_labels as "original_labels!"
               FROM snoozed ORDER BY wake_at ASC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("snooze.list_snoozed", started_at, rows.len());

        rows
            .into_iter()
            .map(|r| {
                Ok(Snoozed {
                    message_id: decode_id(&r.message_id)?,
                    account_id: decode_id(&r.account_id)?,
                    snoozed_at: decode_timestamp(r.snoozed_at)?,
                    wake_at: decode_timestamp(r.wake_at)?,
                    original_labels: decode_json(&r.original_labels)?,
                })
            })
            .collect()
    }

    pub async fn get_snooze(&self, message_id: &MessageId) -> Result<Option<Snoozed>, sqlx::Error> {
        let mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query_as::<_, (String, String, i64, i64, String)>(
            r#"SELECT message_id, account_id, snoozed_at, wake_at, original_labels
               FROM snoozed WHERE message_id = ?"#,
        )
        .bind(mid)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("snooze.get_snooze", started_at, row.is_some());

        row.map(
            |(message_id, account_id, snoozed_at, wake_at, original_labels)| {
                Ok(Snoozed {
                    message_id: decode_id(&message_id)?,
                    account_id: decode_id(&account_id)?,
                    snoozed_at: decode_timestamp(snoozed_at)?,
                    wake_at: decode_timestamp(wake_at)?,
                    original_labels: decode_json(&original_labels)?,
                })
            },
        )
        .transpose()
    }

    pub async fn remove_snooze(&self, message_id: &MessageId) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        sqlx::query!("DELETE FROM snoozed WHERE message_id = ?", mid)
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
