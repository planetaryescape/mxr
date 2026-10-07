//! The newest arrivals of one account, for freshness (`GetFreshness`):
//! "is the local copy current?" is answered by when the newest message
//! came in, whatever mailbox or mode it went to.

use crate::{decode_id, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageFlags};
use sqlx::Row;
use std::time::Instant;

/// One message that came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrival {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// `inbound` or `unknown`: outbound mail never arrives.
    pub direction: String,
    pub date: DateTime<Utc>,
    pub from: Address,
    pub subject: String,
}

impl super::Store {
    /// The newest messages of `account_id` that arrived rather than were
    /// sent, newest first, at most `limit`.
    ///
    /// Any mailbox counts, archived included, except Trash and Spam: mail
    /// there says nothing about whether new mail is reaching you, and the
    /// popover that lists these opens each one. Drafts are yours, not an
    /// arrival. A Date header more than a day ahead is a bad clock, not
    /// the newest mail, so it is skipped as the desk skips it.
    pub async fn latest_arrivals(
        &self,
        account_id: &AccountId,
        limit: u32,
    ) -> Result<Vec<Arrival>, sqlx::Error> {
        let started_at = Instant::now();
        let hidden_flags =
            i64::from((MessageFlags::TRASH | MessageFlags::SPAM | MessageFlags::DRAFT).bits());
        let future_cutoff = Utc::now().timestamp() + 86_400;
        // `idx_messages_account_date` walks the account newest first, so the
        // filters only see rows until `limit` of them pass.
        let rows = sqlx::query(
            "SELECT m.id, m.account_id, m.thread_id, m.direction, m.date,
                    m.from_name, m.from_email, m.subject
             FROM messages m
             WHERE m.account_id = ?1
               AND m.date <= ?2
               AND (m.flags & ?3) = 0
               AND m.direction != 'outbound'
               AND NOT EXISTS (
                   SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                   WHERE ml.message_id = m.id AND l.provider_id IN ('TRASH', 'SPAM', 'DRAFT')
               )
             ORDER BY m.date DESC, m.rowid DESC
             LIMIT ?4",
        )
        .bind(account_id.as_str())
        .bind(future_cutoff)
        .bind(hidden_flags)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        let arrivals = rows
            .into_iter()
            .map(|row| {
                Ok(Arrival {
                    id: decode_id(row.try_get::<&str, _>("id")?)?,
                    account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    direction: row.try_get("direction")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                    from: Address {
                        name: row.try_get("from_name")?,
                        email: row.try_get("from_email")?,
                    },
                    subject: row.try_get("subject")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("arrivals.latest", started_at, arrivals.len());
        Ok(arrivals)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;
    use crate::Store;
    use chrono::{Duration, Utc};
    use mxr_core::id::*;
    use mxr_core::types::{MessageDirection, MessageFlags};

    async fn put(
        store: &Store,
        account: &AccountId,
        from: &str,
        minutes_ago: i64,
        direction: MessageDirection,
        flags: MessageFlags,
    ) -> MessageId {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        envelope.provider_id = format!("p-{}", envelope.id);
        envelope.thread_id = ThreadId::new();
        envelope.from.email = from.into();
        envelope.date = Utc::now() - Duration::minutes(minutes_ago);
        envelope.flags = flags;
        store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .unwrap();
        envelope.id
    }

    #[tokio::test]
    async fn arrivals_are_received_mail_newest_first_skipping_trash_spam_and_sent() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let id = &account.id;
        let old = put(
            &store,
            id,
            "a@x.com",
            60,
            MessageDirection::Inbound,
            MessageFlags::empty(),
        )
        .await;
        let newest = put(
            &store,
            id,
            "b@x.com",
            2,
            MessageDirection::Unknown,
            MessageFlags::READ,
        )
        .await;
        put(
            &store,
            id,
            "me@x.com",
            1,
            MessageDirection::Outbound,
            MessageFlags::empty(),
        )
        .await;
        put(
            &store,
            id,
            "spam@x.com",
            0,
            MessageDirection::Inbound,
            MessageFlags::SPAM,
        )
        .await;
        put(
            &store,
            id,
            "gone@x.com",
            0,
            MessageDirection::Inbound,
            MessageFlags::TRASH,
        )
        .await;
        // A bad Date header a week ahead is not the newest mail.
        put(
            &store,
            id,
            "clock@x.com",
            -7 * 24 * 60,
            MessageDirection::Inbound,
            MessageFlags::empty(),
        )
        .await;

        let arrivals = store.latest_arrivals(id, 10).await.unwrap();
        let ids: Vec<_> = arrivals.iter().map(|a| a.id.clone()).collect();
        assert_eq!(ids, vec![newest, old]);
        assert_eq!(store.latest_arrivals(id, 1).await.unwrap().len(), 1);
    }
}
