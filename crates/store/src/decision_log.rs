//! Slice 3.2 of docs/reference/ai-email.md
//!
//! Decision log: stable, citation-backed records of "we agreed on X"
//! moments extracted from threads. The unique key is a stable decision
//! id derived from account, thread, normalized decision text, and
//! evidence ids. Re-extraction with changed source content refreshes
//! the same decision row by updating `source_hash`.

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
    /// Idempotent upsert keyed on stable decision id.
    ///
    /// Also records which stored messages the decision cites, so a message
    /// delete can find it through an index instead of parsing every
    /// decision's evidence JSON while holding the writer.
    pub async fn upsert_decision(&self, entry: &DecisionLogEntry) -> Result<(), sqlx::Error> {
        self.write_decision(entry, false).await.map(|_| ())
    }

    /// Writes a decision a model just extracted, unless a message it cites
    /// was deleted while the model ran. Returns whether it was written.
    ///
    /// Without the check the decision would be stored with no evidence row
    /// for the deleted message, where no later delete could find it, and its
    /// text can quote that message. The check and the write share one
    /// transaction on the single writer connection, so a delete lands either
    /// before (the write is refused) or after (it finds the evidence row).
    pub async fn upsert_extracted_decision(
        &self,
        entry: &DecisionLogEntry,
    ) -> Result<bool, sqlx::Error> {
        self.write_decision(entry, true).await
    }

    /// Deletes decisions that cite messages none of which is stored any
    /// more: left over from deletes made before decision evidence was
    /// tracked, which nothing else would ever find. Decisions that cite
    /// nothing are kept. Idempotent, and one statement driven by the
    /// evidence table's primary key, so it is cheap on a large store.
    pub async fn prune_decisions_without_evidence(&self) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query(
            "DELETE FROM decision_log
             WHERE json_array_length(evidence_msg_ids) > 0
               AND NOT EXISTS (
                   SELECT 1 FROM decision_evidence
                   WHERE decision_evidence.decision_id = decision_log.id)",
        )
        .execute(self.writer())
        .await?
        .rows_affected())
    }

    async fn write_decision(
        &self,
        entry: &DecisionLogEntry,
        require_evidence: bool,
    ) -> Result<bool, sqlx::Error> {
        let evidence_json = serde_json::to_string(
            &entry
                .evidence_msg_ids
                .iter()
                .map(mxr_core::MessageId::as_str)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        let mut tx = self.writer().begin().await?;
        if require_evidence {
            let evidence_missing: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM json_each(?)
                                WHERE value NOT IN (SELECT id FROM messages))",
            )
            .bind(&evidence_json)
            .fetch_one(&mut *tx)
            .await?;
            if evidence_missing {
                return Ok(false);
            }
        }
        sqlx::query(
            r#"INSERT INTO decision_log
               (id, account_id, thread_id, topic, decision, rationale,
                evidence_msg_ids, decided_at, extracted_at, source_hash)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                 account_id = excluded.account_id,
                 thread_id = excluded.thread_id,
                 topic = excluded.topic,
                 decision = excluded.decision,
                 rationale = excluded.rationale,
                 evidence_msg_ids = excluded.evidence_msg_ids,
                 decided_at = excluded.decided_at,
                 extracted_at = excluded.extracted_at,
                 source_hash = excluded.source_hash"#,
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
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM decision_evidence WHERE decision_id = ?")
            .bind(&entry.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO decision_evidence (decision_id, message_id)
             SELECT ?1, value FROM json_each(?2) WHERE value IN (SELECT id FROM messages)",
        )
        .bind(&entry.id)
        .bind(&evidence_json)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Fetch a single decision by primary-key id. Returns `Ok(None)`
    /// when the id is unknown; reserved for actual storage errors.
    pub async fn get_decision(&self, id: &str) -> Result<Option<DecisionLogEntry>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, account_id, thread_id, topic, decision, rationale,
                      evidence_msg_ids, decided_at, extracted_at, source_hash
               FROM decision_log
               WHERE id = ?"#,
        )
        .bind(id)
        .fetch_optional(self.reader())
        .await?;
        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };
        let evidence_json: String = row.try_get("evidence_msg_ids")?;
        let evidence_strs: Vec<String> = serde_json::from_str(&evidence_json).unwrap_or_default();
        let evidence_msg_ids: Result<Vec<MessageId>, sqlx::Error> =
            evidence_strs.into_iter().map(|s| decode_id(&s)).collect();
        Ok(Some(DecisionLogEntry {
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
        }))
    }

    pub async fn list_decisions(
        &self,
        account_id: &AccountId,
        topic: Option<&str>,
        since_days: Option<u32>,
        limit: u32,
    ) -> Result<Vec<DecisionLogEntry>, sqlx::Error> {
        let cutoff =
            since_days.map(|d| (Utc::now() - chrono::Duration::days(d as i64)).timestamp());
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
                let evidence_msg_ids: Result<Vec<MessageId>, sqlx::Error> =
                    evidence_strs.into_iter().map(|s| decode_id(&s)).collect();
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
    let mut sorted: Vec<String> = evidence_msg_ids
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    sorted.sort();
    h.update(sorted.join(",").as_bytes());
    base16ct::lower::encode_string(&h.finalize())
}

