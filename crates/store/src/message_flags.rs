//! User-intent flags on individual messages.
//!
//! Distinct from [`mxr_core::types::MessageFlags`]: those mirror provider-side
//! flags (SEEN, FLAGGED, ANSWERED). This table holds local-only intents
//! the user expressed while triaging — currently just `reply_later`. A row
//! exists only when at least one local flag is non-default.

use crate::trace_query;
use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;

impl super::Store {
    /// Mark a message for "reply later". Idempotent — re-marking refreshes
    /// `reply_later_set_at` so the queue surfaces the most recently
    /// flagged message first.
    pub async fn set_reply_later(
        &self,
        message_id: &MessageId,
        set_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let set_at_ts = set_at.timestamp();

        sqlx::query!(
            r#"INSERT INTO message_flags (message_id, reply_later, reply_later_set_at, reply_later_dismissed_at)
               VALUES (?, 1, ?, NULL)
               ON CONFLICT(message_id) DO UPDATE SET
                   reply_later = 1,
                   reply_later_set_at = excluded.reply_later_set_at,
                   reply_later_dismissed_at = NULL"#,
            mid,
            set_at_ts,
        )
        .execute(self.writer())
        .await?;

        Ok(())
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

    /// List message IDs currently flagged for reply-later, ordered by
    /// `set_at` descending (most recently flagged first).
    pub async fn list_reply_later(&self) -> Result<Vec<MessageId>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT message_id as "message_id!"
               FROM message_flags
               WHERE reply_later = 1
               ORDER BY reply_later_set_at DESC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("message_flags.list_reply_later", started_at, rows.len());

        rows.into_iter()
            .map(|r| crate::decode_id(&r.message_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
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
        let listed = store.list_reply_later().await.unwrap();
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

        let listed = store.list_reply_later().await.unwrap();
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

        let listed = store.list_reply_later().await.unwrap();
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

        let listed = store.list_reply_later().await.unwrap();
        assert_eq!(listed[0], a.id, "re-flagged message ranks first");
    }
}
