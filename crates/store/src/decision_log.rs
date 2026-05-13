//! Slice 3.2 of docs/ai-email/03-archive-intelligence.md.
//!
//! Decision log: stable, citation-backed records of "we agreed on X"
//! moments extracted from threads. The unique key is
//! (account_id, thread_id, source_hash); re-extraction with the
//! same prompt content is idempotent. When the source content
//! changes the source_hash changes too, producing an updated row
//! through the upsert.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionLogEntry {
    pub id: String,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub topic: Option<String>,
    pub decision: String,
    pub rationale: Option<String>,
    pub evidence_msg_ids: Vec<MessageId>,
    pub decided_at: Option<DateTime<Utc>>,
    pub extracted_at: DateTime<Utc>,
    pub source_hash: String,
}

impl super::Store {
    /// Idempotent upsert keyed on (account, thread, source_hash).
    pub async fn upsert_decision(
        &self,
        entry: &DecisionLogEntry,
    ) -> Result<(), sqlx::Error> {
        let evidence_json = serde_json::to_string(
            &entry
                .evidence_msg_ids
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        sqlx::query(
            r#"INSERT INTO decision_log
               (id, account_id, thread_id, topic, decision, rationale,
                evidence_msg_ids, decided_at, extracted_at, source_hash)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id, thread_id, source_hash) DO UPDATE SET
                 topic = excluded.topic,
                 decision = excluded.decision,
                 rationale = excluded.rationale,
                 evidence_msg_ids = excluded.evidence_msg_ids,
                 decided_at = excluded.decided_at,
                 extracted_at = excluded.extracted_at"#,
        )
        .bind(&entry.id)
        .bind(entry.account_id.as_str())
        .bind(entry.thread_id.as_str())
        .bind(entry.topic.as_ref())
        .bind(&entry.decision)
        .bind(entry.rationale.as_ref())
        .bind(&evidence_json)
        .bind(entry.decided_at.map(|v| v.timestamp()))
        .bind(entry.extracted_at.timestamp())
        .bind(&entry.source_hash)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_decisions(
        &self,
        account_id: &AccountId,
        topic: Option<&str>,
        since_days: Option<u32>,
        limit: u32,
    ) -> Result<Vec<DecisionLogEntry>, sqlx::Error> {
        let cutoff = since_days
            .map(|d| (Utc::now() - chrono::Duration::days(d as i64)).timestamp());
        let rows = sqlx::query(
            r#"SELECT id, account_id, thread_id, topic, decision, rationale,
                      evidence_msg_ids, decided_at, extracted_at, source_hash
               FROM decision_log
               WHERE account_id = ?
                 AND (?2 IS NULL OR LOWER(topic) = LOWER(?2))
                 AND (?3 IS NULL OR COALESCE(decided_at, extracted_at) >= ?3)
               ORDER BY COALESCE(decided_at, extracted_at) DESC, id ASC
               LIMIT ?4"#,
        )
        .bind(account_id.as_str())
        .bind(topic)
        .bind(cutoff)
        .bind(limit as i64)
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                let evidence_json: String = row.try_get("evidence_msg_ids")?;
                let evidence_strs: Vec<String> =
                    serde_json::from_str(&evidence_json).unwrap_or_default();
                let evidence_msg_ids: Result<Vec<MessageId>, sqlx::Error> = evidence_strs
                    .into_iter()
                    .map(|s| decode_id(&s))
                    .collect();
                Ok(DecisionLogEntry {
                    id: row.try_get("id")?,
                    account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    topic: row.try_get("topic")?,
                    decision: row.try_get("decision")?,
                    rationale: row.try_get("rationale")?,
                    evidence_msg_ids: evidence_msg_ids?,
                    decided_at: decode_optional_timestamp(row.try_get("decided_at")?)?,
                    extracted_at: decode_timestamp(row.try_get("extracted_at")?)?,
                    source_hash: row.try_get("source_hash")?,
                })
            })
            .collect()
    }
}

