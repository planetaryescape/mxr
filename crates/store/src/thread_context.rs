//! Store reads behind the reader's thread context: which messages in a
//! thread are yours, and how much you and one person have written to each
//! other. Live queries rather than the `contacts` cache, so a thread opened
//! right after sync already counts its own messages.

use crate::{decode_id, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::MessageDirection;
use sqlx::Row;
use std::time::Instant;

/// Message counts and last contact between the account and one address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CounterpartyExchange {
    pub from_them: u32,
    pub from_you: u32,
    /// Latest message between you outside `exclude_thread`.
    pub last_elsewhere_at: Option<DateTime<Utc>>,
    /// Any of their mail carried a `List-Id`.
    pub list_sender: bool,
}

impl super::Store {
    /// Direction of each message in a thread, oldest first.
    pub async fn thread_message_directions(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<(MessageId, MessageDirection)>, sqlx::Error> {
        let started_at = Instant::now();
        let rows = sqlx::query(
            "SELECT id, direction FROM messages WHERE thread_id = ? ORDER BY date ASC, id ASC",
        )
        .bind(thread_id.as_str())
        .fetch_all(self.reader())
        .await?;
        trace_query("thread_context.directions", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                let id: MessageId = decode_id(row.try_get::<&str, _>("id")?)?;
                let direction = MessageDirection::from_db_str(row.try_get::<&str, _>("direction")?)
                    .unwrap_or(MessageDirection::Unknown);
                Ok((id, direction))
            })
            .collect()
    }

    /// How much you and `email` have written to each other in `account_id`.
    /// Counts come from the `contacts` aggregate, which matches addresses
    /// case-insensitively across all mail; an address it hasn't aggregated
    /// yet is counted live. The latest inbound message outside this thread
    /// is found through the `from_email` index, matching `from_variants`
    /// (the address as their mail spells it); the latest outbound one is
    /// found newest first and stops at the first match.
    pub async fn counterparty_exchange(
        &self,
        account_id: &AccountId,
        email: &str,
        from_variants: &[String],
        exclude_thread: &ThreadId,
    ) -> Result<CounterpartyExchange, sqlx::Error> {
        let started_at = Instant::now();
        let email = email.trim().to_ascii_lowercase();
        let variants = address_spellings(&email, from_variants);

        let inbound_query = sqlx::query(
            r#"SELECT COUNT(*) AS total,
                      MAX(CASE WHEN thread_id != ?3 THEN date END) AS last_elsewhere,
                      MAX(list_id IS NOT NULL) AS list_sender
               FROM messages INDEXED BY idx_messages_from
               WHERE from_email IN (SELECT value FROM json_each(?2))
                 AND account_id = ?1
                 AND direction = 'inbound'"#,
        )
        .bind(account_id.as_str())
        .bind(&variants)
        .bind(exclude_thread.as_str())
        .fetch_one(self.reader());
        let aggregate_query = sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT total_inbound, total_outbound, is_list_sender
             FROM contacts WHERE account_id = ? AND email = ?",
        )
        .bind(account_id.as_str())
        .bind(&email)
        .fetch_optional(self.reader());
        let (inbound, aggregate) = tokio::try_join!(inbound_query, aggregate_query)?;
        let (from_them, from_you, last_out) = match aggregate {
            Some((inbound_total, 0, _)) => {
                (u32::try_from(inbound_total).unwrap_or(u32::MAX), 0, None)
            }
            Some((inbound_total, total, _)) => (
                u32::try_from(inbound_total).unwrap_or(u32::MAX),
                u32::try_from(total).unwrap_or(u32::MAX),
                self.latest_outbound_elsewhere(account_id, &email, exclude_thread)
                    .await?,
            ),
            // Not aggregated yet (a brand-new address): count it live.
            None => {
                let sql = format!(
                    "SELECT COUNT(*) AS total,
                            MAX(CASE WHEN thread_id != ?3 THEN date END) AS last_elsewhere
                     FROM messages m
                     WHERE m.account_id = ?1 AND m.direction = 'outbound' AND {SENT_TO}"
                );
                let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(account_id.as_str())
                    .bind(&email)
                    .bind(exclude_thread.as_str())
                    .fetch_one(self.reader())
                    .await?;
                (
                    count(&inbound)?,
                    count(&row)?,
                    row.try_get::<Option<i64>, _>("last_elsewhere")?,
                )
            }
        };
        trace_query("thread_context.exchange", started_at, 3);

