//! Per-sender relationship aggregates: a single read joining the
//! `contacts` materialized view with on-the-fly counts that don't live
//! in `contacts` (open threads, reply-later flags from this sender).
//!
//! Lightweight v1 — surfaces the stats users care about glancing at
//! when deciding "do I need to deal with this person right now?":
//!
//!   * Volume (in/out).
//!   * Last seen in either direction.
//!   * Median cadence (when refresh has computed it).
//!   * Replied count (proxy for "do I usually engage?").
//!   * Open thread count: threads whose latest message is from this
//!     sender and has no outbound reply yet.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_lookup, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq)]
pub struct SenderProfile {
    pub account_id: AccountId,
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_inbound_at: Option<DateTime<Utc>>,
    pub last_outbound_at: Option<DateTime<Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    pub cadence_days_p50: Option<f64>,
    pub is_list_sender: bool,
    pub list_id: Option<String>,
    pub open_thread_count: u32,
    pub inbound_storage_bytes: u64,
    pub outbound_storage_bytes: u64,
    pub attachment_count: u32,
    pub attachment_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SenderEmailReference {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub date: DateTime<Utc>,
    pub direction: String,
    pub has_attachments: bool,
}

impl super::Store {
    /// Look up the per-sender profile for an (account, email) pair.
    /// Returns `None` if the contact is unknown.
    pub async fn get_sender_profile(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<Option<SenderProfile>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT account_id as "account_id!", email as "email!",
                      display_name, first_seen_at as "first_seen_at!",
                      last_seen_at as "last_seen_at!",
                      last_inbound_at, last_outbound_at,
                      total_inbound as "total_inbound!",
                      total_outbound as "total_outbound!",
                      replied_count as "replied_count!",
                      cadence_days_p50,
                      is_list_sender as "is_list_sender!",
                      list_id
               FROM contacts
               WHERE account_id = ? AND email = ? COLLATE NOCASE"#,
            aid,
            email,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("sender_profile.get", started_at, row.is_some());

        let Some(r) = row else { return Ok(None) };

        // "Open threads": threads where the latest envelope is from this
        // sender and there's no outbound message in the same thread that's
        // chronologically after it. Approximated by checking each thread
        // whose most-recent envelope is from `email`.
        let open = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!: i64"
               FROM (
                   SELECT thread_id, MAX(date) AS latest_date
                   FROM messages
                   WHERE account_id = ?
                   GROUP BY thread_id
               ) t
               JOIN messages m
                 ON m.account_id = ? AND m.thread_id = t.thread_id AND m.date = t.latest_date
               WHERE LOWER(m.from_email) = LOWER(?)
                 AND NOT EXISTS (
                     SELECT 1 FROM messages om
                     WHERE om.account_id = ? AND om.thread_id = m.thread_id
                       AND om.direction = 'outbound' AND om.date >= m.date
                 )"#,
            aid,
            aid,
            email,
            aid,
        )
        .fetch_one(self.reader())
        .await?;

