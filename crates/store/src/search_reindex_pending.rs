//! Messages whose lexical search entry may be stale, waiting for a reindex.
//!
//! Unchecked queries, like the other internal maintenance queries, so the
//! table needs no `.sqlx` cache entry.

use crate::decode_id;
use mxr_core::id::MessageId;
use sqlx::Row;

impl super::Store {
    /// Marks `message_ids` for the next search reindex. Marking one twice
    /// keeps the first mark.
    pub async fn mark_search_reindex_pending(
        &self,
        message_ids: &[MessageId],
    ) -> Result<(), sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now().timestamp();
        let mut tx = self.writer().begin().await?;
        for message_id in message_ids {
            sqlx::query(
                "INSERT OR IGNORE INTO search_reindex_pending (message_id, marked_at) VALUES (?, ?)",
            )
            .bind(message_id.as_str())
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Up to `limit` marked messages, oldest mark first.
    pub async fn list_search_reindex_pending(
        &self,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        sqlx::query(
            "SELECT message_id FROM search_reindex_pending ORDER BY marked_at ASC, message_id ASC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?
        .into_iter()
        .map(|row| decode_id(row.get::<String, _>("message_id").as_str()))
        .collect()
    }

    pub async fn clear_search_reindex_pending(
        &self,
        message_ids: &[MessageId],
    ) -> Result<(), sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.writer().begin().await?;
        for message_id in message_ids {
            sqlx::query("DELETE FROM search_reindex_pending WHERE message_id = ?")
                .bind(message_id.as_str())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await
    }
}
