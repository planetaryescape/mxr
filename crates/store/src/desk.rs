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
use std::collections::HashMap;
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

    /// For each `(email, around)`, the newest non-trashed message from or to
    /// that address dated within a day of `around` (the contact's known last
    /// exchange). One query for every contact; the date bound keeps each
    /// lookup to a handful of rows instead of a walk over the mailbox.
    pub async fn desk_latest_exchanges(
        &self,
        account_id: &AccountId,
        wanted: &[(String, DateTime<Utc>)],
    ) -> Result<HashMap<String, DeskLatestExchange>, sqlx::Error> {
        if wanted.is_empty() {
            return Ok(HashMap::new());
        }
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        let windows: Vec<serde_json::Value> = wanted
            .iter()
            .map(|(email, around)| {
                serde_json::json!({
                    "email": email.to_ascii_lowercase(),
                    "lo": around.timestamp() - 86_400,
                    "hi": around.timestamp() + 86_400,
                })
            })
            .collect();
        let windows =
            serde_json::to_string(&windows).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"WITH wanted AS (
                SELECT json_extract(value, '$.email') AS email,
                       json_extract(value, '$.lo') AS lo,
                       json_extract(value, '$.hi') AS hi
                FROM json_each(?2)
            )
            SELECT wanted.email AS email, m.id, m.thread_id, m.subject, m.date
            FROM wanted
            JOIN messages m
              ON m.account_id = ?1
             AND m.date BETWEEN wanted.lo AND wanted.hi
             AND (m.flags & ?3) = 0
             AND (
                 LOWER(m.from_email) = wanted.email
                 OR EXISTS (
                     SELECT 1 FROM json_each(m.to_addrs) recipient
                     WHERE LOWER(json_extract(recipient.value, '$.email')) = wanted.email
                 )
             )
            ORDER BY m.date DESC, m.id DESC"#,
        )
        .bind(account_id.as_str())
        .bind(windows)
        .bind(hidden_flags)
        .fetch_all(self.reader())
        .await?;
        let mut latest = HashMap::new();
        for row in rows {
            let email: String = row.try_get("email")?;
            if latest.contains_key(&email) {
                continue;
            }
            latest.insert(
                email,
                DeskLatestExchange {
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    message_id: decode_id(row.try_get::<&str, _>("id")?)?,
                    subject: row.try_get("subject")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                },
            );
        }
        trace_query("desk.latest_exchanges", started_at, latest.len());
        Ok(latest)
    }

    /// Threads marked "done waiting", with the date of the newest message
    /// they covered. A thread whose newest message is later is back.
    pub async fn desk_dismissals(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<ThreadId, DateTime<Utc>>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT thread_id, through_date FROM desk_dismissals WHERE account_id = ?1",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    decode_timestamp(row.try_get("through_date")?)?,
                ))
            })
            .collect()
    }

    /// Mark threads "done waiting" through their newest message. Threads
    /// with no stored message are skipped. With `dry_run`, nothing is
    /// written and the same selection is returned, so a preview matches.
    pub async fn dismiss_desk_threads(
        &self,
        thread_ids: &[ThreadId],
        dry_run: bool,
    ) -> Result<Vec<(AccountId, ThreadId)>, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        // One selection for the preview and the write, so they agree.
        const SELECTION: &str = r#"SELECT account_id, thread_id, MAX(date) AS through_date
               FROM messages
               WHERE thread_id IN (SELECT value FROM json_each(?1))
               GROUP BY account_id, thread_id"#;
        let rows = if dry_run {
            sqlx::query(SELECTION)
                .bind(&wanted)
                .fetch_all(self.reader())
                .await?
        } else {
            let sql = format!(
                "INSERT INTO desk_dismissals (account_id, thread_id, through_date, dismissed_at)
                 SELECT account_id, thread_id, through_date, ?2 FROM ({SELECTION}) WHERE true
                 ON CONFLICT(account_id, thread_id) DO UPDATE SET
                     through_date = excluded.through_date,
                     dismissed_at = excluded.dismissed_at
                 RETURNING account_id, thread_id"
            );
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(&wanted)
                .bind(Utc::now().timestamp())
                .fetch_all(self.writer())
                .await?
        };
        let mut selected = rows
            .into_iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("account_id")?)?,
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                ))
            })
            .collect::<Result<Vec<(AccountId, ThreadId)>, sqlx::Error>>()?;
        selected.sort_by_key(|(account, thread)| (account.as_str(), thread.as_str()));
        Ok(selected)
    }

    /// Undo "done waiting": the threads show under Waiting again.
    pub async fn restore_desk_threads(&self, thread_ids: &[ThreadId]) -> Result<u64, sqlx::Error> {
        if thread_ids.is_empty() {
            return Ok(0);
        }
        let wanted =
            serde_json::to_string(&thread_ids.iter().map(ThreadId::as_str).collect::<Vec<_>>())
                .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let result = sqlx::query(
            "DELETE FROM desk_dismissals WHERE thread_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(wanted)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
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

    #[tokio::test]
    async fn dismissals_cover_the_thread_through_its_newest_message() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        let thread = ThreadId::new();
        let sent = envelope(
            &account.id,
            &thread,
            "me@example.com",
            now - Duration::days(2),
        );
        store
            .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
            .await
            .unwrap();

        let preview = store
            .dismiss_desk_threads(std::slice::from_ref(&thread), true)
            .await
            .unwrap();
        assert_eq!(preview, vec![(account.id.clone(), thread.clone())]);
        assert!(store.desk_dismissals(&account.id).await.unwrap().is_empty());

        let done = store
            .dismiss_desk_threads(&[thread.clone(), ThreadId::new()], false)
            .await
            .unwrap();
        assert_eq!(done, preview, "unknown threads are skipped");
        let dismissals = store.desk_dismissals(&account.id).await.unwrap();
        assert_eq!(
            dismissals.get(&thread).map(DateTime::timestamp),
            Some(sent.date.timestamp())
        );

        // Dismissing again is an upsert, and restoring clears it.
        store
            .dismiss_desk_threads(std::slice::from_ref(&thread), false)
            .await
            .unwrap();
        assert_eq!(
            store
                .restore_desk_threads(std::slice::from_ref(&thread))
                .await
                .unwrap(),
            1
        );
        assert!(store.desk_dismissals(&account.id).await.unwrap().is_empty());
    }
}
