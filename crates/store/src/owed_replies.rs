//! Slice 2.2 of docs/reference/ai-email.md
//!
//! "Owed reply" is a thread whose latest inbound message has not been
//! followed by an outbound message from the user. Ranked by
//! `overdue_score = waiting_days / expected_days`, where
//! `expected_days` is the recipient's `contacts.cadence_days_p50`,
//! falling back to the global median, then a 7-day default.
//!
//! Excludes:
//! * list senders (`contacts.is_list_sender`)
//! * screener-denied senders
//! * threads where the user has already replied after the latest
//!   inbound (no `latest_outbound_at > latest_inbound_at`)

use crate::{decode_id, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::*;
use sqlx::Row;
use std::time::Instant;

const DEFAULT_EXPECTED_DAYS: f64 = 7.0;

#[derive(Debug, Clone, PartialEq)]
pub struct OwedReplyRow {
    pub thread_id: ThreadId,
    pub latest_inbound_msg_id: MessageId,
    pub from_email: String,
    pub from_name: Option<String>,
    pub subject: String,
    pub latest_inbound_at: DateTime<Utc>,
    pub waiting_days: f64,
    pub expected_days: f64,
    pub overdue_score: f64,
}

/// Latest unanswered inbound message per thread, scored and ranked in
/// SQL so only the returned page is read into Rust. Every candidate is
/// scored, so the per-thread work stays inside `idx_messages_owed` and the
/// table row is read only for the returned page (see `scale_tests`).
///
/// Parameters: ?1 account, ?2 earliest latest-inbound time (the `within`
/// window) or NULL, ?3 latest latest-inbound time (the `older than`
/// floor) or NULL, ?4 now (unix seconds), ?5 the expected days when no
/// contact has a cadence, ?6 the limit.
const OWED_REPLIES_SQL: &str = r#"WITH inbound_latest AS (
        SELECT
            thread_id,
            MAX(date) AS latest_inbound_at
        FROM messages
        WHERE account_id = ?1 AND direction = 'inbound'
        GROUP BY thread_id
        -- A future-dated latest inbound is not waiting yet. The windows
        -- apply to the latest inbound, so they drop threads before lookups.
        HAVING MAX(date) <= ?4
           AND (?2 IS NULL OR MAX(date) >= ?2)
           AND (?3 IS NULL OR MAX(date) <= ?3)
    ),
    global_cadence AS (
        SELECT COALESCE(AVG(cadence_days_p50), ?5) AS days
        FROM contacts
        WHERE account_id = ?1 AND cadence_days_p50 IS NOT NULL
    ),
    scored AS (
        SELECT
            m.rowid AS msg_rowid,
            m.thread_id,
            m.date,
            -- The sender's reply cadence when it is a usable number of
            -- days, else the account's average, and never under half a day.
            MAX(
                CASE
                    -- Text sorts above any number, so these bounds also
                    -- reject a non-numeric value, as decoding it did.
                    WHEN contacts.cadence_days_p50 > 0
                     AND contacts.cadence_days_p50 <= 1.7976931348623157e308
                    THEN contacts.cadence_days_p50
                    ELSE global_cadence.days
                END,
                0.5
            ) AS expected_days,
            (CAST(?4 - m.date AS REAL) / 86400.0) AS waiting_days
        -- CROSS JOIN pins the join order: threads first, then a key search
        -- for their latest inbound message (ties on date return each one).
        FROM inbound_latest
        CROSS JOIN global_cadence
        CROSS JOIN messages m
          ON m.account_id = ?1
         AND m.direction = 'inbound'
         AND m.thread_id = inbound_latest.thread_id
         AND m.date = inbound_latest.latest_inbound_at
        -- Bare columns on the table side, so the (account_id, email)
        -- keys are used: contacts are stored lowercase, and
        -- screener_decisions.sender_email is COLLATE NOCASE. Wrapping
        -- them in LOWER() scanned every contact per candidate thread.
        LEFT JOIN contacts
          ON contacts.account_id = m.account_id
         AND contacts.email = LOWER(m.from_email)
        LEFT JOIN screener_decisions
          ON screener_decisions.account_id = m.account_id
         AND screener_decisions.sender_email = LOWER(m.from_email)
        WHERE NOT EXISTS (
                SELECT 1
                FROM messages outbound
                WHERE outbound.account_id = ?1
                  AND outbound.direction = 'outbound'
                  AND outbound.thread_id = inbound_latest.thread_id
                  AND outbound.date > inbound_latest.latest_inbound_at
            )
          AND COALESCE(contacts.is_list_sender, 0) = 0
          AND COALESCE(screener_decisions.disposition, '') != 'deny'
    ),
    ranked AS (
        SELECT *, waiting_days / expected_days AS overdue_score
        FROM scored
        -- Highest overdue first, then most recent inbound, then thread id,
        -- then insertion order for two messages sharing a thread and date.
        ORDER BY overdue_score DESC, date DESC, thread_id, msg_rowid
        LIMIT ?6
    )
    SELECT
        m.id AS msg_id,
        m.thread_id,
        m.from_email,
        m.from_name,
        m.subject,
        m.date AS latest_inbound_at,
        ranked.waiting_days,
        ranked.expected_days,
        ranked.overdue_score
    FROM ranked
    JOIN messages m ON m.rowid = ranked.msg_rowid
    -- The join does not keep the subquery's order.
    ORDER BY ranked.overdue_score DESC, ranked.date DESC, ranked.thread_id, ranked.msg_rowid
    "#;

