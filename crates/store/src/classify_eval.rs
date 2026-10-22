//! Reads for `mxr classify eval`: which stored emails a scenario labels,
//! and which of them made a to-do or a record. Read-only.

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

/// Emails a to-do or a record was made from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalAspects {
    pub todo: HashSet<MessageId>,
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

impl super::Store {
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

    /// Which of `message_ids` a to-do or a record was made from. Closing a
    /// to-do or dismissing a record is the user's action, not the
    /// classifier's answer, so state is ignored; a to-do that was already
    /// over when it was found (`expired_at_birth`) is not one the product
    /// ever shows, so it is not counted.
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
        let ids = |sql: &'static str| {
            sqlx::query_scalar::<_, String>(sql)
                .bind(ids_json.clone())
                .fetch_all(self.reader())
        };
        let todo = ids("SELECT DISTINCT source_message_id FROM todos
             WHERE source_message_id IN (SELECT value FROM json_each(?1))
               AND expired_at_birth = 0")
        .await?;
        let record = ids("SELECT DISTINCT message_id FROM record_messages
             WHERE message_id IN (SELECT value FROM json_each(?1))")
        .await?;
        Ok(EvalAspects {
            todo: todo
                .iter()
                .map(|id| decode_id(id))
                .collect::<Result<_, _>>()?,
            record: record
                .iter()
                .map(|id| decode_id(id))
                .collect::<Result<_, _>>()?,
        })
    }
}
