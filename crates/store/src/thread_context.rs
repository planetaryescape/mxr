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
    /// Inbound counts their messages (by From); outbound counts yours with
    /// them on To, Cc or Bcc.
    pub async fn counterparty_exchange(
        &self,
        account_id: &AccountId,
        email: &str,
        exclude_thread: &ThreadId,
    ) -> Result<CounterpartyExchange, sqlx::Error> {
        let started_at = Instant::now();
        let email = email.trim().to_ascii_lowercase();
        let inbound = sqlx::query(
            r#"SELECT COUNT(*) AS total,
                      MAX(CASE WHEN thread_id != ?3 THEN date END) AS last_elsewhere,
                      MAX(list_id IS NOT NULL) AS list_sender
               FROM messages
               WHERE account_id = ?1
                 AND direction = 'inbound'
                 AND LOWER(from_email) = ?2"#,
        )
        .bind(account_id.as_str())
        .bind(&email)
        .bind(exclude_thread.as_str())
        .fetch_one(self.reader())
        .await?;
        let outbound = sqlx::query(
            r#"SELECT COUNT(*) AS total,
                      MAX(CASE WHEN thread_id != ?3 THEN date END) AS last_elsewhere
               FROM messages m
               WHERE m.account_id = ?1
                 AND m.direction = 'outbound'
                 AND EXISTS (
                     SELECT 1 FROM json_each(m.to_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = ?2
                     UNION ALL
                     SELECT 1 FROM json_each(m.cc_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = ?2
                     UNION ALL
                     SELECT 1 FROM json_each(m.bcc_addrs)
                     WHERE LOWER(json_extract(value, '$.email')) = ?2
                 )"#,
        )
        .bind(account_id.as_str())
        .bind(&email)
        .bind(exclude_thread.as_str())
        .fetch_one(self.reader())
        .await?;
        trace_query("thread_context.exchange", started_at, 2);

        let last_in: Option<i64> = inbound.try_get("last_elsewhere")?;
        let last_out: Option<i64> = outbound.try_get("last_elsewhere")?;
        let last_elsewhere_at = last_in
            .into_iter()
            .chain(last_out)
            .max()
            .map(decode_timestamp)
            .transpose()?;
        Ok(CounterpartyExchange {
            from_them: count(&inbound)?,
            from_you: count(&outbound)?,
            last_elsewhere_at,
            list_sender: inbound
                .try_get::<Option<i64>, _>("list_sender")?
                .unwrap_or(0)
                > 0,
        })
    }
}

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
    ) {
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
            .counterparty_exchange(&account_id, "MAYA@example.com", &here)
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
            .counterparty_exchange(&account_id, "new@example.com", &here)
            .await
            .unwrap();
        assert_eq!(exchange.from_them, 1);
        assert_eq!(exchange.from_you, 0);
        assert_eq!(exchange.last_elsewhere_at, None);
    }
}
