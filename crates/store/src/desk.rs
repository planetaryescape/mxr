//! Raw reads behind the desk (`Request::GetDesk`).
//!
//! The desk asks "what needs me?" instead of "what arrived?". The daemon
//! decides lanes; this module only fetches what that decision needs, in a
//! handful of windowed queries so the whole desk stays a fast local read:
//!
//! * every message of every thread that saw activity inside the window,
//!   with its inbox/trash/snooze/invite/delivery state;
//! * the contact rows and reply latencies for the counterparties involved.

use crate::{decode_id, decode_json, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};
use sqlx::Row;
use std::time::Instant;

/// One message of a thread that was active inside the desk window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskMessage {
    pub id: MessageId,
    pub thread_id: ThreadId,
    /// `inbound`, `outbound` or `unknown`, as stored.
    pub direction: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub list_id: Option<String>,
    pub unsubscribe: UnsubscribeMethod,
    pub in_inbox: bool,
    /// In Trash or Spam, by label or flag.
    pub trashed: bool,
    pub snoozed: bool,
    pub is_invite: bool,
    pub is_delivery: bool,
}

/// The contact facts the desk needs about a counterparty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskContact {
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub is_list_sender: bool,
}

/// One past reply with a counterparty (`reply_pairs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskReplyLatency {
    /// Lowercased counterparty address.
    pub email: String,
    /// `i_replied` or `they_replied`.
    pub direction: String,
    pub latency_seconds: i64,
}

/// The newest message exchanged with one address, for rows that start from
/// a person rather than a thread (cadence drift).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskLatestExchange {
    pub thread_id: ThreadId,
    pub message_id: MessageId,
    pub subject: String,
    pub date: DateTime<Utc>,
}

impl super::Store {
    /// Every message of every thread in `account_id` that has a message
    /// dated at or after `since`, ordered by thread then date.
    ///
    /// Threads are picked by recent activity but returned whole, so the
    /// caller sees the full conversation (who wrote last, every message id
    /// a verb must cover) and not just the recent tail.
    pub async fn desk_thread_messages(
        &self,
        account_id: &AccountId,
        since: DateTime<Utc>,
    ) -> Result<Vec<DeskMessage>, sqlx::Error> {
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        // Messages dated in the future (bad Date: headers) would otherwise
        // pin a thread to the top of every lane forever.
        let future_cutoff = Utc::now().timestamp() + 86_400;
        let rows = sqlx::query(
            r#"WITH active AS (
                SELECT DISTINCT thread_id
                FROM messages
                WHERE account_id = ?1 AND date >= ?2 AND date <= ?3
            )
            SELECT
                m.id, m.thread_id, m.direction, m.date, m.flags,
                m.from_email, m.from_name, m.to_addrs, m.cc_addrs, m.subject,
                m.list_id, m.unsubscribe_method,
                EXISTS (
                    SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                    WHERE ml.message_id = m.id AND l.provider_id = 'INBOX'
                ) AS in_inbox,
                ((m.flags & ?4) != 0 OR EXISTS (
                    SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                    WHERE ml.message_id = m.id AND l.provider_id IN ('TRASH', 'SPAM')
                )) AS trashed,
                EXISTS (SELECT 1 FROM snoozed s WHERE s.message_id = m.id) AS snoozed,
                EXISTS (SELECT 1 FROM calendar_invites ci WHERE ci.message_id = m.id) AS is_invite,
                EXISTS (SELECT 1 FROM delivery_messages dm WHERE dm.message_id = m.id) AS is_delivery
            FROM messages m
            JOIN active ON active.thread_id = m.thread_id
            WHERE m.account_id = ?1
            ORDER BY m.thread_id, m.date, m.id"#,
        )
        .bind(account_id.as_str())
        .bind(since.timestamp())
        .bind(future_cutoff)
        .bind(hidden_flags)
        .fetch_all(self.reader())
        .await?;

