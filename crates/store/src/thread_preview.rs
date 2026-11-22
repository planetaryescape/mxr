//! A thread's headline facts without its bodies: the small read behind
//! hover cards and `mxr thread-preview`.

use crate::message::future_date_cutoff_timestamp;
use crate::{decode_id, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, ThreadId};
use sqlx::Row;

/// What one thread looks like at a glance. Sender, date, snippet and
/// attachment flag come from the latest message; counts cover the thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPreviewRecord {
    pub thread_id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub latest_at: DateTime<Utc>,
    pub message_count: u32,
    pub unread_count: u32,
    pub snippet: String,
    /// Any message in the thread carries an attachment.
    pub has_attachments: bool,
}

impl super::Store {
    /// Previews for the named threads in one read, no bodies. Ids with no
    /// messages are left out; order is not kept.
    pub async fn thread_previews(
        &self,
        thread_ids: &[ThreadId],
    ) -> Result<Vec<ThreadPreviewRecord>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            "WITH ranked AS (
                SELECT thread_id, account_id, subject, from_name, from_email, snippet,
                       CASE WHEN date > ?1 THEN 0 ELSE date END AS dated,
                       ROW_NUMBER() OVER (
                           PARTITION BY thread_id
                           ORDER BY CASE WHEN date > ?1 THEN 0 ELSE date END DESC, id DESC
                       ) AS recency,
                       COUNT(*) OVER (PARTITION BY thread_id) AS message_count,
                       SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END)
                           OVER (PARTITION BY thread_id) AS unread_count,
                       MAX(has_attachments) OVER (PARTITION BY thread_id) AS any_attachment,
                       MIN(subject) OVER (PARTITION BY thread_id) AS thread_subject
                FROM messages
                WHERE thread_id IN (SELECT value FROM json_each(?2))
             )
             SELECT thread_id, account_id, thread_subject, from_name, from_email, snippet,
                    dated, message_count, unread_count, any_attachment
             FROM ranked WHERE recency = 1",
        )
        .bind(future_date_cutoff_timestamp())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        trace_query("thread_preview.batch", started_at, rows.len());
        rows.iter()
            .map(|row| {
                Ok(ThreadPreviewRecord {
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
                    subject: row.try_get("thread_subject")?,
                    sender_name: row.try_get("from_name")?,
                    sender_email: row.try_get("from_email")?,
                    latest_at: decode_timestamp(row.try_get("dated")?)?,
                    message_count: row.try_get::<i64, _>("message_count")? as u32,
                    unread_count: row.try_get::<i64, _>("unread_count")? as u32,
                    snippet: row.try_get("snippet")?,
                    has_attachments: row.try_get::<i64, _>("any_attachment")? != 0,
                })
            })
            .collect()
    }
}
