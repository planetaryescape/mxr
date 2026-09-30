//! Auto-reminders: "remind me if no reply within N days."
//!
//! State machine:
//!
//!   * `pending`   — `triggered_at IS NULL AND cancelled_at IS NULL`
//!   * `triggered` — `triggered_at IS NOT NULL` (loop fired the reminder)
//!   * `cancelled` — `cancelled_at IS NOT NULL` (reply arrived first)
//!
//! Rows are append-once, mutated by status updates. Cancellation is
//! distinct from "deleted" so analytics can answer "how often did the
//! user actually need this nudge?" later.

use crate::SQLITE_BIND_CHUNK;
use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;

/// A reminder row as stored, for putting it back exactly (undo).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderState {
    pub account_id: AccountId,
    pub remind_at: DateTime<Utc>,
    pub set_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggered_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl ReminderState {
    /// Neither fired nor cancelled.
    pub fn is_pending(&self) -> bool {
        self.triggered_at.is_none() && self.cancelled_at.is_none()
    }
}

/// A stored message `r` that may answer the sent message `s` of a reminder:
/// inbound, from someone else, and either later in the same thread (by
/// storage order) or, in any thread, naming `s` in In-Reply-To or
/// References (IMAP can file a reply under another thread id). Whether it
/// is a person is the daemon's classifier's call.
const REPLY_CANDIDATE: &str = r#"r.account_id = s.account_id
    AND r.id != s.id
    AND r.direction != 'outbound'
    AND LOWER(r.from_email) != LOWER(s.from_email)
    AND (
        (r.thread_id = s.thread_id AND r.rowid > s.rowid)
        OR (
            COALESCE(s.message_id_header, '') != ''
            AND (
                r.in_reply_to = s.message_id_header
                OR instr(COALESCE(r.reference_headers, ''), '"' || s.message_id_header || '"') > 0
            )
        )
    )"#;

/// A stored message that may answer a reminder's sent message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyCandidate {
    pub sent_message_id: MessageId,
    pub reply_message_id: MessageId,
    pub reply_thread_id: ThreadId,
}

/// What `take_timers` took off, each as it was.
#[derive(Debug, Default)]
pub struct TakenTimers {
    pub reply_later: Vec<(MessageId, crate::ReplyLaterState)>,
    pub reminders: Vec<(MessageId, ReminderState)>,
}

/// A live (not cancelled) reminder in one account, with its conversation,
/// for the desk's Waiting on lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskReminder {
    pub thread_id: ThreadId,
    pub sent_message_id: MessageId,
    pub remind_at: DateTime<Utc>,
    pub triggered: bool,
}

/// A reminder row from the `auto_reminders` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoReminder {
    pub sent_message_id: MessageId,
    pub account_id: AccountId,
    pub remind_at: DateTime<Utc>,
    pub set_at: DateTime<Utc>,
    pub triggered_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

