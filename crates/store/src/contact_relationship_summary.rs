use crate::{decode_id, decode_json, decode_timestamp, encode_json};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactRelationshipSummaryRecord {
    pub account_id: AccountId,
    pub email: String,
    pub text: String,
    pub model: String,
    pub known_topics: Vec<String>,
    pub computed_at: DateTime<Utc>,
    pub source_hash: String,
    pub last_error: Option<String>,
}

impl super::Store {
    pub async fn upsert_contact_relationship_summary(
        &self,
        record: &ContactRelationshipSummaryRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO contact_relationship_summary
               (account_id, email, text, model, known_topics_json, computed_at, source_hash, last_error)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id, email) DO UPDATE SET
                 text = excluded.text,
                 model = excluded.model,
                 known_topics_json = excluded.known_topics_json,
                 computed_at = excluded.computed_at,
                 source_hash = excluded.source_hash,
                 last_error = excluded.last_error"#,
        )
        .bind(record.account_id.as_str())
        .bind(&record.email)
        .bind(&record.text)
        .bind(&record.model)
        .bind(encode_json(&record.known_topics)?)
        .bind(record.computed_at.timestamp())
        .bind(&record.source_hash)
        .bind(&record.last_error)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Writes a relationship summary built from `sources`, unless any of
    /// them was deleted while the model ran. Returns whether it was written.
    pub async fn upsert_contact_relationship_summary_if_sources_exist(
        &self,
        record: &ContactRelationshipSummaryRecord,
        sources: &[mxr_core::MessageId],
    ) -> Result<bool, sqlx::Error> {
        let sql = format!(
            r#"INSERT INTO contact_relationship_summary
               (account_id, email, text, model, known_topics_json, computed_at, source_hash, last_error)
               SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
               WHERE {sources_exist}
               ON CONFLICT(account_id, email) DO UPDATE SET
                 text = excluded.text,
                 model = excluded.model,
                 known_topics_json = excluded.known_topics_json,
                 computed_at = excluded.computed_at,
                 source_hash = excluded.source_hash,
                 last_error = excluded.last_error"#,
            sources_exist = crate::sources_exist_sql(9)
        );
        let written = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(record.account_id.as_str())
            .bind(&record.email)
            .bind(&record.text)
            .bind(&record.model)
            .bind(encode_json(&record.known_topics)?)
            .bind(record.computed_at.timestamp())
            .bind(&record.source_hash)
            .bind(&record.last_error)
            .bind(crate::sources_json(sources))
            .execute(self.writer())
            .await?
            .rows_affected();
        Ok(written > 0)
    }

    pub async fn get_contact_relationship_summary(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Option<ContactRelationshipSummaryRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT account_id, email, text, model, known_topics_json, computed_at, source_hash, last_error
               FROM contact_relationship_summary
               WHERE account_id = ? AND email = ? COLLATE NOCASE"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            Ok(ContactRelationshipSummaryRecord {
                account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
                email: row.get("email"),
                text: row.get("text"),
                model: row.get("model"),
                known_topics: decode_json(&row.get::<String, _>("known_topics_json"))?,
                computed_at: decode_timestamp(row.get("computed_at"))?,
                source_hash: row.get("source_hash"),
                last_error: row.get("last_error"),
            })
        })
        .transpose()
    }
}
