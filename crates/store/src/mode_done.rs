//! Per-mode done marks (`mode_done`): Messages, Updates and Reading each
//! keep their own "done here" watermark per thread, read like
//! `DeskDismissal`, so done in one mode never clears another and a message
//! stored after the mark brings the thread back to that mode only.
//!
//! The caller reads the watermark from the messages it planned from and
//! writes exactly that, as the desk's Done does: a message stored between
//! plan and write stays uncovered.

use chrono::Utc;
use mxr_core::id::{AccountId, ThreadId};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

use crate::{decode_id, DeskDismissal, DeskDismissalRow};

/// Done in `mode` through the messages stored so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeDoneMark {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// `messages`, `updates` or `reading`.
    pub mode: String,
    pub through_seq: i64,
    pub through_count: i64,
}

/// A thread's mark in one mode before a write, so undo can put it back
/// exactly: `None` means it had none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeDonePrior {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<DeskDismissalRow>,
}

fn dismissal(row: &sqlx::sqlite::SqliteRow) -> Result<DeskDismissal, sqlx::Error> {
    Ok(DeskDismissal {
        through_seq: row.try_get("through_rowid")?,
        through_count: row.try_get::<i64, _>("through_count")?.max(0) as usize,
    })
}

async fn upsert_mark(
    tx: &mut sqlx::SqliteConnection,
    account_id: &AccountId,
    thread_id: &ThreadId,
    mode: &str,
    row: DeskDismissalRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO mode_done
             (account_id, thread_id, mode, through_rowid, through_count, done_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(account_id, thread_id, mode) DO UPDATE SET
             through_rowid = excluded.through_rowid,
             through_count = excluded.through_count,
             done_at = excluded.done_at",
    )
    .bind(account_id.as_str())
    .bind(thread_id.as_str())
    .bind(mode)
    .bind(row.through_seq)
    .bind(row.through_count)
    .bind(row.dismissed_at)
    .execute(tx)
    .await?;
    Ok(())
}

impl super::Store {
    /// Every thread marked done in `mode` in this account.
    pub async fn mode_done_marks(
        &self,
        account_id: &AccountId,
        mode: &str,
    ) -> Result<HashMap<ThreadId, DeskDismissal>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT thread_id, through_rowid, through_count FROM mode_done
             WHERE account_id = ?1 AND mode = ?2",
        )
        .bind(account_id.as_str())
        .bind(mode)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    dismissal(row)?,
                ))
            })
            .collect()
    }

    /// Every mode's mark on these threads, keyed by thread and mode.
    pub async fn mode_done_for_threads(
        &self,
        account_id: &AccountId,
        thread_ids: &[ThreadId],
    ) -> Result<HashMap<(ThreadId, String), DeskDismissal>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            "SELECT thread_id, mode, through_rowid, through_count FROM mode_done
             WHERE account_id = ?1 AND thread_id IN (SELECT value FROM json_each(?2))",
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    (
                        decode_id(row.try_get::<&str, _>("thread_id")?)?,
                        row.try_get::<String, _>("mode")?,
                    ),
                    dismissal(row)?,
                ))
            })
            .collect()
    }

    /// The marks these threads have now in each named mode, one entry per
    /// key asked for (`prior: None` when there is none).
    pub async fn mode_done_priors(
        &self,
        keys: &[(AccountId, ThreadId, String)],
    ) -> Result<Vec<ModeDonePrior>, sqlx::Error> {
        let mut priors = Vec::with_capacity(keys.len());
        for (account_id, thread_id, mode) in keys {
            let row = sqlx::query(
                "SELECT through_rowid, through_count, done_at FROM mode_done
                 WHERE account_id = ?1 AND thread_id = ?2 AND mode = ?3",
            )
            .bind(account_id.as_str())
            .bind(thread_id.as_str())
            .bind(mode)
            .fetch_optional(self.reader())
            .await?;
            let prior = row
                .map(|row| {
                    Ok::<_, sqlx::Error>(DeskDismissalRow {
                        through_seq: row.try_get("through_rowid")?,
                        through_count: row.try_get("through_count")?,
                        dismissed_at: row.try_get("done_at")?,
                    })
                })
                .transpose()?;
            priors.push(ModeDonePrior {
                account_id: account_id.clone(),
                thread_id: thread_id.clone(),
                mode: mode.clone(),
                prior,
            });
        }
        Ok(priors)
    }

    /// Mark threads done in their modes through the given watermarks.
    pub async fn mark_mode_done(&self, marks: &[ModeDoneMark]) -> Result<(), sqlx::Error> {
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
            upsert_mark(&mut tx, &mark.account_id, &mark.thread_id, &mark.mode, row).await?;
        }
        tx.commit().await
    }

    /// Put marks back as they were: the prior row, or none.
    pub async fn put_back_mode_done(&self, priors: &[ModeDonePrior]) -> Result<(), sqlx::Error> {
        if priors.is_empty() {
            return Ok(());
        }
        let mut tx = self.writer().begin().await?;
        for entry in priors {
            match entry.prior {
                Some(row) => {
                    upsert_mark(
                        &mut tx,
                        &entry.account_id,
                        &entry.thread_id,
                        &entry.mode,
                        row,
                    )
                    .await?;
                }
                None => {
                    sqlx::query(
                        "DELETE FROM mode_done
                         WHERE account_id = ?1 AND thread_id = ?2 AND mode = ?3",
                    )
                    .bind(entry.account_id.as_str())
                    .bind(entry.thread_id.as_str())
                    .bind(&entry.mode)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        tx.commit().await
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;
    use crate::Store;
    use mxr_core::id::ThreadId;

    use super::*;

    #[tokio::test]
    async fn marks_are_per_mode_and_undo_puts_them_back() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let thread = ThreadId::new();
        let mark = |mode: &str, seq| ModeDoneMark {
            account_id: account.id.clone(),
            thread_id: thread.clone(),
            mode: mode.to_string(),
            through_seq: seq,
            through_count: 2,
        };
        store.mark_mode_done(&[mark("messages", 7)]).await.unwrap();

        let messages = store
            .mode_done_marks(&account.id, "messages")
            .await
            .unwrap();
        assert_eq!(messages[&thread].through_seq, 7);
        assert!(store
            .mode_done_marks(&account.id, "updates")
            .await
            .unwrap()
            .is_empty());

        let key = (account.id.clone(), thread.clone(), "messages".to_string());
        let updates_key = (account.id.clone(), thread.clone(), "updates".to_string());
        let priors = store.mode_done_priors(&[key, updates_key]).await.unwrap();
        assert_eq!(priors[0].prior.map(|row| row.through_seq), Some(7));
        assert!(priors[1].prior.is_none());

        store
            .mark_mode_done(&[mark("messages", 9), mark("updates", 9)])
            .await
            .unwrap();
        store.put_back_mode_done(&priors).await.unwrap();
        let all = store
            .mode_done_for_threads(&account.id, std::slice::from_ref(&thread))
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[&(thread.clone(), "messages".to_string())].through_seq,
            7
        );
    }
}
