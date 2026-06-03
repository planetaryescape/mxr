use crate::{decode_id, decode_timestamp, trace_lookup};
use mxr_core::id::*;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageCacheRecord {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub prompt_version: String,
    pub content_hash: String,
    pub verdict: String,
    pub verdict_line: String,
    pub reason: String,
    pub model: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

impl super::Store {
    pub async fn get_triage_cache(
        &self,
        message_id: &MessageId,
        prompt_version: &str,
    ) -> Result<Option<TriageCacheRecord>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT message_id, account_id, thread_id, prompt_version, content_hash,
                      verdict, verdict_line, reason, model, generated_at
               FROM triage_cache
               WHERE message_id = ? AND prompt_version = ?"#,
        )
        .bind(message_id.as_str())
        .bind(prompt_version)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("triage_cache.get", started_at, row.is_some());
        row.map(row_to_triage_cache).transpose()
    }

    pub async fn upsert_triage_cache(&self, record: &TriageCacheRecord) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            r#"INSERT INTO triage_cache
               (message_id, account_id, thread_id, prompt_version, content_hash, verdict,
                verdict_line, reason, model, generated_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(message_id, prompt_version) DO UPDATE SET
                   account_id = excluded.account_id,
                   thread_id = excluded.thread_id,
                   content_hash = excluded.content_hash,
                   verdict = excluded.verdict,
                   verdict_line = excluded.verdict_line,
                   reason = excluded.reason,
                   model = excluded.model,
                   generated_at = excluded.generated_at,
                   updated_at = excluded.updated_at"#,
        )
        .bind(record.message_id.as_str())
        .bind(record.account_id.as_str())
        .bind(record.thread_id.as_str())
        .bind(&record.prompt_version)
        .bind(&record.content_hash)
        .bind(&record.verdict)
        .bind(&record.verdict_line)
        .bind(&record.reason)
        .bind(&record.model)
        .bind(record.generated_at.timestamp())
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(())
    }
}

fn row_to_triage_cache(row: sqlx::sqlite::SqliteRow) -> Result<TriageCacheRecord, sqlx::Error> {
    Ok(TriageCacheRecord {
        message_id: decode_id(&row.get::<String, _>("message_id"))?,
        account_id: decode_id(&row.get::<String, _>("account_id"))?,
        thread_id: decode_id(&row.get::<String, _>("thread_id"))?,
        prompt_version: row.get("prompt_version"),
        content_hash: row.get("content_hash"),
        verdict: row.get("verdict"),
        verdict_line: row.get("verdict_line"),
        reason: row.get("reason"),
        model: row.get("model"),
        generated_at: decode_timestamp(row.get("generated_at"))?,
    })
}