        let last_in: Option<i64> = inbound.try_get("last_elsewhere")?;
        let last_elsewhere_at = last_in
            .into_iter()
            .chain(last_out)
            .max()
            .map(decode_timestamp)
            .transpose()?;
        let list_sender = aggregate.is_some_and(|(_, _, list)| list > 0)
            || inbound
                .try_get::<Option<i64>, _>("list_sender")?
                .unwrap_or(0)
                > 0;
        Ok(CounterpartyExchange {
            from_them,
            from_you,
            last_elsewhere_at,
            list_sender,
        })
    }

    /// Whether any of these messages has a synced body. One query, no body
    /// columns read.
    pub async fn any_body_synced(&self, message_ids: &[MessageId]) -> Result<bool, sqlx::Error> {
        let ids = serde_json::to_string(
            &message_ids
                .iter()
                .map(MessageId::as_str)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".into());
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM bodies WHERE message_id IN (SELECT value FROM json_each(?)))",
        )
        .bind(ids)
        .fetch_one(self.reader())
        .await
    }

    /// How many reply pairs there are with `email` in `direction`, and the
    /// median latency, without loading them: the reader only needs the
    /// middle value.
    pub async fn reply_latency_median(
        &self,
        account_id: &AccountId,
        direction: mxr_core::types::ResponseTimeDirection,
        email: &str,
        spellings: &[String],
    ) -> Result<(u32, Option<u32>), sqlx::Error> {
        let started_at = Instant::now();
        // Reply pairs keep the address as the mail spelled it; match every
        // known spelling exactly (plus lowercase) so the party index applies.
        let spellings = address_spellings(email, spellings);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reply_pairs INDEXED BY idx_reply_pairs_party
             WHERE counterparty_email IN (SELECT value FROM json_each(?1))
               AND account_id = ?2 AND direction = ?3",
        )
        .bind(&spellings)
        .bind(account_id.as_str())
        .bind(direction.as_db_str())
        .fetch_one(self.reader())
        .await?;
        let median: Option<i64> = if count == 0 {
            None
        } else {
            sqlx::query_scalar(
                "SELECT latency_seconds FROM reply_pairs INDEXED BY idx_reply_pairs_party
                 WHERE counterparty_email IN (SELECT value FROM json_each(?1))
                   AND account_id = ?2 AND direction = ?3
                 ORDER BY latency_seconds
                 LIMIT 1 OFFSET ?4",
            )
            .bind(&spellings)
            .bind(account_id.as_str())
            .bind(direction.as_db_str())
            .bind(count / 2)
            .fetch_optional(self.reader())
            .await?
        };
        trace_query("thread_context.reply_median", started_at, 2);
        Ok((
            u32::try_from(count).unwrap_or(u32::MAX),
            median.map(|seconds| u32::try_from(seconds.max(0)).unwrap_or(u32::MAX)),
        ))
    }

    async fn latest_outbound_elsewhere(
        &self,
        account_id: &AccountId,
        email: &str,
        exclude_thread: &ThreadId,
    ) -> Result<Option<i64>, sqlx::Error> {
        let sql = format!(
            "SELECT m.date FROM messages m
             WHERE m.account_id = ?1 AND m.direction = 'outbound' AND m.thread_id != ?3
               AND {SENT_TO}
             ORDER BY m.date DESC
             LIMIT 1"
        );
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(account_id.as_str())
            .bind(email)
            .bind(exclude_thread.as_str())
            .fetch_optional(self.reader())
            .await
    }
}

/// The JSON array of spellings to match exactly: those given plus the
/// lowercase form, deduplicated.
fn address_spellings(email: &str, spellings: &[String]) -> String {
    let lower = email.trim().to_ascii_lowercase();
    let mut all: Vec<&str> = spellings.iter().map(String::as_str).collect();
    all.push(&lower);
    all.sort_unstable();
    all.dedup();
    serde_json::to_string(&all).unwrap_or_else(|_| "[]".into())
}

/// `m` was sent to `?2` (lowercased) on To, Cc or Bcc.
const SENT_TO: &str = "EXISTS (
    SELECT 1 FROM json_each(m.to_addrs) WHERE LOWER(json_extract(value, '$.email')) = ?2
    UNION ALL
    SELECT 1 FROM json_each(m.cc_addrs) WHERE LOWER(json_extract(value, '$.email')) = ?2
    UNION ALL
    SELECT 1 FROM json_each(m.bcc_addrs) WHERE LOWER(json_extract(value, '$.email')) = ?2
)";