impl super::Store {
    /// Set or replace a reminder for an outbound message. Re-setting
    /// updates `remind_at` and `set_at`, and clears any prior
    /// triggered/cancelled state — useful for "I want to extend the
    /// reminder window" without bookkeeping.
    pub async fn set_auto_reminder(
        &self,
        sent_message_id: &MessageId,
        account_id: &AccountId,
        remind_at: DateTime<Utc>,
        set_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mid = sent_message_id.as_str();
        let aid = account_id.as_str();
        let remind_ts = remind_at.timestamp();
        let set_ts = set_at.timestamp();

        sqlx::query!(
            r#"INSERT INTO auto_reminders
                   (sent_message_id, account_id, remind_at, set_at,
                    triggered_at, cancelled_at)
               VALUES (?, ?, ?, ?, NULL, NULL)
               ON CONFLICT(sent_message_id) DO UPDATE SET
                   account_id = excluded.account_id,
                   remind_at = excluded.remind_at,
                   set_at = excluded.set_at,
                   triggered_at = NULL,
                   cancelled_at = NULL"#,
            mid,
            aid,
            remind_ts,
            set_ts,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    /// Mark a reminder cancelled — the user got their reply (or
    /// dismissed the reminder) before it could fire.
    pub async fn cancel_auto_reminder(
        &self,
        sent_message_id: &MessageId,
        cancelled_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mid = sent_message_id.as_str();
        let cancelled_ts = cancelled_at.timestamp();
        sqlx::query!(
            r#"UPDATE auto_reminders
               SET cancelled_at = ?
               WHERE sent_message_id = ? AND cancelled_at IS NULL"#,
            cancelled_ts,
            mid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Mark a reminder as triggered. Idempotent — already-triggered
    /// rows stay at their original `triggered_at`.
    pub async fn mark_auto_reminder_triggered(
        &self,
        sent_message_id: &MessageId,
        triggered_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mid = sent_message_id.as_str();
        let triggered_ts = triggered_at.timestamp();
        sqlx::query!(
            r#"UPDATE auto_reminders
               SET triggered_at = ?
               WHERE sent_message_id = ? AND triggered_at IS NULL"#,
            triggered_ts,
            mid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Fire one reminder: mark it triggered and put its message in the
    /// reply-later queue, in one transaction. Returns false when it had
    /// already fired, been cancelled, or been moved to a later time since
    /// the caller read it, so each reminder fires exactly once, at its
    /// current time, however often (or concurrently) the loop runs.
    ///
    /// `checked_through` is the highest message rowid the caller had when it
    /// checked the replies: the claim also refuses while any possible reply
    /// stored after that exists, so a reply landing between the check and
    /// the claim always wins. The next pass classifies it.
    pub async fn trigger_auto_reminder(
        &self,
        sent_message_id: &MessageId,
        now: DateTime<Utc>,
        checked_through: i64,
    ) -> Result<bool, sqlx::Error> {
        let mid = sent_message_id.as_str();
        let now_ts = now.timestamp();
        let mut tx = self.writer().begin().await?;
        let claim = format!(
            r#"UPDATE auto_reminders
               SET triggered_at = ?1
               WHERE sent_message_id = ?2
                 AND remind_at <= ?1
                 AND triggered_at IS NULL
                 AND cancelled_at IS NULL
                 AND NOT EXISTS (
                   SELECT 1 FROM messages s, messages r
                   WHERE s.id = ?2 AND r.rowid > ?3 AND {REPLY_CANDIDATE}
                 )"#
        );
        let claimed = sqlx::query(sqlx::AssertSqlSafe(claim.as_str()))
            .bind(now_ts)
            .bind(&mid)
            .bind(checked_through)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if claimed {
            let queued = crate::ReplyLaterState {
                set_at: now,
                due_at: None,
                returned_at: None,
            };
            crate::message_flags::upsert_reply_later(&mut *tx, sent_message_id, &queued).await?;
        }
        tx.commit().await?;
        Ok(claimed)
    }

    /// The reminders on `message_ids`, as stored.
    pub async fn auto_reminder_states(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, ReminderState>, sqlx::Error> {
        let mut states = HashMap::new();
        for chunk in message_ids.chunks(SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT sent_message_id, account_id, remind_at, set_at, triggered_at, cancelled_at
                 FROM auto_reminders
                 WHERE sent_message_id IN ({placeholders})"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for message_id in chunk {
                query = query.bind(message_id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                let id: String = row.try_get("sent_message_id")?;
                let account_id: String = row.try_get("account_id")?;
                states.insert(
                    decode_id(&id)?,
                    ReminderState {
                        account_id: decode_id(&account_id)?,
                        remind_at: decode_timestamp(row.try_get("remind_at")?)?,
                        set_at: decode_timestamp(row.try_get("set_at")?)?,
                        triggered_at: decode_optional_timestamp(row.try_get("triggered_at")?)?,
                        cancelled_at: decode_optional_timestamp(row.try_get("cancelled_at")?)?,
                    },
                );
            }
        }
        Ok(states)
    }

    /// Put a message's reminder back as it was: `None` removes the row.
    pub async fn restore_auto_reminder(
        &self,
        sent_message_id: &MessageId,
        prior: Option<&ReminderState>,
    ) -> Result<(), sqlx::Error> {
        let Some(prior) = prior else {
            sqlx::query("DELETE FROM auto_reminders WHERE sent_message_id = ?")
                .bind(sent_message_id.as_str())
                .execute(self.writer())
                .await?;
            return Ok(());
        };
        sqlx::query(
            r#"INSERT INTO auto_reminders
                   (sent_message_id, account_id, remind_at, set_at, triggered_at, cancelled_at)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(sent_message_id) DO UPDATE SET
                   account_id = excluded.account_id,
                   remind_at = excluded.remind_at,
                   set_at = excluded.set_at,
                   triggered_at = excluded.triggered_at,
                   cancelled_at = excluded.cancelled_at"#,
        )
        .bind(sent_message_id.as_str())
        .bind(prior.account_id.as_str())
        .bind(prior.remind_at.timestamp())
        .bind(prior.set_at.timestamp())
        .bind(prior.triggered_at.map(|at| at.timestamp()))
        .bind(prior.cancelled_at.map(|at| at.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Every reminder in the account that was not cancelled, with its
    /// conversation. Ones that fired before `fired_since` are left out: the
    /// desk marks recent returns. Judged by when it fired, not when it was
    /// due, so a reminder that fires late (the daemon was off for weeks)
    /// still comes back.
    pub async fn desk_reminders(
        &self,
        account_id: &AccountId,
        fired_since: DateTime<Utc>,
    ) -> Result<Vec<DeskReminder>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT m.thread_id, r.sent_message_id, r.remind_at, r.triggered_at
               FROM auto_reminders r
               CROSS JOIN messages m ON m.id = r.sent_message_id
               WHERE r.account_id = ?
                 AND r.cancelled_at IS NULL
                 AND (r.triggered_at IS NULL OR r.triggered_at >= ?)"#,
        )
        .bind(account_id.as_str())
        .bind(fired_since.timestamp())
        .fetch_all(self.reader())
        .await?;
        trace_query("auto_reminders.desk", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                let thread_id: String = row.try_get("thread_id")?;
                let sent: String = row.try_get("sent_message_id")?;
                let triggered_at: Option<i64> = row.try_get("triggered_at")?;
                Ok(DeskReminder {
                    thread_id: decode_id(&thread_id)?,
                    sent_message_id: decode_id(&sent)?,
                    remind_at: decode_timestamp(row.try_get("remind_at")?)?,
                    triggered: triggered_at.is_some(),
                })
            })
            .collect()
    }

    /// Reminders due to fire by `now`: pending (not triggered, not
    /// cancelled) with `remind_at <= now`. Ordered by `remind_at` so
    /// the oldest-pending fires first.
    pub async fn get_due_auto_reminders(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<AutoReminder>, sqlx::Error> {
        let now_ts = now.timestamp();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT sent_message_id as "sent_message_id!",
                      account_id as "account_id!",
                      remind_at as "remind_at!",
                      set_at as "set_at!",
                      triggered_at,
                      cancelled_at
               FROM auto_reminders
               WHERE triggered_at IS NULL
                 AND cancelled_at IS NULL
                 AND remind_at <= ?
               ORDER BY remind_at ASC"#,
            now_ts,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("auto_reminders.get_due", started_at, rows.len());

        rows.into_iter()
            .map(|r| {
                Ok(AutoReminder {
                    sent_message_id: decode_id(&r.sent_message_id)?,
                    account_id: decode_id(&r.account_id)?,
                    remind_at: decode_timestamp(r.remind_at)?,
                    set_at: decode_timestamp(r.set_at)?,
                    triggered_at: decode_optional_timestamp(r.triggered_at)?,
                    cancelled_at: decode_optional_timestamp(r.cancelled_at)?,
                })
            })
            .collect()
    }

    /// The highest message rowid stored so far: a watermark for
    /// `trigger_auto_reminder`, read before the replies are checked.
    pub async fn max_message_rowid(&self) -> Result<i64, sqlx::Error> {
        let max: Option<i64> = sqlx::query_scalar("SELECT MAX(rowid) FROM messages")
            .fetch_one(self.writer())
            .await?;
        Ok(max.unwrap_or(0))
    }

    /// Every stored message that may answer one of these sent messages
    /// (see `REPLY_CANDIDATE`), for the daemon to classify.
    pub async fn reminder_reply_candidates(
        &self,
        sent_message_ids: &[MessageId],
    ) -> Result<Vec<ReplyCandidate>, sqlx::Error> {
        let mut candidates = Vec::new();
        let sql = format!(
            r#"SELECT s.id AS sent_id, r.id AS reply_id, r.thread_id AS reply_thread
               FROM messages s, messages r
               WHERE s.id = ? AND {REPLY_CANDIDATE}"#
        );
        for sent in sent_message_ids {
            for row in sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(sent.as_str())
                .fetch_all(self.writer())
                .await?
            {
                let sent_id: String = row.try_get("sent_id")?;
                let reply_id: String = row.try_get("reply_id")?;
                let reply_thread: String = row.try_get("reply_thread")?;
                candidates.push(ReplyCandidate {
                    sent_message_id: decode_id(&sent_id)?,
                    reply_message_id: decode_id(&reply_id)?,
                    reply_thread_id: decode_id(&reply_thread)?,
                });
            }
        }
        Ok(candidates)
    }

    /// Cancel a reminder that has not fired: someone answered. Returns
    /// whether it was pending.
    pub async fn cancel_pending_auto_reminder(
        &self,
        sent_message_id: &MessageId,
        cancelled_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"UPDATE auto_reminders
               SET cancelled_at = ?
               WHERE sent_message_id = ?
                 AND triggered_at IS NULL
                 AND cancelled_at IS NULL"#,
        )
        .bind(cancelled_at.timestamp())
        .bind(sent_message_id.as_str())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Take every timer off these messages in one transaction: clear their
    /// reply-later flags and cancel their pending reminders, returning each
    /// as it was. A reminder firing concurrently either lands before (and
    /// its flag is taken here) or finds its reminder cancelled.
    pub async fn take_timers(
        &self,
        message_ids: &[MessageId],
        now: DateTime<Utc>,
    ) -> Result<TakenTimers, sqlx::Error> {
        let mut taken = TakenTimers::default();
        let now_ts = now.timestamp();
        let mut tx = self.writer().begin().await?;
        for chunk in message_ids.chunks(SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let flags = format!(
                "UPDATE message_flags
                 SET reply_later = 0, reply_later_dismissed_at = ?
                 WHERE reply_later = 1 AND message_id IN ({placeholders})
                 RETURNING message_id, reply_later_set_at, reply_later_due_at,
                           reply_later_returned_at"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(flags.as_str())).bind(now_ts);
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(&mut *tx).await? {
                let id: String = row.try_get("message_id")?;
                let set_at: Option<i64> = row.try_get("reply_later_set_at")?;
                taken.reply_later.push((
                    decode_id(&id)?,
                    crate::ReplyLaterState {
                        set_at: set_at
                            .and_then(|ts| DateTime::from_timestamp(ts, 0))
                            .unwrap_or(now),
                        due_at: decode_optional_timestamp(row.try_get("reply_later_due_at")?)?,
                        returned_at: decode_optional_timestamp(
                            row.try_get("reply_later_returned_at")?,
                        )?,
                    },
                ));
            }
            let reminders = format!(
                "UPDATE auto_reminders
                 SET cancelled_at = ?
                 WHERE triggered_at IS NULL AND cancelled_at IS NULL
                   AND sent_message_id IN ({placeholders})
                 RETURNING sent_message_id, account_id, remind_at, set_at"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(reminders.as_str())).bind(now_ts);
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(&mut *tx).await? {
                let id: String = row.try_get("sent_message_id")?;
                let account_id: String = row.try_get("account_id")?;
                taken.reminders.push((
                    decode_id(&id)?,
                    ReminderState {
                        account_id: decode_id(&account_id)?,
                        remind_at: decode_timestamp(row.try_get("remind_at")?)?,
                        set_at: decode_timestamp(row.try_get("set_at")?)?,
                        triggered_at: None,
                        cancelled_at: None,
                    },
                ));
            }
        }
        tx.commit().await?;
        Ok(taken)
    }

    /// All reminders for a given message — useful for the UI / debug.
    /// Returns at most one row by primary-key.
    pub async fn get_auto_reminder(
        &self,
        sent_message_id: &MessageId,
    ) -> Result<Option<AutoReminder>, sqlx::Error> {
        let mid = sent_message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT sent_message_id as "sent_message_id!",
                      account_id as "account_id!",
                      remind_at as "remind_at!",
                      set_at as "set_at!",
                      triggered_at,
                      cancelled_at
               FROM auto_reminders
               WHERE sent_message_id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(AutoReminder {
                sent_message_id: decode_id(&r.sent_message_id)?,
                account_id: decode_id(&r.account_id)?,
                remind_at: decode_timestamp(r.remind_at)?,
                set_at: decode_timestamp(r.set_at)?,
                triggered_at: decode_optional_timestamp(r.triggered_at)?,
                cancelled_at: decode_optional_timestamp(r.cancelled_at)?,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use chrono::{Duration, TimeZone, Utc};
    use mxr_core::id::MessageId;
    use mxr_core::types::Envelope;

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    async fn seed(store: &Store) -> (mxr_core::id::AccountId, Envelope) {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        env.id = MessageId::new();
        env.provider_id = "fake-1".into();
        store.upsert_envelope(&env).await.unwrap();
        (account.id, env)
    }

    #[tokio::test]
    async fn set_auto_reminder_persists_and_round_trips() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        let remind_at = anchor() + Duration::days(5);

        store
            .set_auto_reminder(&env.id, &account_id, remind_at, anchor())
            .await
            .unwrap();

        let stored = store
            .get_auto_reminder(&env.id)
            .await
            .unwrap()
            .expect("reminder stored");
        assert_eq!(stored.sent_message_id, env.id);
        assert_eq!(stored.remind_at, remind_at);
        assert_eq!(stored.set_at, anchor());
        assert!(stored.triggered_at.is_none());
        assert!(stored.cancelled_at.is_none());
    }

    #[tokio::test]
    async fn re_setting_clears_triggered_and_cancelled_state() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(
                &env.id,
                &account_id,
                anchor() + Duration::hours(1),
                anchor(),
            )
            .await
            .unwrap();
        store
            .mark_auto_reminder_triggered(&env.id, anchor() + Duration::hours(2))
            .await
            .unwrap();

        // Re-set: a new window, fresh state.
        store
            .set_auto_reminder(&env.id, &account_id, anchor() + Duration::days(2), anchor())
            .await
            .unwrap();

        let stored = store.get_auto_reminder(&env.id).await.unwrap().unwrap();
        assert!(
            stored.triggered_at.is_none(),
            "re-set clears prior triggered_at"
        );
        assert!(
            stored.cancelled_at.is_none(),
            "re-set clears prior cancelled_at"
        );
    }

    #[tokio::test]
    async fn get_due_excludes_future_reminders() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(&env.id, &account_id, anchor() + Duration::days(5), anchor())
            .await
            .unwrap();

        let due = store.get_due_auto_reminders(anchor()).await.unwrap();
        assert!(due.is_empty(), "future reminders are not due yet");
    }

    #[tokio::test]
    async fn get_due_includes_past_pending_reminders() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(
                &env.id,
                &account_id,
                anchor() - Duration::hours(1),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();

        let due = store.get_due_auto_reminders(anchor()).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].sent_message_id, env.id);
    }

    #[tokio::test]
    async fn get_due_excludes_triggered_reminders() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(
                &env.id,
                &account_id,
                anchor() - Duration::hours(1),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();
        store
            .mark_auto_reminder_triggered(&env.id, anchor())
            .await
            .unwrap();

        let due = store.get_due_auto_reminders(anchor()).await.unwrap();
        assert!(due.is_empty(), "triggered reminders are excluded");
    }

    #[tokio::test]
    async fn get_due_excludes_cancelled_reminders() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(
                &env.id,
                &account_id,
                anchor() - Duration::hours(1),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();
        store.cancel_auto_reminder(&env.id, anchor()).await.unwrap();

        let due = store.get_due_auto_reminders(anchor()).await.unwrap();
        assert!(due.is_empty(), "cancelled reminders are excluded");
    }

    #[tokio::test]
    async fn get_due_orders_by_remind_at_ascending() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let make = |ix: u32| {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = format!("msg-{ix}");
            env
        };
        let envs: Vec<Envelope> = (0..3).map(make).collect();
        for env in &envs {
            store.upsert_envelope(env).await.unwrap();
        }

        // Three reminders, all in the past, with different remind_at.
        // Set in non-chronological order; expect ascending by remind_at.
        store
            .set_auto_reminder(
                &envs[0].id,
                &account.id,
                anchor() - Duration::hours(2),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();
        store
            .set_auto_reminder(
                &envs[1].id,
                &account.id,
                anchor() - Duration::hours(5),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();
        store
            .set_auto_reminder(
                &envs[2].id,
                &account.id,
                anchor() - Duration::hours(1),
                anchor() - Duration::days(2),
            )
            .await
            .unwrap();

        let due = store.get_due_auto_reminders(anchor()).await.unwrap();
        let order: Vec<_> = due.iter().map(|r| r.sent_message_id.clone()).collect();
        assert_eq!(
            order,
            vec![envs[1].id.clone(), envs[0].id.clone(), envs[2].id.clone()],
            "oldest-due fires first"
        );
    }

    #[tokio::test]
    async fn a_reminder_fires_once_and_queues_its_message() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(&env.id, &account_id, anchor(), anchor() - Duration::days(1))
            .await
            .unwrap();

        assert!(store
            .trigger_auto_reminder(&env.id, anchor(), i64::MAX)
            .await
            .unwrap());
        assert!(
            !store
                .trigger_auto_reminder(&env.id, anchor() + Duration::minutes(1), i64::MAX)
                .await
                .unwrap(),
            "a fired reminder never fires again"
        );
        assert!(store.is_reply_later(&env.id).await.unwrap());
        let row = store.get_auto_reminder(&env.id).await.unwrap().unwrap();
        assert_eq!(row.triggered_at, Some(anchor()));
    }

    #[tokio::test]
    async fn a_cancelled_reminder_does_not_fire() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(&env.id, &account_id, anchor(), anchor() - Duration::days(1))
            .await
            .unwrap();
        store.cancel_auto_reminder(&env.id, anchor()).await.unwrap();

        assert!(!store
            .trigger_auto_reminder(&env.id, anchor(), i64::MAX)
            .await
            .unwrap());
        assert!(!store.is_reply_later(&env.id).await.unwrap());
    }

    #[tokio::test]
    async fn restore_puts_a_reminder_back_or_removes_it() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        let ids = std::slice::from_ref(&env.id);
        assert!(store.auto_reminder_states(ids).await.unwrap().is_empty());

        store
            .set_auto_reminder(&env.id, &account_id, anchor(), anchor() - Duration::days(1))
            .await
            .unwrap();
        let prior = store
            .auto_reminder_states(ids)
            .await
            .unwrap()
            .remove(&env.id);
        assert!(prior.as_ref().is_some_and(super::ReminderState::is_pending));

        store.cancel_auto_reminder(&env.id, anchor()).await.unwrap();
        store
            .restore_auto_reminder(&env.id, prior.as_ref())
            .await
            .unwrap();
        assert_eq!(
            store
                .auto_reminder_states(ids)
                .await
                .unwrap()
                .remove(&env.id),
            prior
        );

        store.restore_auto_reminder(&env.id, None).await.unwrap();
        assert!(store.get_auto_reminder(&env.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_reminder_moved_later_after_it_was_read_does_not_fire_at_the_old_time() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        store
            .set_auto_reminder(&env.id, &account_id, anchor(), anchor() - Duration::days(1))
            .await
            .unwrap();
        // The loop read it as due...
        assert_eq!(
            store.get_due_auto_reminders(anchor()).await.unwrap().len(),
            1
        );
        // ...then the user moved it to next week before the claim.
        let later = anchor() + Duration::days(7);
        store
            .set_auto_reminder(&env.id, &account_id, later, anchor())
            .await
            .unwrap();

        assert!(!store
            .trigger_auto_reminder(&env.id, anchor(), i64::MAX)
            .await
            .unwrap());
        assert!(!store.is_reply_later(&env.id).await.unwrap());
        assert!(store
            .trigger_auto_reminder(&env.id, later, i64::MAX)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn take_timers_clears_flags_and_cancels_pending_reminders_returning_each() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, env) = seed(&store).await;
        let ids = std::slice::from_ref(&env.id);
        store
            .set_auto_reminder(&env.id, &account_id, anchor(), anchor() - Duration::days(1))
            .await
            .unwrap();
        store
            .defer_reply_later(&env.id, anchor(), anchor() + Duration::days(2))
            .await
            .unwrap();

        let taken = store.take_timers(ids, anchor()).await.unwrap();
        assert_eq!(taken.reply_later.len(), 1);
        assert_eq!(
            taken.reply_later[0].1.due_at,
            Some(anchor() + Duration::days(2))
        );
        assert_eq!(taken.reminders.len(), 1);
        assert!(!store.is_reply_later(&env.id).await.unwrap());
        assert!(!store
            .trigger_auto_reminder(&env.id, anchor(), i64::MAX)
            .await
            .unwrap());
        assert!(store
            .take_timers(ids, anchor())
            .await
            .unwrap()
            .reply_later
            .is_empty());
    }
}
