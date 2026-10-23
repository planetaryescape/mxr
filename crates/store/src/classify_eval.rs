//! Reads for `mxr classify eval`: which stored emails a scenario labels,
//! and which to-do rows and records were made from them. Read-only.

use crate::{decode_id, encode_json};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::MessageFlags;
use sqlx::Row;
use std::collections::HashSet;

/// A stored email whose provider id a scenario names. The sender is
/// returned so the caller can tell a scenario's email from another
/// account's message that happens to share the provider id (both demo
/// accounts number their mail from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalCandidate {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub provider_id: String,
    pub from_email: String,
    /// Flagged or labelled Spam, as placement reads it.
    pub spam: bool,
}

/// A to-do row, with what the eval grades about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalTodo {
    /// The email the row was made from.
    pub source_message_id: MessageId,
    /// `bill`, `payment_failed`, `renewal`, ... (`TodoKind::as_str`).
    pub kind: String,
    pub title: String,
    pub counterparty: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    /// The phrase the due date was read from, when there is a due date.
    pub due_words: Option<String>,
    /// The due date as stored (Unix seconds, the end of the day it resolved
    /// to in the daemon's time zone).
    pub due_at: Option<i64>,
}

/// The to-do rows and records made from some emails.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalAspects {
    /// In a stable order, whatever order the table returns rows in.
    pub todos: Vec<EvalTodo>,
    pub record: HashSet<MessageId>,
}

fn candidate(row: &sqlx::sqlite::SqliteRow) -> Result<EvalCandidate, sqlx::Error> {
    Ok(EvalCandidate {
        message_id: decode_id(row.try_get::<&str, _>("id")?)?,
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
        provider_id: row.try_get("provider_id")?,
        from_email: row.try_get("from_email")?,
        spam: row.try_get("spam")?,
    })
}

/// One SQLite statement supplies all the stored facets and evidence for grading.
#[derive(Debug)]
pub struct EvalStoredMessage {
    pub envelope: mxr_core::Envelope,
    pub home: Option<mxr_core::assessment::MailAssessment>,
    pub asks: Option<mxr_core::asks::AskAssessment>,
    pub record: bool,
    pub projected_units: HashSet<String>,
    pub projection_pending: bool,
}

impl super::Store {
    pub async fn classify_eval_packet(
        &self,
        ids: &[MessageId],
    ) -> Result<Vec<EvalStoredMessage>, sqlx::Error> {
        let context = crate::asks_context_token().map_err(sqlx::Error::Io)?;
        let rows = sqlx::query("SELECT m.*, COALESCE((SELECT GROUP_CONCAT(l.provider_id, char(31)) FROM message_labels ml JOIN labels l ON l.id = ml.label_id WHERE ml.message_id = m.id), '') AS label_provider_ids,
            h.assessment_json AS home_json, a.assessment_json AS asks_json,
            EXISTS(SELECT 1 FROM record_messages r WHERE r.message_id = m.id) AS is_record,
            (SELECT json_group_array(json_extract(t.field_sources, '$.asks_provenance.evidence')) FROM todos t WHERE t.account_id = m.account_id AND t.source_message_id = m.id AND t.expired_at_birth = 0 AND json_extract(t.field_sources, '$.asks_fingerprint.evidence') = a.input_fingerprint AND json_extract(t.field_sources, '$.asks_provenance.evidence') IS NOT NULL) AS projected_units,
            EXISTS(SELECT 1 FROM asks_projection_outbox q WHERE q.message_id = m.id AND q.account_id = m.account_id AND q.input_fingerprint = a.input_fingerprint) AS projection_pending
            FROM messages m LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id
            LEFT JOIN mail_assessments a ON a.message_id = m.id AND a.account_id = m.account_id AND a.policy_version = ?2 AND a.input_revision = i.effective_revision AND json_extract(a.assessment_json, '$.context_token') = ?4
            LEFT JOIN mail_assessments h ON h.message_id = m.id AND h.account_id = m.account_id AND h.policy_version = ?3 AND h.input_revision = i.effective_revision AND a.message_id IS NOT NULL AND m.direction != 'outbound' AND json_extract(a.assessment_json, '$.facts.owner_authored') = 0
            WHERE m.id IN (SELECT value FROM json_each(?1)) ORDER BY m.id")
            .bind(encode_json(&ids)?).bind(mxr_core::asks::ASKS_POLICY_VERSION).bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION).bind(context).fetch_all(self.reader()).await?;
        rows.iter()
            .map(|row| {
                Ok(EvalStoredMessage {
                    envelope: crate::message::sqlite_row_to_envelope(row)?,
                    home: row
                        .try_get::<Option<String>, _>("home_json")?
                        .map(|s| crate::decode_json(&s))
                        .transpose()?,
                    asks: row
                        .try_get::<Option<String>, _>("asks_json")?
                        .map(|s| crate::decode_json(&s))
                        .transpose()?,
                    record: row.try_get("is_record")?,
                    projected_units: crate::decode_json(
                        &row.try_get::<String, _>("projected_units")?,
                    )?,
                    projection_pending: row.try_get("projection_pending")?,
                })
            })
            .collect()
    }