        let (inbound_storage_bytes, outbound_storage_bytes, attachment_count, attachment_bytes): (
            i64,
            i64,
            i64,
            i64,
        ) = sqlx::query_as(
            r#"WITH matched AS (
                   SELECT
                     m.id,
                     CASE WHEN LOWER(m.from_email) = LOWER(?2) THEN m.size_bytes ELSE 0 END
                       AS inbound_storage_bytes,
                     CASE
                       WHEN m.direction = 'outbound' AND (
                         EXISTS (
                           SELECT 1 FROM json_each(m.to_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                         OR EXISTS (
                           SELECT 1 FROM json_each(m.cc_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                         OR EXISTS (
                           SELECT 1 FROM json_each(m.bcc_addrs)
                           WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                         )
                       )
                       THEN m.size_bytes
                       ELSE 0
                     END AS outbound_storage_bytes
                   FROM messages m
                   WHERE m.account_id = ?1
                     AND (
                       LOWER(m.from_email) = LOWER(?2)
                       OR (
                         m.direction = 'outbound'
                         AND (
                           EXISTS (
                             SELECT 1 FROM json_each(m.to_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                           OR EXISTS (
                             SELECT 1 FROM json_each(m.cc_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                           OR EXISTS (
                             SELECT 1 FROM json_each(m.bcc_addrs)
                             WHERE LOWER(json_extract(value, '$.email')) = LOWER(?2)
                           )
                         )
                       )
                     )
                 )
                 SELECT
                   COALESCE((SELECT SUM(inbound_storage_bytes) FROM matched), 0),
                   COALESCE((SELECT SUM(outbound_storage_bytes) FROM matched), 0),
                   COALESCE((SELECT COUNT(*) FROM attachments a JOIN matched ON matched.id = a.message_id), 0),
                   COALESCE((SELECT SUM(a.size_bytes) FROM attachments a JOIN matched ON matched.id = a.message_id), 0)"#,
        )
        .bind(aid)
        .bind(email)
        .fetch_one(self.reader())
        .await?;

        Ok(Some(SenderProfile {
            account_id: decode_id(&r.account_id)?,
            email: r.email,
            display_name: r.display_name,
            first_seen_at: decode_timestamp(r.first_seen_at)?,
            last_seen_at: decode_timestamp(r.last_seen_at)?,
            last_inbound_at: decode_optional_timestamp(r.last_inbound_at)?,
            last_outbound_at: decode_optional_timestamp(r.last_outbound_at)?,
            total_inbound: r.total_inbound as u32,
            total_outbound: r.total_outbound as u32,
            replied_count: r.replied_count as u32,
            cadence_days_p50: r.cadence_days_p50,
            is_list_sender: r.is_list_sender != 0,
            list_id: r.list_id,
            open_thread_count: open as u32,
            inbound_storage_bytes: inbound_storage_bytes.max(0) as u64,
            outbound_storage_bytes: outbound_storage_bytes.max(0) as u64,
            attachment_count: attachment_count.max(0) as u32,
            attachment_bytes: attachment_bytes.max(0) as u64,
        }))
    }

    pub async fn list_recent_sender_messages(
        &self,
        account_id: &AccountId,
        email: &str,
        limit: u32,
    ) -> Result<Vec<SenderEmailReference>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let limit = i64::from(limit.clamp(1, 25));
        let rows = sqlx::query(
            r#"SELECT id, thread_id, subject, snippet, from_name, from_email,
                      date, direction, has_attachments
               FROM messages
               WHERE account_id = ?1
                 AND LOWER(from_email) = LOWER(?2)
               ORDER BY date DESC
               LIMIT ?3"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .bind(limit)
        .fetch_all(self.reader())
        .await?;
        trace_query("sender_profile.recent_messages", started_at, rows.len());

        rows.into_iter()
            .map(|row| {
                Ok(SenderEmailReference {
                    message_id: decode_id(row.get::<String, _>("id").as_str())?,
                    thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
                    subject: row.get::<String, _>("subject"),
                    snippet: row.get::<String, _>("snippet"),
                    from_name: row.get::<Option<String>, _>("from_name"),
                    from_email: row.get::<String, _>("from_email"),
                    date: decode_timestamp(row.get::<i64, _>("date"))?,
                    direction: row.get::<String, _>("direction"),
                    has_attachments: row.get::<bool, _>("has_attachments"),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::Store;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use chrono::{TimeZone, Utc};
    use mxr_core::id::AccountId;
    use mxr_core::{Address, MessageDirection};

    #[tokio::test]
    async fn get_sender_profile_returns_none_for_unknown_contact() {
        let store = Store::in_memory().await.unwrap();
        let got = store
            .get_sender_profile(&AccountId::new(), "nobody@example.com")
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn list_recent_sender_messages_returns_latest_first() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let mut older = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        older.provider_id = "older".into();
        older.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        older.subject = "Older note".into();
        older.snippet = "The first note".into();
        older.date = Utc.with_ymd_and_hms(2026, 5, 10, 9, 0, 0).unwrap();
        older.has_attachments = true;

        let mut newer = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        newer.provider_id = "newer".into();
        newer.from = Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        newer.subject = "Newer note".into();
        newer.snippet = "The latest note".into();
        newer.date = Utc.with_ymd_and_hms(2026, 5, 11, 9, 0, 0).unwrap();

        let mut other = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        other.provider_id = "other".into();
        other.from = Address {
            name: Some("Bob".into()),
            email: "bob@example.com".into(),
        };
        other.subject = "Different sender".into();
        other.date = Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap();

        store
            .upsert_envelope_with_direction(&older, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&newer, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&other, MessageDirection::Inbound)
            .await
            .unwrap();

        let messages = store
            .list_recent_sender_messages(&account.id, "ALICE@example.com", 10)
            .await
            .unwrap();

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].subject, "Newer note");
        assert_eq!(messages[1].subject, "Older note");
        assert!(messages[1].has_attachments);
    }
}
