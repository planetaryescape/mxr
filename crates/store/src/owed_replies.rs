//! Slice 2.2 of docs/ai-email/02-follow-up-work.md.
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
        let started_at = Instant::now();
        let now_unix = Utc::now().timestamp();
        let global_p50: Option<f64> = sqlx::query_scalar(
            "SELECT AVG(cadence_days_p50)
             FROM contacts
             WHERE account_id = ? AND cadence_days_p50 IS NOT NULL",
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?
        .flatten();
        let global_p50 = global_p50.unwrap_or(DEFAULT_EXPECTED_DAYS);

        let rows = sqlx::query(
            r#"WITH inbound_latest AS (
                SELECT
                    thread_id,
                    MAX(date) AS latest_inbound_at
                FROM messages
                WHERE account_id = ?1 AND direction = 'inbound'
                GROUP BY thread_id
            ),
            outbound_latest AS (
                SELECT
                    thread_id,
                    MAX(date) AS latest_outbound_at
                FROM messages
                WHERE account_id = ?1 AND direction = 'outbound'
                GROUP BY thread_id
            ),
            owed AS (
                SELECT
                    inbound_latest.thread_id,
                    inbound_latest.latest_inbound_at
                FROM inbound_latest
                LEFT JOIN outbound_latest USING (thread_id)
                WHERE outbound_latest.latest_outbound_at IS NULL
                   OR outbound_latest.latest_outbound_at <= inbound_latest.latest_inbound_at
            )
            SELECT
                m.id AS msg_id,
                m.thread_id,
                m.from_email,
                m.from_name,
                m.subject,
                m.date AS latest_inbound_at,
                contacts.cadence_days_p50 AS contact_cadence,
                COALESCE(contacts.is_list_sender, 0) AS is_list_sender,
                COALESCE(screener_decisions.disposition, '') AS screener_disposition
            FROM owed
            JOIN messages m
              ON m.thread_id = owed.thread_id
             AND m.date = owed.latest_inbound_at
             AND m.account_id = ?1
             AND m.direction = 'inbound'
            LEFT JOIN contacts
              ON contacts.account_id = m.account_id
             AND LOWER(contacts.email) = LOWER(m.from_email)
            LEFT JOIN screener_decisions
              ON screener_decisions.account_id = m.account_id
             AND LOWER(screener_decisions.sender_email) = LOWER(m.from_email)
            WHERE COALESCE(contacts.is_list_sender, 0) = 0
              AND COALESCE(screener_decisions.disposition, '') != 'deny'
            "#,
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;

        let mut owed = Vec::with_capacity(rows.len());
        for row in rows {
            let inbound_at_secs: i64 = row.try_get("latest_inbound_at")?;
            let inbound_at = decode_timestamp(inbound_at_secs)?;
            let waiting_secs = now_unix - inbound_at_secs;
            if waiting_secs < 0 {
                continue;
            }
            let waiting_days = waiting_secs as f64 / 86_400.0;

            if let Some(min) = older_than_days {
                if waiting_days < min as f64 {
                    continue;
                }
            }
            if let Some(max) = within_days {
                if waiting_days > max as f64 {
                    continue;
                }
            }

            let contact_cadence: Option<f64> = row.try_get("contact_cadence").ok();
            let expected_days = contact_cadence
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(global_p50)
                .max(0.5);
            let overdue_score = waiting_days / expected_days;

            owed.push(OwedReplyRow {
                thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                latest_inbound_msg_id: decode_id(row.try_get::<&str, _>("msg_id")?)?,
                from_email: row.try_get("from_email")?,
                from_name: row.try_get("from_name")?,
                subject: row.try_get("subject")?,
                latest_inbound_at: inbound_at,
                waiting_days,
                expected_days,
                overdue_score,
            });
        }

        // Stable sort: highest overdue first, then most-recent inbound
        // first, then thread_id for determinism.
        owed.sort_by(|a, b| {
            b.overdue_score
                .partial_cmp(&a.overdue_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.latest_inbound_at.cmp(&a.latest_inbound_at))
                .then(a.thread_id.as_str().cmp(&b.thread_id.as_str()))
        });
        owed.truncate(limit as usize);

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

    fn envelope(account_id: &AccountId, thread_id: &ThreadId, from: &str, days_ago: i64) -> Envelope {
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
            label_provider_ids: vec![],
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
