//! Persistence for per-message custom keywords.
//!
//! Mirrors the `message_labels` junction table but for IMAP-style
//! `$Foo` keywords rather than provider labels. The Envelope's
//! `keywords` set is hydrated from this table on read and replaced
//! wholesale on sync upsert.

use mxr_core::{Envelope, MessageId};
use sqlx::Row;
use std::collections::{BTreeSet, HashMap};

impl super::Store {
    /// Replace the full keyword set for a message in one transaction
    /// (delete-then-insert). Called during sync ingest with the
    /// authoritative set from the provider.
    pub async fn set_message_keywords(
        &self,
        message_id: &MessageId,
        keywords: &BTreeSet<String>,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str().clone();
        let mut tx = self.writer().begin().await?;
        sqlx::query("DELETE FROM message_keywords WHERE message_id = ?")
            .bind(&mid)
            .execute(&mut *tx)
            .await?;
        for keyword in keywords {
            sqlx::query(
                "INSERT OR IGNORE INTO message_keywords (message_id, keyword) VALUES (?, ?)",
            )
            .bind(&mid)
            .bind(keyword)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Read the keyword set for a single message. Empty set if no rows.
    pub async fn get_message_keywords(
        &self,
        message_id: &MessageId,
    ) -> Result<BTreeSet<String>, sqlx::Error> {
        let mid = message_id.as_str();
        let rows = sqlx::query("SELECT keyword FROM message_keywords WHERE message_id = ?")
            .bind(mid)
            .fetch_all(self.reader())
            .await?;
        let mut set = BTreeSet::new();
        for row in rows {
            set.insert(row.try_get::<String, _>("keyword")?);
        }
        Ok(set)
    }

    /// Stitch keyword sets onto a slice of envelopes in one batch query.
    /// Empty input is a no-op. Called by every public list/get path that
    /// returns envelopes to clients so the IPC layer sees full state.
    pub async fn hydrate_envelope_keywords(
        &self,
        envelopes: &mut [Envelope],
    ) -> Result<(), sqlx::Error> {
        if envelopes.is_empty() {
            return Ok(());
        }
        let ids: Vec<MessageId> = envelopes.iter().map(|e| e.id.clone()).collect();
        let map = self.get_message_keywords_batch(&ids).await?;
        for env in envelopes.iter_mut() {
            if let Some(set) = map.get(&env.id) {
                env.keywords = set.clone();
            }
        }
        Ok(())
    }

    /// Batch read: returns the keyword set for each requested message id.
    /// Ids with no rows are absent from the map (caller treats absence as
    /// the empty set).
    pub async fn get_message_keywords_batch(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, BTreeSet<String>>, sqlx::Error> {
        let mut out: HashMap<MessageId, BTreeSet<String>> = HashMap::new();
        if message_ids.is_empty() {
            return Ok(out);
        }
        let placeholders = std::iter::repeat_n("?", message_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT message_id, keyword FROM message_keywords WHERE message_id IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for id in message_ids {
            query = query.bind(id.as_str());
        }
        let rows = query.fetch_all(self.reader()).await?;
        let by_id: HashMap<String, MessageId> = message_ids
            .iter()
            .map(|id| (id.as_str().clone(), id.clone()))
            .collect();
        for row in rows {
            let mid_str: String = row.try_get("message_id")?;
            let keyword: String = row.try_get("keyword")?;
            if let Some(mid) = by_id.get(&mid_str) {
                out.entry(mid.clone()).or_default().insert(keyword);
            }
        }
        Ok(out)
    }
}
