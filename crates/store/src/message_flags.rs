//! User-intent flags on individual messages.
//!
//! Distinct from [`mxr_core::types::MessageFlags`]: those mirror provider-side
//! flags (SEEN, FLAGGED, ANSWERED). This table holds local-only intents
//! the user expressed while triaging — currently just `reply_later`. A row
//! exists only when at least one local flag is non-default.
//!
//! Reply later can carry a time (`reply_later_due_at`): until then the
//! message is out of the queue and its conversation off the desk. Whether
//! it is back is a pure function of the clock, so a restart or a late wake
//! loop can never lose or repeat a return; `reply_later_returned_at` only
//! records that the return was announced.

use crate::{decode_id, decode_optional_timestamp, trace_query, SQLITE_BIND_CHUNK};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

/// A reply-later flag as stored, for putting it back exactly (undo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyLaterState {
    pub set_at: DateTime<Utc>,
    /// Timed reply later: when it comes back. `None` is untimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<DateTime<Utc>>,
    /// When the wake loop announced the return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returned_at: Option<DateTime<Utc>>,
}

/// A timed reply later in one account, for the desk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskReplyLater {
    pub thread_id: ThreadId,
    pub due_at: DateTime<Utc>,
}

/// Set a message's reply-later flag to exactly `state`: the one write every
/// path shares (flag, timed flag, restore, a fired reminder's queueing).
pub(crate) async fn upsert_reply_later<'e, E>(
    executor: E,
    message_id: &MessageId,
    state: &ReplyLaterState,
) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query(
        r#"INSERT INTO message_flags
               (message_id, reply_later, reply_later_set_at, reply_later_dismissed_at,
                reply_later_due_at, reply_later_returned_at)
           VALUES (?, 1, ?, NULL, ?, ?)
           ON CONFLICT(message_id) DO UPDATE SET
               reply_later = 1,
               reply_later_set_at = excluded.reply_later_set_at,
               reply_later_dismissed_at = NULL,
               reply_later_due_at = excluded.reply_later_due_at,
               reply_later_returned_at = excluded.reply_later_returned_at"#,
    )
    .bind(message_id.as_str())
    .bind(state.set_at.timestamp())
    .bind(state.due_at.map(|due| due.timestamp()))
    .bind(state.returned_at.map(|at| at.timestamp()))
    .execute(executor)
    .await?;
    Ok(())
}

impl super::Store {
    /// Mark a message for "reply later". Idempotent — re-marking refreshes
    /// `reply_later_set_at` so the queue surfaces the most recently
    /// flagged message first.
    pub async fn set_reply_later(
        &self,
        message_id: &MessageId,
        set_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        self.put_reply_later(message_id, set_at, None).await
    }

    /// Reply later until `due_at`: out of the queue (and its conversation
    /// off the desk) until then, back in both from then on. Replaces any
    /// earlier flag or time on the message.
    pub async fn defer_reply_later(
        &self,
        message_id: &MessageId,
        set_at: DateTime<Utc>,
        due_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        self.put_reply_later(message_id, set_at, Some(due_at)).await
    }

    /// Flag the message, timed or not. A new flag has not been announced.
    async fn put_reply_later(
        &self,
        message_id: &MessageId,
        set_at: DateTime<Utc>,
        due_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        let state = ReplyLaterState {
            set_at,
            due_at,
            returned_at: None,
        };
        upsert_reply_later(self.writer(), message_id, &state).await
    }

