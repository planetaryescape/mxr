use crate::{decode_id, decode_timestamp, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashSet;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSummaryRecord {
    pub thread_id: ThreadId,
    pub account_id: AccountId,
    pub content_hash: String,
    pub text: String,
    pub model: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl super::Store {
    pub async fn get_thread_summary(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Option<ThreadSummaryRecord>, sqlx::Error> {
        let started_at = Instant::now();
        let row = sqlx::query(
            r#"SELECT thread_id, account_id, content_hash, text, model, generated_at
               FROM thread_summaries
               WHERE thread_id = ?"#,
        )
        .bind(thread_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("thread_summary.get", started_at, row.is_some());
        row.map(row_to_thread_summary).transpose()
    }

    pub async fn upsert_thread_summary(
        &self,
        record: &ThreadSummaryRecord,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"INSERT INTO thread_summaries
               (thread_id, account_id, content_hash, text, model, generated_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(thread_id) DO UPDATE SET
                   account_id = excluded.account_id,
                   content_hash = excluded.content_hash,
                   text = excluded.text,
                   model = excluded.model,
                   generated_at = excluded.generated_at,
                   updated_at = excluded.updated_at"#,
        )
        .bind(record.thread_id.as_str())
        .bind(record.account_id.as_str())
        .bind(&record.content_hash)
        .bind(&record.text)
        .bind(&record.model)
        .bind(record.generated_at.timestamp())
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn thread_ids_for_message_ids(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat_n("?", message_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT DISTINCT thread_id FROM messages WHERE id IN ({placeholders})");
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
        for message_id in message_ids {
            query = query.bind(message_id.as_str());
        }
        let started_at = Instant::now();
        let rows = query.fetch_all(self.reader()).await?;
        trace_query(
            "thread_summary.thread_ids_for_messages",
            started_at,
            rows.len(),
        );

        let mut ids = Vec::new();
        let mut seen = HashSet::new();
        for row in rows {
            let raw: String = row.get("thread_id");
            if seen.insert(raw.clone()) {
                ids.push(decode_id::<ThreadId>(&raw)?);
            }
        }
        Ok(ids)
    }
}

pub fn thread_summary_content_hash(envelopes: &[Envelope]) -> String {
    let mut hasher = Sha256::new();
    for envelope in envelopes {
        hash_str(&mut hasher, &envelope.id.as_str());
        hash_str(&mut hasher, &envelope.provider_id);
        hash_str(
            &mut hasher,
            envelope.message_id_header.as_deref().unwrap_or_default(),
        );
        hash_str(&mut hasher, &envelope.subject);
        hash_str(&mut hasher, &envelope.date.timestamp().to_string());
        hash_address(&mut hasher, &envelope.from);
        hash_addresses(&mut hasher, &envelope.to);
        hash_addresses(&mut hasher, &envelope.cc);
        hash_addresses(&mut hasher, &envelope.bcc);
        hash_str(&mut hasher, &envelope.size_bytes.to_string());
    }
    base16ct::lower::encode_string(&hasher.finalize())
}

fn hash_addresses(hasher: &mut Sha256, addresses: &[Address]) {
    hash_str(hasher, &addresses.len().to_string());
    for address in addresses {
        hash_address(hasher, address);
    }
}

fn hash_address(hasher: &mut Sha256, address: &Address) {
    hash_str(hasher, address.name.as_deref().unwrap_or_default());
    hash_str(hasher, &address.email);
}

fn hash_str(hasher: &mut Sha256, value: &str) {
    hasher.update(value.len().to_string().as_bytes());
    hasher.update(b":");
    hasher.update(value.as_bytes());
    hasher.update(b"\n");
}

fn row_to_thread_summary(row: sqlx::sqlite::SqliteRow) -> Result<ThreadSummaryRecord, sqlx::Error> {
    Ok(ThreadSummaryRecord {
        thread_id: decode_id(&row.get::<String, _>("thread_id"))?,
        account_id: decode_id(&row.get::<String, _>("account_id"))?,
        content_hash: row.get("content_hash"),
        text: row.get("text"),
        model: row.get("model"),
        generated_at: decode_timestamp(row.get("generated_at"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};

    #[tokio::test]
    async fn thread_summary_roundtrips_and_updates() {
        let store = crate::Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let thread_id = ThreadId::new();

        let record = ThreadSummaryRecord {
            thread_id: thread_id.clone(),
            account_id: account.id.clone(),
            content_hash: "v1".into(),
            text: "old".into(),
            model: "model-a".into(),
            generated_at: chrono::Utc::now(),
        };
        store.upsert_thread_summary(&record).await.unwrap();

        let stored = store
            .get_thread_summary(&thread_id)
            .await
            .unwrap()
            .expect("summary");
        assert_eq!(stored.content_hash, "v1");
        assert_eq!(stored.text, "old");

        let updated = ThreadSummaryRecord {
            content_hash: "v2".into(),
            text: "new".into(),
            model: "model-b".into(),
            ..record
        };
        store.upsert_thread_summary(&updated).await.unwrap();

        let stored = store
            .get_thread_summary(&thread_id)
            .await
            .unwrap()
            .expect("summary");
        assert_eq!(stored.content_hash, "v2");
        assert_eq!(stored.text, "new");
        assert_eq!(stored.model, "model-b");
    }

    #[tokio::test]
    async fn thread_ids_for_message_ids_deduplicates_threads() {
        let store = crate::Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let thread_id = ThreadId::new();
        let mut first = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        first.thread_id = thread_id.clone();
        first.provider_id = "first".into();
        let mut second = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        second.thread_id = thread_id.clone();
        second.provider_id = "second".into();
        store.upsert_envelope(&first).await.unwrap();
        store.upsert_envelope(&second).await.unwrap();

        let ids = store
            .thread_ids_for_message_ids(&[first.id.clone(), second.id.clone()])
            .await
            .unwrap();

        assert_eq!(ids, vec![thread_id]);
    }
}
