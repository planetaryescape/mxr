//! Writes behind the desk's Done: dismissals at a watermark the caller
//! read, and the prior dismissal state an undo puts back.
//!
//! `dismiss_desk_threads` (desk.rs) reads its watermark at write time. Done
//! archives exactly the messages it planned from, so its dismissal must
//! cover exactly those too: a message stored between the plan and the
//! write stays uncovered and brings the conversation back.

use chrono::Utc;
use mxr_core::id::{AccountId, ThreadId};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::decode_id;

/// Dismiss a conversation through the messages stored so far: the highest
/// storage rowid and the message count, as `DeskDismissal` reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskDismissalMark {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub through_seq: i64,
    pub through_count: i64,
}

/// A conversation's dismissal row before Done wrote it, so undo can put
/// it back exactly: `None` means it had none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskDismissalPrior {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<DeskDismissalRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskDismissalRow {
    pub through_seq: i64,
    pub through_count: i64,
    pub dismissed_at: i64,
}

fn pairs_json(pairs: &[(AccountId, ThreadId)]) -> Result<String, sqlx::Error> {
    let pairs: Vec<[String; 2]> = pairs
        .iter()
        .map(|(account, thread)| [account.as_str(), thread.as_str()])
        .collect();
    serde_json::to_string(&pairs).map_err(|e| sqlx::Error::Encode(Box::new(e)))
}

async fn upsert_dismissal(
    tx: &mut sqlx::SqliteConnection,
    account_id: &AccountId,
    thread_id: &ThreadId,
    row: DeskDismissalRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO desk_dismissals
             (account_id, thread_id, through_rowid, through_count, dismissed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(account_id, thread_id) DO UPDATE SET
             through_rowid = excluded.through_rowid,
             through_count = excluded.through_count,
             dismissed_at = excluded.dismissed_at",
    )
    .bind(account_id.as_str())
    .bind(thread_id.as_str())
    .bind(row.through_seq)
    .bind(row.through_count)
    .bind(row.dismissed_at)
    .execute(tx)
    .await?;
    Ok(())
}

impl super::Store {
    /// The dismissal rows these conversations have now, one entry per pair
    /// asked for (`prior: None` when there is none).
    pub async fn desk_dismissal_priors(
        &self,
        pairs: &[(AccountId, ThreadId)],
    ) -> Result<Vec<DeskDismissalPrior>, sqlx::Error> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let wanted = pairs_json(pairs)?;
        let rows = sqlx::query(
            r#"SELECT d.account_id, d.thread_id, d.through_rowid, d.through_count, d.dismissed_at
               FROM json_each(?1) wanted
               JOIN desk_dismissals d
                 ON d.account_id = json_extract(wanted.value, '$[0]')
                AND d.thread_id = json_extract(wanted.value, '$[1]')"#,
        )
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let mut existing = std::collections::HashMap::new();
        for row in rows {
            existing.insert(
                (
                    decode_id::<AccountId>(row.try_get::<&str, _>("account_id")?)?,
                    decode_id::<ThreadId>(row.try_get::<&str, _>("thread_id")?)?,
                ),
                DeskDismissalRow {
                    through_seq: row.try_get("through_rowid")?,
                    through_count: row.try_get("through_count")?,
                    dismissed_at: row.try_get("dismissed_at")?,
                },
            );
        }
        Ok(pairs
            .iter()
            .map(|(account_id, thread_id)| DeskDismissalPrior {
                prior: existing
                    .get(&(account_id.clone(), thread_id.clone()))
                    .copied(),
                account_id: account_id.clone(),
                thread_id: thread_id.clone(),
            })
            .collect())
    }

    /// Dismiss conversations through the given watermarks (an upsert).
    pub async fn mark_desk_dismissals(
        &self,
        marks: &[DeskDismissalMark],
    ) -> Result<(), sqlx::Error> {
        if marks.is_empty() {
            return Ok(());
        }
        let now = Utc::now().timestamp();
        let mut tx = self.writer().begin().await?;
        for mark in marks {
            let row = DeskDismissalRow {
                through_seq: mark.through_seq,
                through_count: mark.through_count,
                dismissed_at: now,
            };
            upsert_dismissal(&mut tx, &mark.account_id, &mark.thread_id, row).await?;
        }
        tx.commit().await
    }

    /// Put dismissals back as they were: the prior row, or none.
    pub async fn put_back_desk_dismissals(
        &self,
        priors: &[DeskDismissalPrior],
    ) -> Result<(), sqlx::Error> {
        if priors.is_empty() {
            return Ok(());
        }
        let mut tx = self.writer().begin().await?;
        for entry in priors {
            match entry.prior {
                Some(row) => {
                    upsert_dismissal(&mut tx, &entry.account_id, &entry.thread_id, row).await?;
                }
                None => {
                    sqlx::query(
                        "DELETE FROM desk_dismissals WHERE account_id = ?1 AND thread_id = ?2",
                    )
                    .bind(entry.account_id.as_str())
                    .bind(entry.thread_id.as_str())
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        tx.commit().await
    }
}