fn count(row: &sqlx::sqlite::SqliteRow) -> Result<u32, sqlx::Error> {
    let total: i64 = row.try_get("total")?;
    Ok(u32::try_from(total).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use mxr_core::id::{AccountId, ThreadId};
    use mxr_core::types::{Address, MessageDirection};

    async fn store_with_account() -> (Store, AccountId) {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        (store, account.id)
    }

    fn address(email: &str) -> Address {
        Address {
            name: None,
            email: email.into(),
        }
    }

    async fn put(
        store: &Store,
        account_id: &AccountId,
        thread: &ThreadId,
        from: &str,
        to: &str,
        days_ago: i64,
        direction: MessageDirection,
    ) -> mxr_core::id::MessageId {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .build();
        envelope.provider_id = format!("p-{}", envelope.id);
        envelope.thread_id = thread.clone();
        envelope.from = address(from);
        envelope.to = vec![address(to)];
        envelope.date = chrono::Utc::now() - chrono::Duration::days(days_ago);
        store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .unwrap();
        envelope.id
    }

    #[tokio::test]
    async fn exchange_counts_both_directions_and_finds_contact_elsewhere() {
        let (store, account_id) = store_with_account().await;
        let here = ThreadId::new();
        let earlier = ThreadId::new();
        use MessageDirection::{Inbound, Outbound};
        put(
            &store,
            &account_id,
            &earlier,
            "Maya@Example.com",
            "me@example.com",
            30,
            Inbound,
        )
        .await;
        put(
            &store,
            &account_id,
            &earlier,
            "me@example.com",
            "maya@example.com",
            29,
            Outbound,
        )
        .await;
        put(
            &store,
            &account_id,
            &here,
            "maya@example.com",
            "me@example.com",
            1,
            Inbound,
        )
        .await;

        let exchange = store
            .counterparty_exchange(
                &account_id,
                "MAYA@example.com",
                &["Maya@Example.com".to_string()],
                &here,
            )
            .await
            .unwrap();
        assert_eq!(exchange.from_them, 2);
        assert_eq!(exchange.from_you, 1);
        let last = exchange
            .last_elsewhere_at
            .expect("the earlier thread counts");
        let days_ago = (chrono::Utc::now() - last).num_days();
        assert_eq!(days_ago, 29, "latest message outside this thread");
        assert!(!exchange.list_sender);

        let directions = store.thread_message_directions(&earlier).await.unwrap();
        assert_eq!(
            directions.iter().map(|(_, d)| *d).collect::<Vec<_>>(),
            vec![Inbound, Outbound]
        );
    }

    #[tokio::test]
    async fn first_conversation_has_no_contact_elsewhere() {
        let (store, account_id) = store_with_account().await;
        let here = ThreadId::new();
        put(
            &store,
            &account_id,
            &here,
            "new@example.com",
            "me@example.com",
            0,
            MessageDirection::Inbound,
        )
        .await;
        let exchange = store
            .counterparty_exchange(&account_id, "new@example.com", &[], &here)
            .await
            .unwrap();
        assert_eq!(exchange.from_them, 1);
        assert_eq!(exchange.from_you, 0);
        assert_eq!(exchange.last_elsewhere_at, None);
    }

    #[tokio::test]
    async fn aggregated_contacts_give_the_same_counts_and_medians_skip_the_rows() {
        let (store, account_id) = store_with_account().await;
        let here = ThreadId::new();
        let earlier = ThreadId::new();
        use MessageDirection::{Inbound, Outbound};
        put(
            &store,
            &account_id,
            &earlier,
            "maya@example.com",
            "me@example.com",
            30,
            Inbound,
        )
        .await;
        put(
            &store,
            &account_id,
            &earlier,
            "me@example.com",
            "maya@example.com",
            29,
            Outbound,
        )
        .await;
        put(
            &store,
            &account_id,
            &here,
            "me@example.com",
            "maya@example.com",
            2,
            Outbound,
        )
        .await;
        put(
            &store,
            &account_id,
            &here,
            "maya@example.com",
            "me@example.com",
            1,
            Inbound,
        )
        .await;
        store.refresh_contacts().await.unwrap();

        let exchange = store
            .counterparty_exchange(&account_id, "maya@example.com", &[], &here)
            .await
            .unwrap();
        assert_eq!((exchange.from_them, exchange.from_you), (2, 2));
        let days_ago = (chrono::Utc::now() - exchange.last_elsewhere_at.unwrap()).num_days();
        assert_eq!(days_ago, 29, "the newest outbound outside this thread");

        let (count, median) = store
            .reply_latency_median(
                &account_id,
                mxr_core::types::ResponseTimeDirection::IReplied,
                "maya@example.com",
                &[],
            )
            .await
            .unwrap();
        assert_eq!((count, median), (0, None));

        // Three replies of 10s, 30s and 20s: the middle one is 20s.
        for latency in [10_i64, 30, 20] {
            let parent = put(
                &store,
                &account_id,
                &here,
                "maya@example.com",
                "me@example.com",
                5,
                Inbound,
            )
            .await;
            let reply = put(
                &store,
                &account_id,
                &here,
                "me@example.com",
                "maya@example.com",
                4,
                Outbound,
            )
            .await;
            sqlx::query(
                "INSERT INTO reply_pairs (reply_message_id, parent_message_id, account_id,
                     counterparty_email, direction, parent_received_at, replied_at,
                     latency_seconds, created_at)
                 VALUES (?, ?, ?, 'Maya@Example.com', 'i_replied', 0, ?, ?, 0)",
            )
            .bind(reply.as_str())
            .bind(parent.as_str())
            .bind(account_id.as_str())
            .bind(latency)
            .bind(latency)
            .execute(store.writer())
            .await
            .unwrap();
        }
        let (count, median) = store
            .reply_latency_median(
                &account_id,
                mxr_core::types::ResponseTimeDirection::IReplied,
                "maya@example.com",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(
            (count, median),
            (0, None),
            "stored as Maya@Example.com: lowercase alone misses it"
        );
        // Given the spelling the thread uses, the stored pairs are found.
        let (count, median) = store
            .reply_latency_median(
                &account_id,
                mxr_core::types::ResponseTimeDirection::IReplied,
                "maya@example.com",
                &["Maya@Example.com".to_string()],
            )
            .await
            .unwrap();
        assert_eq!((count, median), (3, Some(20)));
    }
}
