//! Receipts for unsubscribes mxr has sent, or may have sent, for a message.
//!
//! An attempt claims its message before it sends, so two requests cannot both
//! send. The claim is a row with method `pending`. A confirmed send replaces
//! it with the real method and time. A failed attempt releases its claim, so
//! a retry can go out. A claim older than `STALE_CLAIM_SECS` is treated as
//! abandoned (a crash mid-send) and taken over.
//!
//! Receipts live in their own table so that purging event_log history cannot
//! make a second unsubscribe go out.

use mxr_core::id::{AccountId, MessageId};

/// Method stored while an attempt is in flight.
pub const PENDING_METHOD: &str = "pending";

/// How long a pending claim holds the message before another attempt may take
/// it over.
pub const STALE_CLAIM_SECS: i64 = 600;

/// What a claim attempt found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// This attempt owns the message and may send. `claimed_at` identifies
    /// the claim for `release_unsubscribe_claim`.
    Claimed { claimed_at: i64 },
    /// A confirmed receipt exists. Nothing may be sent.
    Confirmed,
    /// Another attempt holds a live claim. Nothing may be sent now.
    InProgress,
}

impl super::Store {
    /// Claims the message for a send, or reports why it cannot be claimed.
    pub async fn claim_unsubscribe(
        &self,
        message_id: &MessageId,
        account_id: &AccountId,
        now: i64,
    ) -> Result<Claim, sqlx::Error> {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO unsubscribe_receipts (message_id, account_id, method, sent_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(message_id.as_str())
        .bind(account_id.as_str())
        .bind(PENDING_METHOD)
        .bind(now)
        .execute(self.writer())
        .await?;
        if inserted.rows_affected() == 1 {
            return Ok(Claim::Claimed { claimed_at: now });
        }

        let taken_over = sqlx::query(
            "UPDATE unsubscribe_receipts SET sent_at = ?
             WHERE message_id = ? AND method = ? AND sent_at < ?",
        )
        .bind(now)
        .bind(message_id.as_str())
        .bind(PENDING_METHOD)
        .bind(now - STALE_CLAIM_SECS)
        .execute(self.writer())
        .await?;
        if taken_over.rows_affected() == 1 {
            return Ok(Claim::Claimed { claimed_at: now });
        }

        // Neither insert nor takeover won: a confirmed receipt, or a live
        // claim. A row released between the two statements reads as a live
        // claim, which is safe: the caller only gets a retryable error.
        let method: Option<String> =
            sqlx::query_scalar("SELECT method FROM unsubscribe_receipts WHERE message_id = ?")
                .bind(message_id.as_str())
                .fetch_optional(self.writer())
                .await?;
        Ok(match method.as_deref() {
            Some(PENDING_METHOD) | None => Claim::InProgress,
            Some(_) => Claim::Confirmed,
        })
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

    /// Removes this attempt's claim after a failed send. Matches the claim's
    /// time, so a claim taken over from this attempt is left alone. A
    /// confirmed receipt is never removed here.
    pub async fn release_unsubscribe_claim(
        &self,
        message_id: &MessageId,
        claimed_at: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "DELETE FROM unsubscribe_receipts
             WHERE message_id = ? AND method = ? AND sent_at = ?",
        )
        .bind(message_id.as_str())
        .bind(PENDING_METHOD)
        .bind(claimed_at)
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
        .bind(account_id.map(AccountId::as_str))
        .bind(message_id.map(MessageId::as_str))
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

    /// The backfill keeps only the handler's success summaries, for messages
    /// that still exist. A subject can contain any words, so matching on
    /// them would create receipts for mail mxr never unsubscribed from.
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
        let subject_only = MessageId::new();
        for id in [
            &plain,
            &upper,
            &wrong_category,
            &unrelated_summary,
            &earliest,
            &subject_only,
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
        // A mutation on a message whose subject contains the words. The
        // subject sits after the action verb, so the prefix never reaches it.
        insert_event(
            &store,
            240,
            "mutation",
            owner,
            Some(&subject_only),
            "Marked \"How to unsubscribe\" as read",
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
            (plain.as_str(), acct.clone(), "unknown".to_string(), 100),
            (upper.as_str(), acct.clone(), "unknown".to_string(), 110),
            (earliest.as_str(), acct.clone(), "unknown".to_string(), 200),
            (
                via_message.as_str(),
                acct.clone(),
                "unknown".to_string(),
                210,
            ),
        ];
        expected.sort();
        assert_eq!(receipts(&store).await, expected);
    }

    async fn fresh_message(store: &Store) -> (AccountId, MessageId) {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let id = MessageId::new();
        insert_message(store, &account.id, &id).await;
        (account.id, id)
    }

    #[tokio::test]
    async fn a_live_claim_blocks_others_and_confirmation_ends_them() {
        let store = Store::in_memory().await.unwrap();
        let (account, id) = fresh_message(&store).await;

        assert_eq!(
            store.claim_unsubscribe(&id, &account, 10).await.unwrap(),
            Claim::Claimed { claimed_at: 10 }
        );
        assert_eq!(
            store.claim_unsubscribe(&id, &account, 11).await.unwrap(),
            Claim::InProgress,
            "a second attempt inside the window must not be granted or confirmed"
        );

        store.confirm_unsubscribe(&id, "mailto", 20).await.unwrap();
        assert_eq!(
            store.claim_unsubscribe(&id, &account, 21).await.unwrap(),
            Claim::Confirmed
        );
        let rows = receipts(&store).await;
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].2.as_str(), rows[0].3), ("mailto", 20));
    }

    #[tokio::test]
    async fn a_pending_claim_older_than_ten_minutes_is_taken_over_once() {
        let store = Store::in_memory().await.unwrap();
        let (account, id) = fresh_message(&store).await;
        store.claim_unsubscribe(&id, &account, 1_000).await.unwrap();

        // Exactly 600 seconds old is not yet stale: the boundary is strict.
        assert_eq!(
            store
                .claim_unsubscribe(&id, &account, 1_000 + STALE_CLAIM_SECS)
                .await
                .unwrap(),
            Claim::InProgress
        );

        let later = 1_000 + STALE_CLAIM_SECS + 1;
        assert_eq!(
            store.claim_unsubscribe(&id, &account, later).await.unwrap(),
            Claim::Claimed { claimed_at: later },
            "a stale claim must be taken over"
        );
        assert_eq!(
            store
                .claim_unsubscribe(&id, &account, later + 1)
                .await
                .unwrap(),
            Claim::InProgress,
            "the takeover is itself a live claim"
        );
    }

    #[tokio::test]
    async fn releasing_a_claim_lets_a_retry_send() {
        let store = Store::in_memory().await.unwrap();
        let (account, id) = fresh_message(&store).await;

        let Claim::Claimed { claimed_at } =
            store.claim_unsubscribe(&id, &account, 1).await.unwrap()
        else {
            panic!("first claim must be granted");
        };
        store
            .release_unsubscribe_claim(&id, claimed_at)
            .await
            .unwrap();
        assert!(!store.has_unsubscribe_receipt(&id).await.unwrap());
        assert!(matches!(
            store.claim_unsubscribe(&id, &account, 2).await.unwrap(),
            Claim::Claimed { .. }
        ));
    }

    #[tokio::test]
    async fn a_stale_attempts_late_release_cannot_remove_the_takeover() {
        let store = Store::in_memory().await.unwrap();
        let (account, id) = fresh_message(&store).await;

        // The first attempt stalls holding its claim.
        let Claim::Claimed {
            claimed_at: stalled,
        } = store.claim_unsubscribe(&id, &account, 1).await.unwrap()
        else {
            panic!("first claim must be granted");
        };
        let later = 1 + STALE_CLAIM_SECS + 1;
        let Claim::Claimed {
            claimed_at: takeover,
        } = store.claim_unsubscribe(&id, &account, later).await.unwrap()
        else {
            panic!("stale claim must be taken over");
        };

        // The stalled attempt finally fails and releases with its own time.
        store.release_unsubscribe_claim(&id, stalled).await.unwrap();
        assert!(
            store.has_unsubscribe_receipt(&id).await.unwrap(),
            "a stale attempt's release must not remove the takeover"
        );

        store
            .release_unsubscribe_claim(&id, takeover)
            .await
            .unwrap();
        assert!(!store.has_unsubscribe_receipt(&id).await.unwrap());
    }

    #[tokio::test]
    async fn a_release_never_removes_a_confirmed_receipt() {
        let store = Store::in_memory().await.unwrap();
        let (account, id) = fresh_message(&store).await;
        let Claim::Claimed { claimed_at } =
            store.claim_unsubscribe(&id, &account, 1).await.unwrap()
        else {
            panic!("first claim must be granted");
        };
        store
            .confirm_unsubscribe(&id, "one_click", 3)
            .await
            .unwrap();
        store
            .release_unsubscribe_claim(&id, claimed_at)
            .await
            .unwrap();
        assert!(
            store.has_unsubscribe_receipt(&id).await.unwrap(),
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
