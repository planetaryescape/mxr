//! Got it's one acknowledgement per message. A claim is written before the
//! reply goes to the provider and is never removed by a failure after that,
//! so a send that timed out or failed to land locally can't be sent twice.

use crate::{decode_id, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, DraftId, MessageId, ThreadId};
use sqlx::Row;
use std::time::Instant;

/// An acknowledgement already claimed for a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotItRecord {
    pub draft_id: DraftId,
    /// Set once the send is confirmed and stored.
    pub sent_message_id: Option<MessageId>,
    /// Unix seconds.
    pub claimed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GotItClaim {
    /// This call holds the claim: it may send.
    Claimed,
    /// Someone already claimed this message: never send again.
    Existing(GotItRecord),
}

impl super::Store {
    /// Claim the acknowledgement of `target` for `draft_id`, atomically:
    /// exactly one caller gets `Claimed`.
    pub async fn claim_got_it(
        &self,
        account_id: &AccountId,
        thread_id: &ThreadId,
        target: &MessageId,
        draft_id: &DraftId,
        at: DateTime<Utc>,
    ) -> Result<GotItClaim, sqlx::Error> {
        let inserted = sqlx::query(
            "INSERT INTO got_it_sends
               (target_message_id, account_id, thread_id, draft_id, sent_message_id, claimed_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)
             ON CONFLICT(target_message_id) DO NOTHING",
        )
        .bind(target.as_str())
        .bind(account_id.as_str())
        .bind(thread_id.as_str())
        .bind(draft_id.as_str())
        .bind(at.timestamp())
        .execute(self.writer())
        .await?
        .rows_affected();
        if inserted == 1 {
            return Ok(GotItClaim::Claimed);
        }
        self.got_it_for(target).await?.map_or_else(
            || Err(sqlx::Error::RowNotFound),
            |record| Ok(GotItClaim::Existing(record)),
        )
    }

    /// Record that the claimed acknowledgement was sent and stored.
    pub async fn finish_got_it(
        &self,
        target: &MessageId,
        sent_message_id: &MessageId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE got_it_sends SET sent_message_id = ?2 WHERE target_message_id = ?1")
            .bind(target.as_str())
            .bind(sent_message_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// The claim on `target`, if any. Read from the writer so a claim made
    /// a moment ago is always seen.
    pub async fn got_it_for(&self, target: &MessageId) -> Result<Option<GotItRecord>, sqlx::Error> {
        let started_at = Instant::now();
        let row = sqlx::query(
            "SELECT draft_id, sent_message_id, claimed_at FROM got_it_sends
             WHERE target_message_id = ?1",
        )
        .bind(target.as_str())
        .fetch_optional(self.writer())
        .await?;
        trace_query("got_it.for", started_at, usize::from(row.is_some()));
        row.map(|row| {
            Ok(GotItRecord {
                draft_id: decode_id(row.try_get::<&str, _>("draft_id")?)?,
                sent_message_id: row
                    .try_get::<Option<&str>, _>("sent_message_id")?
                    .map(decode_id)
                    .transpose()?,
                claimed_at: row.try_get("claimed_at")?,
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests;
