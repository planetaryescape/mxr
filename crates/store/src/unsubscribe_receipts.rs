//! Receipts that mxr sent an unsubscribe for a message.
//!
//! The daemon checks these before it sends anything again. They live in
//! their own table so that purging event_log history cannot make a second
//! unsubscribe go out.

use mxr_core::id::{AccountId, MessageId};

impl super::Store {
    /// Records a sent unsubscribe. The first receipt for a message is kept.
    pub async fn record_unsubscribe_receipt(
        &self,
        message_id: &MessageId,
        account_id: &AccountId,
        method: &str,
        sent_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR IGNORE INTO unsubscribe_receipts (message_id, account_id, method, sent_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(message_id.as_str())
        .bind(account_id.as_str())
        .bind(method)
        .bind(sent_at)
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
    /// treated as sent, so the guard never lets a message through that it
    /// used to block.
    #[tokio::test]
    async fn backfill_records_exactly_what_the_event_log_guard_matched() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let acct = account.id.as_str();
        let owner = Some(&account.id);

        let plain = MessageId::new();
        let upper = MessageId::new();
        let wrong_category = MessageId::new();
        let unrelated_summary = MessageId::new();
        let earliest = MessageId::new();
        let via_message = MessageId::new();
        let unresolvable = MessageId::new();

        // The envelope gives `via_message` an account only through messages.
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        envelope.id = via_message.clone();
        store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();

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
        insert_event(
            &store,
            220,
            "mutation",
            None,
            Some(&unresolvable),
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

        // Parity with the guard it replaces, for every message with an
        // account. `unresolvable` is excluded: the old guard matched it, but
        // no account can be attached, so the receipt is skipped by design.
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
    async fn a_receipt_is_kept_once_and_survives_a_second_record() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let id = MessageId::new();

        assert!(!store.has_unsubscribe_receipt(&id).await.unwrap());
        store
            .record_unsubscribe_receipt(&id, &account.id, "mailto", 10)
            .await
            .unwrap();
        store
            .record_unsubscribe_receipt(&id, &account.id, "one_click", 20)
            .await
            .unwrap();

        assert!(store.has_unsubscribe_receipt(&id).await.unwrap());
        let rows = receipts(&store).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, "mailto");
        assert_eq!(rows[0].3, 10);
    }
}