        let messages = rows
            .into_iter()
            .map(|row| {
                let unsubscribe = row
                    .try_get::<Option<String>, _>("unsubscribe_method")?
                    .as_deref()
                    .map(decode_json::<UnsubscribeMethod>)
                    .transpose()?
                    .unwrap_or(UnsubscribeMethod::None);
                Ok(DeskMessage {
                    id: decode_id(row.try_get::<&str, _>("id")?)?,
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    direction: row.try_get("direction")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                    flags: MessageFlags::from_bits_truncate(row.try_get::<i64, _>("flags")? as u32),
                    from: Address {
                        name: row.try_get("from_name")?,
                        email: row.try_get("from_email")?,
                    },
                    to: decode_json(row.try_get::<&str, _>("to_addrs")?)?,
                    cc: decode_json(row.try_get::<&str, _>("cc_addrs")?)?,
                    subject: row.try_get("subject")?,
                    list_id: row.try_get("list_id")?,
                    unsubscribe,
                    in_inbox: row.try_get("in_inbox")?,
                    trashed: row.try_get("trashed")?,
                    snoozed: row.try_get("snoozed")?,
                    is_invite: row.try_get("is_invite")?,
                    is_delivery: row.try_get("is_delivery")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("desk.thread_messages", started_at, messages.len());
        Ok(messages)
    }

    /// Contact rows for `emails` (matched case-insensitively).
    pub async fn desk_contacts(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<Vec<DeskContact>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT email, display_name, first_seen_at, total_inbound, total_outbound,
                      is_list_sender
               FROM contacts
               WHERE account_id = ?1
                 AND email IN (SELECT LOWER(value) FROM json_each(?2))"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let contacts = rows
            .into_iter()
            .map(|row| {
                Ok(DeskContact {
                    email: row.try_get("email")?,
                    display_name: row.try_get("display_name")?,
                    first_seen_at: decode_timestamp(row.try_get("first_seen_at")?)?,
                    total_inbound: row.try_get::<i64, _>("total_inbound")?.max(0) as u32,
                    total_outbound: row.try_get::<i64, _>("total_outbound")?.max(0) as u32,
                    is_list_sender: row.try_get::<i64, _>("is_list_sender")? != 0,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("desk.contacts", started_at, contacts.len());
        Ok(contacts)
    }

    /// Past reply latencies with `emails`, both directions, excluding pairs
    /// with the account's own addresses (as `list_response_time` does).
    pub async fn desk_reply_latencies(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<Vec<DeskReplyLatency>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT LOWER(counterparty_email) AS email, direction, latency_seconds
               FROM reply_pairs
               WHERE account_id = ?1
                 AND LOWER(counterparty_email) IN (SELECT LOWER(value) FROM json_each(?2))
                 AND latency_seconds >= 0"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let latencies = rows
            .into_iter()
            .map(|row| {
                Ok(DeskReplyLatency {
                    email: row.try_get("email")?,
                    direction: row.try_get("direction")?,
                    latency_seconds: row.try_get("latency_seconds")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("desk.reply_latencies", started_at, latencies.len());
        Ok(latencies)
    }

    /// The newest non-trashed message from or to `email` in `account_id`
    /// dated within a day of `around` (the contact's known last exchange),
    /// so the lookup reads a handful of rows instead of walking the mailbox.
    pub async fn desk_latest_exchange(
        &self,
        account_id: &AccountId,
        email: &str,
        around: DateTime<Utc>,
    ) -> Result<Option<DeskLatestExchange>, sqlx::Error> {
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        let row = sqlx::query(
            r#"SELECT m.id, m.thread_id, m.subject, m.date
               FROM messages m
               WHERE m.account_id = ?1
                 AND (m.flags & ?3) = 0
                 AND m.date BETWEEN ?4 AND ?5
                 AND (
                     LOWER(m.from_email) = LOWER(?2)
                     OR EXISTS (
                         SELECT 1 FROM json_each(m.to_addrs) recipient
                         WHERE LOWER(json_extract(recipient.value, '$.email')) = LOWER(?2)
                     )
                 )
               ORDER BY m.date DESC, m.id DESC
               LIMIT 1"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .bind(hidden_flags)
        .bind(around.timestamp() - 86_400)
        .bind(around.timestamp() + 86_400)
        .fetch_optional(self.reader())
        .await?;
        let exchange = row
            .map(|row| {
                Ok::<_, sqlx::Error>(DeskLatestExchange {
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    message_id: decode_id(row.try_get::<&str, _>("id")?)?,
                    subject: row.try_get("subject")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                })
            })
            .transpose()?;
        trace_query(
            "desk.latest_exchange",
            started_at,
            usize::from(exchange.is_some()),
        );
        Ok(exchange)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::*;
    use crate::Store;
    use chrono::Duration;
    use mxr_core::types::MessageDirection;

    fn envelope(
        account: &AccountId,
        thread: &ThreadId,
        from: &str,
        at: DateTime<Utc>,
    ) -> mxr_core::types::Envelope {
        let mut envelope = TestEnvelopeBuilder::new()
            .account_id(account.clone())
            .build();
        envelope.provider_id = format!("p-{}", envelope.id);
        envelope.thread_id = thread.clone();
        envelope.from.email = from.into();
        envelope.date = at;
        envelope
    }

    #[tokio::test]
    async fn thread_messages_return_whole_active_threads_with_state() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let now = Utc::now();
        let active_thread = ThreadId::new();
        let old = envelope(
            &account.id,
            &active_thread,
            "maya@example.com",
            now - Duration::days(90),
        );
        let recent = envelope(
            &account.id,
            &active_thread,
            "maya@example.com",
            now - Duration::hours(3),
        );
        let stale = envelope(
            &account.id,
            &ThreadId::new(),
            "jon@example.com",
            now - Duration::days(60),
        );
        for envelope in [&old, &recent, &stale] {
            store
                .upsert_envelope_with_direction(envelope, MessageDirection::Inbound)
                .await
                .unwrap();
        }

        let rows = store
            .desk_thread_messages(&account.id, now - Duration::days(30))
            .await
            .unwrap();
        // The old message comes with its recently active thread; the stale
        // thread stays out.
        assert_eq!(
            rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            vec![old.id.clone(), recent.id.clone()]
        );
        assert!(rows.iter().all(|row| row.direction == "inbound"));
        assert!(rows.iter().all(|row| !row.snoozed && !row.trashed));
    }
}
