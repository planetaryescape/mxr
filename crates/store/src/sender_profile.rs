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

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, trace_lookup};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;

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
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::super::Store;
    use mxr_core::id::AccountId;

    #[tokio::test]
    async fn get_sender_profile_returns_none_for_unknown_contact() {
        let store = Store::in_memory().await.unwrap();
        let got = store
            .get_sender_profile(&AccountId::new(), "nobody@example.com")
            .await
            .unwrap();
        assert!(got.is_none());
    }
}