/// Compute a hash of the input prompt context. When the underlying
/// thread mutates the source_hash changes and the upsert path
/// produces an updated row.
pub fn source_hash(thread_text: &str, evidence_msg_ids: &[MessageId]) -> String {
    let mut h = Sha256::new();
    h.update(thread_text.trim().as_bytes());
    h.update(b"|");
    let mut sorted: Vec<String> = evidence_msg_ids
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    sorted.sort();
    h.update(sorted.join(",").as_bytes());
    base16ct::lower::encode_string(&h.finalize())
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

    /// A model extraction that outlived the delete of a message it cites
    /// must not store the decision: no evidence row would point the next
    /// delete at it.
    #[tokio::test]
    async fn an_extracted_decision_citing_deleted_mail_is_not_written() {
        let (store, account, thread, _) = fixture().await;
        let mut stored = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        stored.thread_id = thread.clone();
        store.upsert_envelope(&stored).await.unwrap();
        let deleted_while_the_model_ran = MessageId::new();

        let mut citing_both = entry(&account, &thread, &stored.id, "Use Postgres");
        citing_both
            .evidence_msg_ids
            .push(deleted_while_the_model_ran.clone());
        assert!(!store.upsert_extracted_decision(&citing_both).await.unwrap());
        assert!(store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap()
            .is_empty());

        let citing_live = entry(&account, &thread, &stored.id, "Use Postgres");
        assert!(store.upsert_extracted_decision(&citing_live).await.unwrap());
        assert_eq!(
            store
                .list_decisions(&account, None, None, 10)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    /// A decision whose cited mail was deleted before evidence was tracked
    /// has no evidence rows, so no delete will ever reach it.
    #[tokio::test]
    async fn decisions_left_over_from_earlier_deletes_are_pruned() {
        let (store, account, thread, deleted_long_ago) = fixture().await;
        let mut stored = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        stored.thread_id = thread.clone();
        store.upsert_envelope(&stored).await.unwrap();
        store
            .upsert_decision(&entry(&account, &thread, &deleted_long_ago, "Orphaned"))
            .await
            .unwrap();
        store
            .upsert_decision(&entry(&account, &thread, &stored.id, "Live"))
            .await
            .unwrap();
        let mut no_evidence = entry(&account, &thread, &stored.id, "Cites nothing");
        no_evidence.evidence_msg_ids.clear();
        store.upsert_decision(&no_evidence).await.unwrap();

        assert_eq!(store.prune_decisions_without_evidence().await.unwrap(), 1);
        assert_eq!(store.prune_decisions_without_evidence().await.unwrap(), 0);
        let mut left = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|decision| decision.decision)
            .collect::<Vec<_>>();
        left.sort();
        assert_eq!(left, vec!["Cites nothing", "Live"]);
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
        store.upsert_decision(&e).await.unwrap();
        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn changed_source_hash_updates_same_decision_row() {
        let (store, account, thread, msg) = fixture().await;
        let mut e1 = entry(&account, &thread, &msg, "Use Postgres");
        e1.source_hash = "hash-v1".into();
        store.upsert_decision(&e1).await.unwrap();
        let mut e2 = e1.clone();
        e2.source_hash = "hash-v2".into();
        e2.rationale = Some("thread changed".into());
        store.upsert_decision(&e2).await.unwrap();
        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "stable decision id must refresh in place");
        assert_eq!(rows[0].source_hash, "hash-v2");
        assert_eq!(rows[0].rationale.as_deref(), Some("thread changed"));
    }

    #[tokio::test]
    async fn multiple_decisions_from_same_thread_and_source_are_preserved() {
        let (store, account, thread, msg) = fixture().await;
        let msg2 = MessageId::new();
        let mut first = entry(&account, &thread, &msg, "Use Postgres");
        let mut second = entry(&account, &thread, &msg2, "Launch in June");
        first.source_hash = "same-thread-source".into();
        second.source_hash = "same-thread-source".into();

        store.upsert_decision(&first).await.unwrap();
        store.upsert_decision(&second).await.unwrap();

        let rows = store
            .list_decisions(&account, None, None, 10)
            .await
            .unwrap();
        let decisions: std::collections::HashSet<_> =
            rows.iter().map(|row| row.decision.as_str()).collect();
        assert_eq!(rows.len(), 2, "thread can contain more than one decision");
        assert!(decisions.contains("Use Postgres"));
        assert!(decisions.contains("Launch in June"));
    }

    /// `get_decision` is the primary-key lookup that backs
    /// `mxr decisions show <id>`. Must return the full row when the
    /// id exists and `None` otherwise — callers distinguish "no such
    /// decision" from "lookup failed" by the Result variant.
    #[tokio::test]
    async fn get_decision_returns_row_or_none() {
        let (store, account, thread, msg) = fixture().await;
        let e = entry(&account, &thread, &msg, "Use Postgres");
        let id = e.id.clone();
        store.upsert_decision(&e).await.unwrap();

        let found = store.get_decision(&id).await.unwrap();
        let found = found.expect("present id returns Some");
        assert_eq!(found.id, id);
        assert_eq!(found.decision, "Use Postgres");
        assert_eq!(found.evidence_msg_ids, vec![msg]);
        assert_eq!(found.topic.as_deref(), Some("pricing"));

        let missing = store.get_decision("does-not-exist").await.unwrap();
        assert!(missing.is_none(), "absent id returns None, not error");
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