    /// The reply-later flags among `message_ids`, as stored (flagged only).
    pub async fn reply_later_states(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, ReplyLaterState>, sqlx::Error> {
        let mut states = HashMap::new();
        for chunk in message_ids.chunks(SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT message_id, reply_later_set_at, reply_later_due_at, reply_later_returned_at
                 FROM message_flags
                 WHERE reply_later = 1 AND message_id IN ({placeholders})"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for message_id in chunk {
                query = query.bind(message_id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                let id: String = row.try_get("message_id")?;
                let set_at: Option<i64> = row.try_get("reply_later_set_at")?;
                states.insert(
                    decode_id(&id)?,
                    ReplyLaterState {
                        set_at: set_at
                            .and_then(|ts| DateTime::from_timestamp(ts, 0))
                            .unwrap_or_else(Utc::now),
                        due_at: decode_optional_timestamp(row.try_get("reply_later_due_at")?)?,
                        returned_at: decode_optional_timestamp(
                            row.try_get("reply_later_returned_at")?,
                        )?,
                    },
                );
            }
        }
        Ok(states)
    }

    /// Put a message's reply-later flag back as it was: `None` clears it.
    pub async fn restore_reply_later(
        &self,
        message_id: &MessageId,
        prior: Option<ReplyLaterState>,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        match prior {
            Some(prior) => upsert_reply_later(self.writer(), message_id, &prior).await,
            None => self.clear_reply_later(message_id, now).await,
        }
    }

    /// Claim every timed reply later that is due by `now` and has not been
    /// announced: each id is returned by exactly one call, ever, because the
    /// same statement that finds a row marks it.
    pub async fn claim_returned_reply_later(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let now_ts = now.timestamp();
        let started_at = std::time::Instant::now();
        let ids: Vec<String> = sqlx::query_scalar(
            r#"UPDATE message_flags
               SET reply_later_returned_at = ?1
               WHERE reply_later = 1
                 AND reply_later_due_at IS NOT NULL
                 AND reply_later_due_at <= ?1
                 AND reply_later_returned_at IS NULL
               RETURNING message_id"#,
        )
        .bind(now_ts)
        .fetch_all(self.writer())
        .await?;
        trace_query("message_flags.claim_returned", started_at, ids.len());
        ids.iter().map(|id| decode_id(id)).collect()
    }

    /// Every timed reply later in the account, due or not, with its
    /// conversation: the desk hides those still to come and marks those
    /// that are back.
    pub async fn desk_reply_later(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<DeskReplyLater>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        // CROSS JOIN keeps the handful of flags as the outer loop: left to
        // itself SQLite walks every message in the account and probes the
        // flags, ~200ms on a 110k-message store against ~0.02ms.
        let rows = sqlx::query(
            r#"SELECT m.thread_id, f.reply_later_due_at
               FROM message_flags f
               CROSS JOIN messages m ON m.id = f.message_id
               WHERE f.reply_later = 1
                 AND f.reply_later_due_at IS NOT NULL
                 AND m.account_id = ?"#,
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        trace_query("message_flags.desk_reply_later", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                let thread_id: String = row.try_get("thread_id")?;
                let due_at: i64 = row.try_get("reply_later_due_at")?;
                Ok(DeskReplyLater {
                    thread_id: decode_id(&thread_id)?,
                    due_at: crate::decode_timestamp(due_at)?,
                })
            })
            .collect()
    }

    /// Clear the reply-later flag on a message. Records `dismissed_at` so
    /// future analytics can distinguish "user followed through" from "user
    /// abandoned" — the queue read-side simply filters on
    /// `reply_later = 1`.
    pub async fn clear_reply_later(
        &self,
        message_id: &MessageId,
        dismissed_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let dismissed_ts = dismissed_at.timestamp();

        sqlx::query!(
            r#"UPDATE message_flags
               SET reply_later = 0,
                   reply_later_dismissed_at = ?
               WHERE message_id = ?"#,
            dismissed_ts,
            mid,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn is_reply_later(&self, message_id: &MessageId) -> Result<bool, sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT reply_later as "reply_later!: i64"
               FROM message_flags
               WHERE message_id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;
        Ok(row.is_some_and(|r| r.reply_later == 1))
    }

    /// Reply-later flags for a set of messages, in one query.
    ///
    /// Sync asks this for a whole page; `is_reply_later` per message would
    /// be one round trip each. Ids are chunked to stay under SQLite's
    /// bind-parameter limit.
    pub async fn reply_later_message_ids(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashSet<MessageId>, sqlx::Error> {
        let mut flagged = HashSet::new();
        for chunk in message_ids.chunks(SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT message_id FROM message_flags
                 WHERE reply_later = 1 AND message_id IN ({placeholders})"
            );
            let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()));
            for message_id in chunk {
                query = query.bind(message_id.as_str());
            }
            for id in query.fetch_all(self.reader()).await? {
                flagged.insert(decode_id(&id)?);
            }
        }
        Ok(flagged)
    }

