//! Receipts for unsubscribes mxr has sent, or may have sent, for a message.
//!
//! The daemon claims a message before it sends, so two requests cannot both
//! send. The claim is a row with method `pending`. A claim becomes a receipt
//! when the send is confirmed, or stays as the record of an outcome mxr cannot
//! tell. Only a send that certainly did not happen releases its claim.
//! Receipts live in their own table so that purging event_log history cannot
//! make a second unsubscribe go out.

use mxr_core::id::{AccountId, MessageId};

/// Method stored while a send is in flight or its outcome is unknown.
pub const PENDING_METHOD: &str = "pending";

impl super::Store {
    /// Claims the message for a send. Returns `false` when a receipt or
    /// another claim already exists, in which case nothing may be sent.
    pub async fn claim_unsubscribe(
        &self,
        message_id: &MessageId,
        account_id: &AccountId,
        now: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "INSERT OR IGNORE INTO unsubscribe_receipts (message_id, account_id, method, sent_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(message_id.as_str())
        .bind(account_id.as_str())
        .bind(PENDING_METHOD)
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Replaces this message's claim with the final method and time. Fails
    /// when there is no claim to confirm.
    pub async fn confirm_unsubscribe(
        &self,
        message_id: &MessageId,
        method: &str,
        sent_at: i64,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            "UPDATE unsubscribe_receipts SET method = ?, sent_at = ? WHERE message_id = ?",
        )
        .bind(method)
        .bind(sent_at)
        .bind(message_id.as_str())
        .execute(self.writer())
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    /// Removes a claim whose send certainly did not happen. A confirmed
    /// receipt is never removed here.
    pub async fn release_unsubscribe_claim(
        &self,
        message_id: &MessageId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM unsubscribe_receipts WHERE message_id = ? AND method = ?")
            .bind(message_id.as_str())
            .bind(PENDING_METHOD)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn has_unsubscribe_receipt(
        &self,
        message_id: &MessageId,
    ) -> Result<bool, sqlx::Error> {
        let row: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM unsubscribe_receipts WHERE message_id = ? LIMIT 1")
                .bind(message_id.as_str())
                .fetch_optional(self.reader())
                .await?;
        Ok(row.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::types::MessageDirection;

    const BACKFILL: &str = include_str!("../migrations/093_unsubscribe_receipts.sql");

    async fn insert_message(store: &Store, account: &AccountId, id: &MessageId) {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        envelope.id = id.clone();
        // The builder's default provider id is shared, so every message would
        // upsert onto one row.
        envelope.provider_id = id.as_str();
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
    }

    async fn insert_event(
        store: &Store,
        timestamp: i64,
        category: &str,
        account_id: Option<&AccountId>,
        message_id: Option<&MessageId>,
        summary: &str,
    ) {
        sqlx::query(
            "INSERT INTO event_log (timestamp, level, category, account_id, message_id, summary)
             VALUES (?, 'info', ?, ?, ?, ?)",
        )
        .bind(timestamp)
        .bind(category)
        .bind(account_id.map(|id| id.as_str()))
        .bind(message_id.map(|id| id.as_str()))
        .bind(summary)
        .execute(store.writer())
        .await
        .unwrap();
    }

    async fn receipts(store: &Store) -> Vec<(String, String, String, i64)> {
        sqlx::query_as(
            "SELECT message_id, account_id, method, sent_at FROM unsubscribe_receipts
             ORDER BY message_id",
        )
        .fetch_all(store.reader())
        .await
        .unwrap()
    }

    /// The backfill must keep exactly the messages the old event_log guard
    /// treated as sent, for messages that still exist.
    #[tokio::test]
    async fn backfill_records_exactly_what_the_event_log_guard_matched() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let owner = Some(&account.id);
        let acct = account.id.as_str();

        let plain = MessageId::new();
        let upper = MessageId::new();
        let wrong_category = MessageId::new();
        let unrelated_summary = MessageId::new();
        let earliest = MessageId::new();
        let via_message = MessageId::new();
        let deleted = MessageId::new();
        for id in [
            &plain,
            &upper,
            &wrong_category,
            &unrelated_summary,
            &earliest,
        ] {
            insert_message(&store, &account.id, id).await;
        }
        // No event carries this account, so it resolves only through messages.
        insert_message(&store, &account.id, &via_message).await;

        insert_event(
            &store,
            100,
            "mutation",
            owner,
            Some(&plain),
            "Unsubscribed from \"News\"",
        )
        .await;
        insert_event(
            &store,
            110,
            "mutation",
            owner,
            Some(&upper),
            "SENT UNSUBSCRIBE request for \"News\"",
        )
        .await;
        insert_event(
            &store,
            120,
            "sync",
            owner,
            Some(&wrong_category),
            "unsubscribe seen during sync",
        )
        .await;
        insert_event(
            &store,
            130,
            "mutation",
            owner,
            Some(&unrelated_summary),
            "Archived \"News\"",
        )
        .await;
        // The later row is inserted first, so the test would catch an
        // implementation that keeps the last row or the lowest id.
        insert_event(
            &store,
            300,
            "mutation",
            owner,
            Some(&earliest),
            "Unsubscribed again",
        )
        .await;
        insert_event(
            &store,
            200,
            "mutation",
            owner,
            Some(&earliest),
            "Unsubscribed from \"News\"",
        )
        .await;
        insert_event(
            &store,
            210,
            "mutation",
            None,
            Some(&via_message),
            "Unsubscribed from \"News\"",
        )
        .await;
        // The message was deleted; its event row survives in history only.
        insert_event(
            &store,
            220,
            "mutation",
            None,
            Some(&deleted),
            "Unsubscribed from \"News\"",
        )
        .await;
        insert_event(
            &store,
            230,
            "mutation",
            owner,
            None,
            "Unsubscribed from \"News\"",
        )
        .await;

        sqlx::raw_sql(BACKFILL)
            .execute(store.writer())
            .await
            .unwrap();

        let mut expected = vec![
            (plain.as_str(), acct.to_string(), "unknown".to_string(), 100),
            (upper.as_str(), acct.to_string(), "unknown".to_string(), 110),
            (
                earliest.as_str(),
                acct.to_string(),
                "unknown".to_string(),
                200,
            ),
            (
                via_message.as_str(),
                acct.to_string(),
                "unknown".to_string(),
                210,
            ),
        ];
        expected.sort();
        assert_eq!(receipts(&store).await, expected);

        // Parity with the guard it replaces, for messages that still exist.
        // `deleted` is excluded: the old guard matched it, but a receipt for a
        // message that no longer exists could never be checked again.
        for id in [
            &plain,
            &upper,
            &wrong_category,
            &unrelated_summary,
            &earliest,
            &via_message,
        ] {
            let old = store
                .has_event_for_message_with_summary(&id.as_str(), "mutation", "unsubscrib")
                .await
                .unwrap();
            let new = store.has_unsubscribe_receipt(id).await.unwrap();
            assert_eq!(old, new, "guard parity for {}", id.as_str());
        }
    }

    #[tokio::test]
    async fn a_claim_is_taken_once_and_confirmed_in_place() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let id = MessageId::new();
        insert_message(&store, &account.id, &id).await;

        assert!(store.claim_unsubscribe(&id, &account.id, 10).await.unwrap());
        assert!(
            !store.claim_unsubscribe(&id, &account.id, 11).await.unwrap(),
            "a second claim must not be granted"
        );
        assert!(store.has_unsubscribe_receipt(&id).await.unwrap());

        store.confirm_unsubscribe(&id, "mailto", 20).await.unwrap();
        let rows = receipts(&store).await;
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].2.as_str(), rows[0].3), ("mailto", 20));
    }

    #[tokio::test]
    async fn releasing_removes_only_a_pending_claim() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let pending = MessageId::new();
        let confirmed = MessageId::new();
        for id in [&pending, &confirmed] {
            insert_message(&store, &account.id, id).await;
        }

        store
            .claim_unsubscribe(&pending, &account.id, 1)
            .await
            .unwrap();
        store.release_unsubscribe_claim(&pending).await.unwrap();
        assert!(!store.has_unsubscribe_receipt(&pending).await.unwrap());
        assert!(store
            .claim_unsubscribe(&pending, &account.id, 2)
            .await
            .unwrap());

        store
            .claim_unsubscribe(&confirmed, &account.id, 1)
            .await
            .unwrap();
        store
            .confirm_unsubscribe(&confirmed, "one_click", 3)
            .await
            .unwrap();
        store.release_unsubscribe_claim(&confirmed).await.unwrap();
        assert!(
            store.has_unsubscribe_receipt(&confirmed).await.unwrap(),
            "a confirmed receipt must survive a release"
        );
    }

    #[tokio::test]
    async fn confirming_without_a_claim_is_an_error() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let id = MessageId::new();
        insert_message(&store, &account.id, &id).await;
        assert!(store.confirm_unsubscribe(&id, "mailto", 1).await.is_err());
    }

    #[tokio::test]
    async fn a_receipt_needs_a_message_and_goes_with_its_account() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let orphan = MessageId::new();
        assert!(
            store
                .claim_unsubscribe(&orphan, &account.id, 1)
                .await
                .is_err(),
            "a receipt must reference an existing message"
        );

        let id = MessageId::new();
        insert_message(&store, &account.id, &id).await;
        store.claim_unsubscribe(&id, &account.id, 1).await.unwrap();
        store.confirm_unsubscribe(&id, "mailto", 2).await.unwrap();
        store.delete_account(&account.id).await.unwrap();
        assert!(receipts(&store).await.is_empty());
    }
}
