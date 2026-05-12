use crate::{decode_id, decode_timestamp, trace_lookup, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageDirection};
use sqlx::Row;
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct ContactStyleRecord {
    pub account_id: AccountId,
    pub email: String,
    pub formality_score: f64,
    pub formality_score_theirs: f64,
    pub avg_sentence_len: f64,
    pub avg_sentence_len_theirs: f64,
    pub msg_count_used: u32,
    pub msg_count_used_theirs: u32,
    pub metrics_json: String,
    pub metrics_json_theirs: String,
    pub computed_at: DateTime<Utc>,
    pub source_hash: String,
    pub drift_detected: bool,
    pub drift_reason: Option<String>,
    pub drift_detected_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipMessageSample {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub direction: MessageDirection,
    pub from_email: String,
    pub is_list_sender: bool,
    pub body: String,
    pub date: DateTime<Utc>,
}

impl super::Store {
    pub async fn upsert_contact_style(
        &self,
        record: &ContactStyleRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO contact_style
               (account_id, email, formality_score, formality_score_theirs,
                avg_sentence_len, avg_sentence_len_theirs, msg_count_used,
                msg_count_used_theirs, metrics_json, metrics_json_theirs,
                computed_at, source_hash, drift_detected, drift_reason, drift_detected_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id, email) DO UPDATE SET
                 formality_score = excluded.formality_score,
                 formality_score_theirs = excluded.formality_score_theirs,
                 avg_sentence_len = excluded.avg_sentence_len,
                 avg_sentence_len_theirs = excluded.avg_sentence_len_theirs,
                 msg_count_used = excluded.msg_count_used,
                 msg_count_used_theirs = excluded.msg_count_used_theirs,
                 metrics_json = excluded.metrics_json,
                 metrics_json_theirs = excluded.metrics_json_theirs,
                 computed_at = excluded.computed_at,
                 source_hash = excluded.source_hash,
                 drift_detected = excluded.drift_detected,
                 drift_reason = excluded.drift_reason,
                 drift_detected_at = excluded.drift_detected_at"#,
        )
        .bind(record.account_id.as_str())
        .bind(&record.email)
        .bind(record.formality_score)
        .bind(record.formality_score_theirs)
        .bind(record.avg_sentence_len)
        .bind(record.avg_sentence_len_theirs)
        .bind(record.msg_count_used as i64)
        .bind(record.msg_count_used_theirs as i64)
        .bind(&record.metrics_json)
        .bind(&record.metrics_json_theirs)
        .bind(record.computed_at.timestamp())
        .bind(&record.source_hash)
        .bind(record.drift_detected as i64)
        .bind(&record.drift_reason)
        .bind(record.drift_detected_at.map(|value| value.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn get_contact_style(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Option<ContactStyleRecord>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT account_id, email, formality_score, formality_score_theirs,
                      avg_sentence_len, avg_sentence_len_theirs, msg_count_used,
                      msg_count_used_theirs, metrics_json, metrics_json_theirs,
                       computed_at, source_hash, drift_detected, drift_reason, drift_detected_at
               FROM contact_style
               WHERE account_id = ? AND email = ? COLLATE NOCASE"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("contact_style.get", started_at, row.is_some());
        row.map(row_to_contact_style).transpose()
    }

    pub async fn relationship_contacts_for_messages(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<(AccountId, String)>, sqlx::Error> {
        let mut contacts = BTreeSet::<(String, String)>::new();
        for message_id in message_ids {
            let Some(row) = sqlx::query(
                r#"SELECT account_id, from_email, to_addrs, cc_addrs, bcc_addrs, direction
                   FROM messages WHERE id = ?"#,
            )
            .bind(message_id.as_str())
            .fetch_optional(self.reader())
            .await?
            else {
                continue;
            };
            let account_id: AccountId = decode_id(row.get::<String, _>("account_id").as_str())?;
            let owned = self
                .list_account_addresses(&account_id)
                .await?
                .into_iter()
                .map(|address| address.email.to_ascii_lowercase())
                .collect::<HashSet<_>>();
            let account_id_str = account_id.as_str();
            let direction =
                MessageDirection::from_db_str(row.get::<String, _>("direction").as_str())
                    .unwrap_or(MessageDirection::Unknown);
            if direction == MessageDirection::Outbound {
                for address in parse_addresses(&row.get::<String, _>("to_addrs"))?
                    .into_iter()
                    .chain(parse_addresses(&row.get::<String, _>("cc_addrs"))?)
                    .chain(parse_addresses(&row.get::<String, _>("bcc_addrs"))?)
                {
                    insert_contact(&mut contacts, &account_id_str, &owned, &address.email);
                }
            } else {
                insert_contact(
                    &mut contacts,
                    &account_id_str,
                    &owned,
                    &row.get::<String, _>("from_email"),
                );
            }
        }
        contacts
            .into_iter()
            .map(|(account_id, email)| Ok((decode_id(&account_id)?, email)))
            .collect()
    }

    pub async fn recent_contact_messages(
        &self,
        account_id: &AccountId,
        email: &str,
        limit: u32,
    ) -> Result<Vec<RelationshipMessageSample>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT m.id, m.account_id, m.thread_id, m.direction, m.from_email,
                      CASE WHEN m.list_id IS NOT NULL THEN 1 ELSE 0 END AS is_list_sender,
                      m.snippet, m.date, b.text_plain, b.text_html
               FROM messages m
               LEFT JOIN bodies b ON b.message_id = m.id
               WHERE m.account_id = ?
                 AND (
                   LOWER(m.from_email) = LOWER(?)
                   OR EXISTS (SELECT 1 FROM json_each(m.to_addrs) WHERE LOWER(json_extract(value, '$.email')) = LOWER(?))
                   OR EXISTS (SELECT 1 FROM json_each(m.cc_addrs) WHERE LOWER(json_extract(value, '$.email')) = LOWER(?))
                   OR EXISTS (SELECT 1 FROM json_each(m.bcc_addrs) WHERE LOWER(json_extract(value, '$.email')) = LOWER(?))
                 )
               ORDER BY m.date DESC
               LIMIT ?"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .bind(email)
        .bind(email)
        .bind(email)
        .bind(limit as i64)
        .fetch_all(self.reader())
        .await?;
        trace_query("contact_style.recent_messages", started_at, rows.len());
        rows.into_iter().map(row_to_message_sample).collect()
    }
}

fn row_to_contact_style(row: sqlx::sqlite::SqliteRow) -> Result<ContactStyleRecord, sqlx::Error> {
    Ok(ContactStyleRecord {
        account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
        email: row.get("email"),
        formality_score: row.get("formality_score"),
        formality_score_theirs: row.get("formality_score_theirs"),
        avg_sentence_len: row.get("avg_sentence_len"),
        avg_sentence_len_theirs: row.get("avg_sentence_len_theirs"),
        msg_count_used: row.get::<i64, _>("msg_count_used") as u32,
        msg_count_used_theirs: row.get::<i64, _>("msg_count_used_theirs") as u32,
        metrics_json: row.get("metrics_json"),
        metrics_json_theirs: row.get("metrics_json_theirs"),
        computed_at: decode_timestamp(row.get("computed_at"))?,
        source_hash: row.get("source_hash"),
        drift_detected: row.get::<i64, _>("drift_detected") != 0,
        drift_reason: row.get("drift_reason"),
        drift_detected_at: crate::decode_optional_timestamp(row.get("drift_detected_at"))?,
    })
}

fn row_to_message_sample(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RelationshipMessageSample, sqlx::Error> {
    let body = row
        .get::<Option<String>, _>("text_plain")
        .or_else(|| row.get::<Option<String>, _>("text_html"))
        .unwrap_or_else(|| row.get::<String, _>("snippet"));
    Ok(RelationshipMessageSample {
        message_id: decode_id(row.get::<String, _>("id").as_str())?,
        account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
        thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
        direction: MessageDirection::from_db_str(row.get::<String, _>("direction").as_str())
            .unwrap_or(MessageDirection::Unknown),
        from_email: row.get("from_email"),
        is_list_sender: row.get::<i64, _>("is_list_sender") != 0,
        body,
        date: decode_timestamp(row.get("date"))?,
    })
}

fn parse_addresses(value: &str) -> Result<Vec<Address>, sqlx::Error> {
    serde_json::from_str(value).map_err(sqlx::Error::decode)
}

fn insert_contact(
    contacts: &mut BTreeSet<(String, String)>,
    account_id: &str,
    owned: &HashSet<String>,
    email: &str,
) {
    let email = email.trim().to_ascii_lowercase();
    if email.is_empty() || owned.contains(&email) {
        return;
    }
    contacts.insert((account_id.to_string(), email));
}