/// Compute a stable id for a (account, thread, normalized decision,
/// evidence ids) tuple. Used as the primary key.
pub fn decision_id(
    account_id: &AccountId,
    thread_id: &ThreadId,
    normalized_decision: &str,
    evidence_msg_ids: &[MessageId],
) -> String {
    let mut h = Sha256::new();
    h.update(account_id.as_str().as_bytes());
    h.update(b"|");
    h.update(thread_id.as_str().as_bytes());
    h.update(b"|");
    h.update(normalized_decision.trim().to_lowercase().as_bytes());
    h.update(b"|");
    let mut sorted: Vec<String> = evidence_msg_ids.iter().map(|m| m.to_string()).collect();
    sorted.sort();
    h.update(sorted.join(",").as_bytes());
    format!("{:x}", h.finalize())
}

/// Compute a hash of the input prompt context. When the underlying
/// thread mutates the source_hash changes and the upsert path
/// produces an updated row.
pub fn source_hash(thread_text: &str, evidence_msg_ids: &[MessageId]) -> String {
    let mut h = Sha256::new();
    h.update(thread_text.trim().as_bytes());
    h.update(b"|");
    let mut sorted: Vec<String> = evidence_msg_ids.iter().map(|m| m.to_string()).collect();
    sorted.sort();
    h.update(sorted.join(",").as_bytes());
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    async fn fixture() -> (Store, AccountId, ThreadId, MessageId) {
        let store = Store::in_memory().await.unwrap();
        let account = mxr_core::Account {
            id: AccountId::new(),
            name: "T".into(),
            email: "me@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.unwrap();
        (store, account.id, ThreadId::new(), MessageId::new())
    }

    fn entry(
        account_id: &AccountId,
        thread_id: &ThreadId,
        msg_id: &MessageId,
        decision: &str,
    ) -> DecisionLogEntry {
        let evidence = vec![msg_id.clone()];
        let id = decision_id(account_id, thread_id, decision, &evidence);
        let hash = source_hash(decision, &evidence);
        DecisionLogEntry {
            id,
            account_id: account_id.clone(),
            thread_id: thread_id.clone(),
            topic: Some("pricing".into()),
            decision: decision.into(),
            rationale: None,
            evidence_msg_ids: evidence,
            decided_at: Some(Utc::now()),
            extracted_at: Utc::now(),
            source_hash: hash,
        }
    }

    #[tokio::test]
    async fn upsert_then_list_returns_inserted_decision() {
        let (store, account, thread, msg) = fixture().await;
        let e = entry(&account, &thread, &msg, "Use Postgres");
        store.upsert_decision(&e).await.unwrap();
        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].decision, "Use Postgres");
        assert_eq!(rows[0].evidence_msg_ids, vec![msg]);
    }

    #[tokio::test]
    async fn upsert_with_unchanged_source_hash_is_idempotent() {
        let (store, account, thread, msg) = fixture().await;
        let e = entry(&account, &thread, &msg, "Use Postgres");
        store.upsert_decision(&e).await.unwrap();
        let mut e2 = e.clone();
        // Same source_hash -- must NOT create a new row even if id
        // differs (which it doesn't here, but in real flows might).
        e2.id = "different-id".into();
        store.upsert_decision(&e2).await.unwrap();
        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn upsert_with_changed_source_hash_creates_new_row() {
        let (store, account, thread, msg) = fixture().await;
        let e1 = entry(&account, &thread, &msg, "Use Postgres");
        store.upsert_decision(&e1).await.unwrap();
        let e2 = entry(&account, &thread, &msg, "Use SQLite");
        store.upsert_decision(&e2).await.unwrap();
        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "different source_hash must yield distinct rows");
    }

    #[tokio::test]
    async fn list_filters_by_topic_case_insensitive() {
        let (store, account, thread, msg) = fixture().await;
        let mut e = entry(&account, &thread, &msg, "Use Postgres");
        e.topic = Some("Pricing".into());
        store.upsert_decision(&e).await.unwrap();
        let mut e2 = entry(&account, &thread, &msg, "Use pgvector");
        e2.topic = Some("Search".into());
        store.upsert_decision(&e2).await.unwrap();

        let rows = store
            .list_decisions(&account, Some("pricing"), None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].topic.as_deref(), Some("Pricing"));
    }
}