    /// List message IDs in the reply-later queue at `now`, most recent
    /// first: flagged, and for a timed flag only once its time has come,
    /// ranked from that time so a returning message tops the queue.
    pub async fn list_reply_later(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let ids: Vec<String> = sqlx::query_scalar(
            r#"SELECT message_id
               FROM message_flags
               WHERE reply_later = 1
                 AND (reply_later_due_at IS NULL OR reply_later_due_at <= ?)
               ORDER BY MAX(COALESCE(reply_later_set_at, 0), COALESCE(reply_later_due_at, 0)) DESC,
                        message_id"#,
        )
        .bind(now.timestamp())
        .fetch_all(self.reader())
        .await?;
        trace_query("message_flags.list_reply_later", started_at, ids.len());

        ids.iter().map(|id| crate::decode_id(id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use super::ReplyLaterState;
    use chrono::{Duration, TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId};
    use mxr_core::types::Envelope;

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    fn make_envelope(account_id: &AccountId, provider_id: &str) -> Envelope {
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .build();
        env.id = MessageId::new();
        env.provider_id = provider_id.to_string();
        env
    }

    async fn seed_envelope(store: &Store) -> MessageId {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = make_envelope(&account.id, "fake-msg-1");
        store.upsert_envelope(&envelope).await.unwrap();
        envelope.id
    }

    #[tokio::test]
    async fn is_reply_later_returns_false_for_unflagged_message() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;
        assert!(!store.is_reply_later(&id).await.unwrap());
    }

    #[tokio::test]
    async fn set_reply_later_persists_flag() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;

        store.set_reply_later(&id, anchor()).await.unwrap();

        assert!(
            store.is_reply_later(&id).await.unwrap(),
            "flag persists after set_reply_later"
        );
    }

    #[tokio::test]
    async fn clear_reply_later_unsets_flag() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;

        store.set_reply_later(&id, anchor()).await.unwrap();
        store.clear_reply_later(&id, anchor()).await.unwrap();

        assert!(
            !store.is_reply_later(&id).await.unwrap(),
            "flag clears after clear_reply_later"
        );
    }

