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

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};

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

    /// Cancel every reminder due by `now` whose thread got an answer after
    /// the send: someone other than the sender wrote in it. Reply headers
    /// already cancel a reminder when the answer names the sent message
    /// (`reply_pairs`); a reply to an earlier message in the thread, or one
    /// whose client dropped the headers, is caught here, just before firing.
    /// Returns how many were cancelled.
    pub async fn cancel_due_reminders_with_replies(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let now_ts = now.timestamp();
        // Dates have second precision, so a reply in the same second as the
        // send counts (`>=`); the sent message and your own follow-ups are
        // excluded by id and sender. The unary `+` keeps SQLite on the
        // thread index: left to itself it walks (account_id, date) over
        // every later message in the account, ~70ms per reminder on a
        // 110k-message store against ~0.03ms.
        let result = sqlx::query(
            r#"UPDATE auto_reminders
               SET cancelled_at = ?
               WHERE triggered_at IS NULL
                 AND cancelled_at IS NULL
                 AND remind_at <= ?
                 AND EXISTS (
                   SELECT 1
                   FROM messages sent
                   JOIN messages reply
                     ON reply.thread_id = sent.thread_id
                    AND +reply.account_id = sent.account_id
                    AND reply.id != sent.id
                    AND +reply.date >= sent.date
                   WHERE sent.id = auto_reminders.sent_message_id
                     AND reply.direction != 'outbound'
                     AND LOWER(reply.from_email) != LOWER(sent.from_email)
                 )"#,
        )
        .bind(now_ts)
        .bind(now_ts)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
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
    async fn due_reminders_cancel_only_when_someone_else_wrote_after_the_send() {
        let store = Store::in_memory().await.unwrap();
        let (account_id, sent) = seed(&store).await;
        let due = sent.date + Duration::days(3);
        store
            .set_auto_reminder(&sent.id, &account_id, due, sent.date)
            .await
            .unwrap();
        let later = due + Duration::hours(1);

        let add = |id: &str, from: mxr_core::types::Address, date| {
            let mut env = TestEnvelopeBuilder::new()
                .account_id(account_id.clone())
                .build();
            env.id = MessageId::new();
            env.provider_id = id.into();
            env.thread_id = sent.thread_id.clone();
            env.from = from;
            env.date = date;
            env
        };
        let maya = mxr_core::types::Address {
            name: None,
            email: "maya@example.com".into(),
        };
        store
            .upsert_envelope(&add(
                "own",
                sent.from.clone(),
                sent.date + Duration::hours(1),
            ))
            .await
            .unwrap();
        store
            .upsert_envelope(&add("before", maya.clone(), sent.date - Duration::hours(1)))
            .await
            .unwrap();
        assert_eq!(
            store
                .cancel_due_reminders_with_replies(later)
                .await
                .unwrap(),
            0,
            "your own follow-up and mail from before the send are not replies"
        );

        // A headerless reply in the same second as the send still counts.
        store
            .upsert_envelope(&add("reply", maya, sent.date))
            .await
            .unwrap();
        assert_eq!(
            store
                .cancel_due_reminders_with_replies(due - Duration::hours(1))
                .await
                .unwrap(),
            0,
            "a reminder that isn't due yet is left for reply_pairs or later"
        );
        assert_eq!(
            store
                .cancel_due_reminders_with_replies(later)
                .await
                .unwrap(),
            1
        );
        assert!(store
            .get_due_auto_reminders(later)
            .await
            .unwrap()
            .is_empty());
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
}
