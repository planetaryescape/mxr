use crate::{decode_id, decode_json, decode_timestamp, encode_json};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserVoiceRegisterMode {
    pub name: String,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub exemplar_message_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserVoiceProfileRecord {
    pub account_id: AccountId,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub msg_count_used: u32,
    pub metrics_json: String,
    pub register_modes: Vec<UserVoiceRegisterMode>,
    pub computed_at: DateTime<Utc>,
    pub source_hash: String,
}

impl super::Store {
    pub async fn upsert_user_voice_profile(
        &self,
        record: &UserVoiceProfileRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO user_voice_profile
               (account_id, formality_score, avg_sentence_len, msg_count_used, metrics_json,
                register_modes_json, computed_at, source_hash)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id) DO UPDATE SET
                 formality_score = excluded.formality_score,
                 avg_sentence_len = excluded.avg_sentence_len,
                 msg_count_used = excluded.msg_count_used,
                 metrics_json = excluded.metrics_json,
                 register_modes_json = excluded.register_modes_json,
                 computed_at = excluded.computed_at,
                 source_hash = excluded.source_hash"#,
        )
        .bind(record.account_id.as_str())
        .bind(record.formality_score)
        .bind(record.avg_sentence_len)
        .bind(record.msg_count_used as i64)
        .bind(&record.metrics_json)
        .bind(encode_json(&record.register_modes)?)
        .bind(record.computed_at.timestamp())
        .bind(&record.source_hash)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn get_user_voice_profile(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<UserVoiceProfileRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT account_id, formality_score, avg_sentence_len, msg_count_used,
                      metrics_json, register_modes_json, computed_at, source_hash
               FROM user_voice_profile WHERE account_id = ?"#,
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            Ok(UserVoiceProfileRecord {
                account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
                formality_score: row.get("formality_score"),
                avg_sentence_len: row.get("avg_sentence_len"),
                msg_count_used: row.get::<i64, _>("msg_count_used") as u32,
                metrics_json: row.get("metrics_json"),
                register_modes: decode_json(&row.get::<String, _>("register_modes_json"))?,
                computed_at: decode_timestamp(row.get("computed_at"))?,
                source_hash: row.get("source_hash"),
            })
        })
        .transpose()
    }
}