    /// Stored emails of `accounts` (all accounts when empty) whose provider
    /// id is in `provider_ids`.
    pub async fn classify_eval_candidates(
        &self,
        accounts: &[AccountId],
        provider_ids: &[&str],
    ) -> Result<Vec<EvalCandidate>, sqlx::Error> {
        let accounts_json =
            encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())?;
        let provider_ids_json = encode_json(&provider_ids)?;
        let rows = sqlx::query(
            "SELECT id, account_id, thread_id, provider_id, from_email,
                    ((flags & ?3) != 0 OR EXISTS (
                        SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                        WHERE ml.message_id = messages.id AND l.provider_id = 'SPAM')) AS spam
             FROM messages
             WHERE provider_id IN (SELECT value FROM json_each(?1))
               AND (json_array_length(?2) = 0
                    OR account_id IN (SELECT value FROM json_each(?2)))",
        )
        .bind(provider_ids_json)
        .bind(accounts_json)
        .bind(i64::from(MessageFlags::SPAM.bits()))
        .fetch_all(self.reader())
        .await?;
        rows.iter().map(candidate).collect()
    }

    /// Every stored email of `accounts` (all accounts when empty), for a set
    /// that labels mail by where the generator put it rather than by a list
    /// of provider ids.
    pub async fn classify_eval_all_candidates(
        &self,
        accounts: &[AccountId],
    ) -> Result<Vec<EvalCandidate>, sqlx::Error> {
        let accounts_json =
            encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())?;
        let rows = sqlx::query(
            "SELECT id, account_id, thread_id, provider_id, from_email,
                    ((flags & ?2) != 0 OR EXISTS (
                        SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                        WHERE ml.message_id = messages.id AND l.provider_id = 'SPAM')) AS spam
             FROM messages
             WHERE json_array_length(?1) = 0
                OR account_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(accounts_json)
        .bind(i64::from(MessageFlags::SPAM.bits()))
        .fetch_all(self.reader())
        .await?;
        rows.iter().map(candidate).collect()
    }

    /// How many emails `accounts` hold (all accounts when empty).
    pub async fn classify_eval_message_count(
        &self,
        accounts: &[AccountId],
    ) -> Result<u32, sqlx::Error> {
        let accounts_json =
            encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages
             WHERE json_array_length(?1) = 0
                OR account_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(accounts_json)
        .fetch_one(self.reader())
        .await?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// The to-do rows and records made from `message_ids`. Closing a to-do
    /// or dismissing a record is the user's action, not the classifier's
    /// answer, so state is ignored; a to-do that was already over when it was
    /// found (`expired_at_birth`) is not one the product ever shows, so it is
    /// not counted.
    pub async fn classify_eval_aspects(
        &self,
        message_ids: &[MessageId],
    ) -> Result<EvalAspects, sqlx::Error> {
        let ids_json = encode_json(
            &message_ids
                .iter()
                .map(MessageId::as_str)
                .collect::<Vec<_>>(),
        )?;
        let todo_rows = sqlx::query(
            "SELECT source_message_id, kind, title, counterparty, amount_minor, currency,
                    due_words, due_at
             FROM todos
             WHERE source_message_id IN (SELECT value FROM json_each(?1))
               AND expired_at_birth = 0
             ORDER BY source_message_id, kind, title, counterparty, amount_minor, due_words",
        )
        .bind(ids_json.clone())
        .fetch_all(self.reader())
        .await?;
        let record = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT message_id FROM record_messages
             WHERE message_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(ids_json)
        .fetch_all(self.reader())
        .await?;
        Ok(EvalAspects {
            todos: todo_rows
                .iter()
                .map(|row| {
                    Ok(EvalTodo {
                        source_message_id: decode_id(row.try_get::<&str, _>("source_message_id")?)?,
                        kind: row.try_get("kind")?,
                        title: row.try_get("title")?,
                        counterparty: row.try_get("counterparty")?,
                        amount_minor: row.try_get("amount_minor")?,
                        currency: row.try_get("currency")?,
                        due_words: row.try_get("due_words")?,
                        due_at: row.try_get("due_at")?,
                    })
                })
                .collect::<Result<_, sqlx::Error>>()?,
            record: record
                .iter()
                .map(|id| decode_id(id))
                .collect::<Result<_, _>>()?,
        })
    }
}