    #[tokio::test]
    async fn list_reply_later_is_empty_for_fresh_store() {
        let store = Store::in_memory().await.unwrap();
        let listed = store
            .list_reply_later(anchor() + Duration::days(1))
            .await
            .unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn list_reply_later_returns_only_flagged_messages() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envs: Vec<Envelope> = (0..3)
            .map(|i| make_envelope(&account.id, &format!("msg-{i}")))
            .collect();
        for env in &envs {
            store.upsert_envelope(env).await.unwrap();
        }

        store.set_reply_later(&envs[1].id, anchor()).await.unwrap();

        let listed = store
            .list_reply_later(anchor() + Duration::days(1))
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], envs[1].id);
    }

    #[tokio::test]
    async fn list_reply_later_orders_by_set_at_descending() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envs: Vec<Envelope> = (0..3)
            .map(|i| make_envelope(&account.id, &format!("msg-{i}")))
            .collect();
        for env in &envs {
            store.upsert_envelope(env).await.unwrap();
        }

        // Flag in non-monotonic order; expect descending-by-set_at output.
        store.set_reply_later(&envs[1].id, anchor()).await.unwrap();
        store
            .set_reply_later(&envs[0].id, anchor() + Duration::seconds(60))
            .await
            .unwrap();
        store
            .set_reply_later(&envs[2].id, anchor() + Duration::seconds(120))
            .await
            .unwrap();

        let listed = store
            .list_reply_later(anchor() + Duration::days(1))
            .await
            .unwrap();
        assert_eq!(
            listed,
            vec![envs[2].id.clone(), envs[0].id.clone(), envs[1].id.clone()],
            "most recently set flag listed first"
        );
    }

    #[tokio::test]
    async fn re_setting_reply_later_refreshes_set_at() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let a = make_envelope(&account.id, "msg-a");
        let b = make_envelope(&account.id, "msg-b");
        store.upsert_envelope(&a).await.unwrap();
        store.upsert_envelope(&b).await.unwrap();

        store.set_reply_later(&a.id, anchor()).await.unwrap();
        store
            .set_reply_later(&b.id, anchor() + Duration::seconds(30))
            .await
            .unwrap();

        // Re-flag `a` with a fresher set_at.
        store
            .set_reply_later(&a.id, anchor() + Duration::seconds(90))
            .await
            .unwrap();

        let listed = store
            .list_reply_later(anchor() + Duration::days(1))
            .await
            .unwrap();
        assert_eq!(listed[0], a.id, "re-flagged message ranks first");
    }

    #[tokio::test]
    async fn timed_reply_later_stays_out_of_the_queue_until_due() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;
        let due = anchor() + Duration::days(2);

        store.defer_reply_later(&id, anchor(), due).await.unwrap();

        assert!(store
            .list_reply_later(due - Duration::seconds(1))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(store.list_reply_later(due).await.unwrap(), vec![id.clone()]);
        assert!(
            store.is_reply_later(&id).await.unwrap(),
            "the flag is set while deferred"
        );
    }

    #[tokio::test]
    async fn a_return_is_claimed_exactly_once() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;
        let due = anchor() + Duration::hours(3);
        store.defer_reply_later(&id, anchor(), due).await.unwrap();

        assert!(store
            .claim_returned_reply_later(due - Duration::seconds(1))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store.claim_returned_reply_later(due).await.unwrap(),
            vec![id.clone()]
        );
        assert!(
            store
                .claim_returned_reply_later(due + Duration::hours(1))
                .await
                .unwrap()
                .is_empty(),
            "a claimed return is never announced again"
        );

        // Setting a new time arms it again.
        let later = due + Duration::days(1);
        store.defer_reply_later(&id, due, later).await.unwrap();
        assert_eq!(
            store.claim_returned_reply_later(later).await.unwrap(),
            vec![id]
        );
    }

    #[tokio::test]
    async fn an_untimed_flag_clears_an_earlier_time() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;
        store
            .defer_reply_later(&id, anchor(), anchor() + Duration::days(5))
            .await
            .unwrap();
        store.set_reply_later(&id, anchor()).await.unwrap();

        assert_eq!(store.list_reply_later(anchor()).await.unwrap(), vec![id]);
    }

    #[tokio::test]
    async fn restore_puts_the_flag_back_exactly() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_envelope(&store).await;
        let due = anchor() + Duration::days(1);
        store.defer_reply_later(&id, anchor(), due).await.unwrap();
        let prior = store
            .reply_later_states(std::slice::from_ref(&id))
            .await
            .unwrap()
            .remove(&id);
        assert_eq!(
            prior,
            Some(ReplyLaterState {
                set_at: anchor(),
                due_at: Some(due),
                returned_at: None,
            })
        );

        store.clear_reply_later(&id, anchor()).await.unwrap();
        store
            .restore_reply_later(&id, prior, anchor())
            .await
            .unwrap();
        assert_eq!(
            store
                .reply_later_states(std::slice::from_ref(&id))
                .await
                .unwrap()
                .remove(&id),
            prior
        );

        store
            .restore_reply_later(&id, None, anchor())
            .await
            .unwrap();
        assert!(!store.is_reply_later(&id).await.unwrap());
    }

    #[tokio::test]
    async fn a_return_is_announced_once_across_restarts() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("restart.db");
        let due = anchor() + Duration::hours(2);
        let id = {
            let store = Store::new(&path).await.unwrap();
            let id = seed_envelope(&store).await;
            store.defer_reply_later(&id, anchor(), due).await.unwrap();
            id
        };

        // The daemon was down at the time; the first tick after a restart
        // announces it, and the tick after the next restart does not.
        {
            let store = Store::new(&path).await.unwrap();
            assert_eq!(
                store
                    .claim_returned_reply_later(due + Duration::days(1))
                    .await
                    .unwrap(),
                vec![id.clone()]
            );
        }
        let store = Store::new(&path).await.unwrap();
        assert!(store
            .claim_returned_reply_later(due + Duration::days(2))
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .list_reply_later(due + Duration::days(2))
                .await
                .unwrap(),
            vec![id],
            "still in the queue: the claim only records the announcement"
        );
    }
}