impl super::Store {
    /// Compute owed-reply rows for `account_id`.
    ///
    /// * `older_than_days` — return only rows that have been waiting
    ///   at least this many days. `None` = no floor.
    /// * `within_days` — return only rows where the latest inbound
    ///   landed within the last N days. `None` = no ceiling.
    /// * `limit` — cap the number of returned rows.
    pub async fn list_owed_replies(
        &self,
        account_id: &AccountId,
        older_than_days: Option<u32>,
        within_days: Option<u32>,
        limit: u32,
    ) -> Result<Vec<OwedReplyRow>, sqlx::Error> {
        self.list_owed_replies_at(
            account_id,
            older_than_days,
            within_days,
            limit,
            Utc::now().timestamp(),
        )
        .await
    }

    async fn list_owed_replies_at(
        &self,
        account_id: &AccountId,
        older_than_days: Option<u32>,
        within_days: Option<u32>,
        limit: u32,
        now_unix: i64,
    ) -> Result<Vec<OwedReplyRow>, sqlx::Error> {
        let started_at = Instant::now();
        let day = 86_400_i64;
        let rows = sqlx::query(OWED_REPLIES_SQL)
            .bind(account_id.as_str())
            .bind(within_days.map(|days| now_unix - i64::from(days) * day))
            .bind(older_than_days.map(|days| now_unix - i64::from(days) * day))
            .bind(now_unix)
            .bind(DEFAULT_EXPECTED_DAYS)
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?;

        let owed = rows
            .into_iter()
            .map(|row| {
                Ok(OwedReplyRow {
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    latest_inbound_msg_id: decode_id(row.try_get::<&str, _>("msg_id")?)?,
                    from_email: row.try_get("from_email")?,
                    from_name: row.try_get("from_name")?,
                    subject: row.try_get("subject")?,
                    latest_inbound_at: decode_timestamp(row.try_get("latest_inbound_at")?)?,
                    waiting_days: row.try_get("waiting_days")?,
                    expected_days: row.try_get("expected_days")?,
                    overdue_score: row.try_get("overdue_score")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        trace_query("owed_replies.list", started_at, owed.len());
        Ok(owed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use mxr_core::types::*;

    async fn fixture_account(store: &Store) -> AccountId {
        let acct = mxr_core::Account {
            id: AccountId::new(),
            name: "T".into(),
            email: "me@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&acct).await.unwrap();
        acct.id
    }

    fn envelope(
        account_id: &AccountId,
        thread_id: &ThreadId,
        from: &str,
        days_ago: i64,
    ) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("p-{}", uuid::Uuid::now_v7()),
            thread_id: thread_id.clone(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: from.into(),
            },
            to: vec![Address {
                name: None,
                email: "me@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "subject".into(),
            date: Utc::now() - chrono::Duration::days(days_ago),
            flags: MessageFlags::empty(),
            snippet: "x".into(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    #[tokio::test]
    async fn latest_inbound_without_reply_appears() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;
        let thread = ThreadId::new();
        let env = envelope(&account_id, &thread, "alice@example.com", 10);
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        store.refresh_contacts().await.unwrap();
        let rows = store
            .list_owed_replies(&account_id, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].thread_id, thread);
        assert!(rows[0].waiting_days >= 9.5 && rows[0].waiting_days <= 10.5);
    }

    #[tokio::test]
    async fn thread_with_later_outbound_is_excluded() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;
        let thread = ThreadId::new();
        let inbound = envelope(&account_id, &thread, "alice@example.com", 5);
        let mut outbound = envelope(&account_id, &thread, "me@example.com", 1);
        outbound.from = Address {
            name: None,
            email: "me@example.com".into(),
        };
        outbound.to = vec![Address {
            name: None,
            email: "alice@example.com".into(),
        }];
        store
            .upsert_envelope_with_direction(&inbound, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();
        store.refresh_contacts().await.unwrap();
        let rows = store
            .list_owed_replies(&account_id, None, None, 10)
            .await
            .unwrap();
        assert!(
            rows.is_empty(),
            "thread with later outbound must not be listed: {rows:?}"
        );
    }

    #[tokio::test]
    async fn list_sender_is_excluded_via_contacts() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;
        let thread = ThreadId::new();
        let mut env = envelope(&account_id, &thread, "newsletter@example.com", 3);
        env.snippet = "List-Unsubscribe based".into();
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        // Force the contacts entry to be classified as a list sender:
        // refresh, then update the column directly. (Production marks
        // is_list_sender based on `messages.list_id`; we overwrite to
        // isolate the filter from upstream classification.)
        store.refresh_contacts().await.unwrap();
        sqlx::query("UPDATE contacts SET is_list_sender = 1 WHERE email = ?")
            .bind("newsletter@example.com")
            .execute(store.writer())
            .await
            .unwrap();
        let rows = store
            .list_owed_replies(&account_id, None, None, 10)
            .await
            .unwrap();
        assert!(rows.is_empty(), "list senders must be excluded: {rows:?}");
    }

    #[tokio::test]
    async fn screener_denied_sender_is_excluded() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;
        let thread = ThreadId::new();
        let env = envelope(&account_id, &thread, "spam@example.com", 4);
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        store.refresh_contacts().await.unwrap();
        store
            .set_screener_decision(&crate::ScreenerDecision {
                account_id: account_id.clone(),
                sender_email: "spam@example.com".into(),
                disposition: crate::ScreenerDisposition::Deny,
                route_label: None,
                decided_at: Utc::now(),
            })
            .await
            .unwrap();
        let rows = store
            .list_owed_replies(&account_id, None, None, 10)
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn contact_cadence_drives_overdue_ranking() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;

        // Two contacts, both 5 days waiting; alice has cadence p50 = 1d
        // (very overdue), bob has cadence p50 = 30d (still patient).
        let thread_a = ThreadId::new();
        let thread_b = ThreadId::new();
        let env_a = envelope(&account_id, &thread_a, "alice@example.com", 5);
        let env_b = envelope(&account_id, &thread_b, "bob@example.com", 5);
        store
            .upsert_envelope_with_direction(&env_a, MessageDirection::Inbound)
            .await
            .unwrap();
        store
            .upsert_envelope_with_direction(&env_b, MessageDirection::Inbound)
            .await
            .unwrap();
        store.refresh_contacts().await.unwrap();
        sqlx::query("UPDATE contacts SET cadence_days_p50 = 1.0 WHERE email = ?")
            .bind("alice@example.com")
            .execute(store.writer())
            .await
            .unwrap();
        sqlx::query("UPDATE contacts SET cadence_days_p50 = 30.0 WHERE email = ?")
            .bind("bob@example.com")
            .execute(store.writer())
            .await
            .unwrap();

        let rows = store
            .list_owed_replies(&account_id, None, None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].from_email, "alice@example.com");
        assert_eq!(rows[1].from_email, "bob@example.com");
        assert!(rows[0].overdue_score > rows[1].overdue_score);
    }

    #[tokio::test]
    async fn older_than_filter_drops_recent_rows() {
        let store = Store::in_memory().await.unwrap();
        let account_id = fixture_account(&store).await;
        let thread = ThreadId::new();
        let env = envelope(&account_id, &thread, "alice@example.com", 2);
        store
            .upsert_envelope_with_direction(&env, MessageDirection::Inbound)
            .await
            .unwrap();
        store.refresh_contacts().await.unwrap();
        let rows = store
            .list_owed_replies(&account_id, Some(7), None, 10)
            .await
            .unwrap();
        assert!(rows.is_empty());
        let rows = store
            .list_owed_replies(&account_id, Some(1), None, 10)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}

#[cfg(test)]
mod scale_tests;
