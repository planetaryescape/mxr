//! Screener: per-(account, sender) consent-based quarantine.
//!
//! The decision row is the user's classification of a sender. The
//! screener queue is *computed* — there's no "queued" table. It's just
//! "inbound messages from senders without a decision row." That keeps
//! the data model lean and lets the queue update for free as new mail
//! arrives.

use crate::{decode_id, decode_timestamp, trace_lookup, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;
use sqlx::Row;

/// User-set classification of a sender. Mirrored 1:1 in the
/// `disposition` text column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenerDisposition {
    Allow,
    Deny,
    Feed,
    PaperTrail,
    Unknown,
}

impl ScreenerDisposition {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Feed => "feed",
            Self::PaperTrail => "paper_trail",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            "feed" => Some(Self::Feed),
            "paper_trail" => Some(Self::PaperTrail),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScreenerDecision {
    pub account_id: AccountId,
    pub sender_email: String,
    pub disposition: ScreenerDisposition,
    pub route_label: Option<String>,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ScreenerQueueEntry {
    pub sender_email: String,
    pub display_name: Option<String>,
    pub message_count: u32,
    pub latest_subject: String,
    pub latest_at: DateTime<Utc>,
}

impl super::Store {
    /// Set or update the disposition for `(account_id, sender_email)`.
    /// Re-setting refreshes `decided_at` and replaces `route_label`.
    pub async fn set_screener_decision(
        &self,
        decision: &ScreenerDecision,
    ) -> Result<(), sqlx::Error> {
        let aid = decision.account_id.as_str();
        let disposition = decision.disposition.as_db_str();
        let decided_at = decision.decided_at.timestamp();
        sqlx::query!(
            r#"INSERT INTO screener_decisions
                   (account_id, sender_email, disposition, route_label, decided_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(account_id, sender_email) DO UPDATE SET
                   disposition = excluded.disposition,
                   route_label = excluded.route_label,
                   decided_at = excluded.decided_at"#,
            aid,
            decision.sender_email,
            disposition,
            decision.route_label,
            decided_at,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn delete_screener_decision(
        &self,
        account_id: &AccountId,
        sender_email: &str,
    ) -> Result<bool, sqlx::Error> {
        let aid = account_id.as_str();
        let result = sqlx::query!(
            "DELETE FROM screener_decisions WHERE account_id = ? AND sender_email = ? COLLATE NOCASE",
            aid,
            sender_email,
        )
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_screener_decision(
        &self,
        account_id: &AccountId,
        sender_email: &str,
    ) -> Result<Option<ScreenerDecision>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT account_id as "account_id!", sender_email as "sender_email!",
                      disposition as "disposition!", route_label,
                      decided_at as "decided_at!"
               FROM screener_decisions
               WHERE account_id = ? AND sender_email = ? COLLATE NOCASE"#,
            aid,
            sender_email,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("screener.get_decision", started_at, row.is_some());
        match row {
            None => Ok(None),
            Some(r) => Ok(Some(ScreenerDecision {
                account_id: decode_id(&r.account_id)?,
                sender_email: r.sender_email,
                disposition: ScreenerDisposition::from_db_str(&r.disposition).ok_or_else(|| {
                    sqlx::Error::Protocol(format!(
                        "unknown screener disposition `{}`",
                        r.disposition
                    ))
                })?,
                route_label: r.route_label,
                decided_at: decode_timestamp(r.decided_at)?,
            })),
        }
    }

    pub async fn list_screener_decisions(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<ScreenerDecision>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT account_id as "account_id!", sender_email as "sender_email!",
                      disposition as "disposition!", route_label,
                      decided_at as "decided_at!"
               FROM screener_decisions
               WHERE account_id = ?
               ORDER BY sender_email ASC"#,
            aid,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("screener.list_decisions", started_at, rows.len());
        rows.into_iter()
            .map(|r| {
                Ok(ScreenerDecision {
                    account_id: decode_id(&r.account_id)?,
                    sender_email: r.sender_email,
                    disposition: ScreenerDisposition::from_db_str(&r.disposition).ok_or_else(
                        || {
                            sqlx::Error::Protocol(format!(
                                "unknown screener disposition `{}`",
                                r.disposition
                            ))
                        },
                    )?,
                    route_label: r.route_label,
                    decided_at: decode_timestamp(r.decided_at)?,
                })
            })
            .collect()
    }

    /// Compute the screener queue: senders with at least one inbound
    /// message in this account, but no `allow`/`deny`/`feed`/`paper_trail`
    /// decision yet. (`unknown` rows are also considered "no decision".)
    /// The screener is for strangers: anyone you have written to (To, Cc
    /// or Bcc of your sent mail, or a contact with outbound mail) is never
    /// queued, the desk's rule for being in conversation (D104).
    /// Returns one entry per distinct sender, with rollup stats.
    pub async fn list_screener_queue(
        &self,
        account_id: &AccountId,
        limit: u32,
    ) -> Result<Vec<ScreenerQueueEntry>, sqlx::Error> {
        let limit = limit.clamp(1, 500) as i64;
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(SCREENER_QUEUE_SQL)
            .bind(account_id.as_str())
            .bind(limit)
            .fetch_all(self.reader())
            .await?;
        trace_query("screener.list_queue", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                Ok(ScreenerQueueEntry {
                    sender_email: row.try_get("from_email")?,
                    display_name: row.try_get("from_name")?,
                    message_count: row.try_get::<i64, _>("message_count")?.max(0) as u32,
                    latest_subject: row.try_get("subject")?,
                    latest_at: decode_timestamp(row.try_get("latest_at")?)?,
                })
            })
            .collect()
    }
}

/// Parameters: ?1 account, ?2 limit.
const SCREENER_QUEUE_SQL: &str = r#"WITH own AS (
        SELECT LOWER(email) AS email FROM accounts WHERE id = ?1
        UNION
        SELECT LOWER(email) FROM account_addresses WHERE account_id = ?1
    ),
    -- Your sent mail by the desk's rule: stored as outbound, or of unknown
    -- direction and from one of the account's own addresses (an alias
    -- synced before its address was known).
    -- Two index lookups on (account_id, direction), read once for the
    -- three recipient lists below.
    sent AS MATERIALIZED (
        SELECT to_addrs, cc_addrs, bcc_addrs
        FROM messages
        WHERE account_id = ?1 AND direction = 'outbound'
        UNION ALL
        SELECT to_addrs, cc_addrs, bcc_addrs
        FROM messages
        WHERE account_id = ?1 AND direction = 'unknown'
          AND LOWER(from_email) IN (SELECT email FROM own)
    ),
    written_to AS (
        SELECT LOWER(json_extract(recipient.value, '$.email')) AS email
        FROM sent, json_each(sent.to_addrs) recipient
        UNION
        SELECT LOWER(json_extract(recipient.value, '$.email'))
        FROM sent, json_each(sent.cc_addrs) recipient
        UNION
        SELECT LOWER(json_extract(recipient.value, '$.email'))
        FROM sent, json_each(sent.bcc_addrs) recipient
        UNION
        SELECT email FROM contacts WHERE account_id = ?1 AND total_outbound > 0
    ),
    senders AS (
        -- With MAX(), SQLite takes the bare columns from the newest
        -- message, so the subject and name are the latest ones.
        SELECT
            LOWER(m.from_email) AS email,
            m.from_email,
            m.from_name,
            m.subject,
            COUNT(*) AS message_count,
            MAX(m.date) AS latest_at
        FROM messages m
        WHERE m.account_id = ?1 AND m.direction = 'inbound'
        GROUP BY LOWER(m.from_email)
    )
    SELECT from_email, from_name, subject, message_count, latest_at
    FROM senders
    WHERE email NOT IN (SELECT email FROM written_to WHERE email IS NOT NULL)
      AND NOT EXISTS (
          SELECT 1 FROM screener_decisions sd
          WHERE sd.account_id = ?1
            AND sd.sender_email = senders.email
            AND sd.disposition IN ('allow', 'deny', 'feed', 'paper_trail')
      )
    ORDER BY latest_at DESC
    LIMIT ?2"#;

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use super::{ScreenerDecision, ScreenerDisposition};
    use chrono::{TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId};
    use mxr_core::types::MessageDirection;

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    async fn make_account(store: &Store) -> AccountId {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        account.id
    }

    fn fresh_decision(
        account_id: &AccountId,
        email: &str,
        d: ScreenerDisposition,
    ) -> ScreenerDecision {
        ScreenerDecision {
            account_id: account_id.clone(),
            sender_email: email.to_string(),
            disposition: d,
            route_label: None,
            decided_at: anchor(),
        }
    }

    #[tokio::test]
    async fn set_and_get_round_trips() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        let decision = fresh_decision(&aid, "alice@example.com", ScreenerDisposition::Allow);
        store.set_screener_decision(&decision).await.unwrap();
        let got = store
            .get_screener_decision(&aid, "alice@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.sender_email, "alice@example.com");
        assert_eq!(got.disposition, ScreenerDisposition::Allow);
    }

    #[tokio::test]
    async fn get_decision_is_case_insensitive() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        store
            .set_screener_decision(&fresh_decision(
                &aid,
                "alice@example.com",
                ScreenerDisposition::Deny,
            ))
            .await
            .unwrap();
        let got = store
            .get_screener_decision(&aid, "ALICE@EXAMPLE.COM")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.disposition, ScreenerDisposition::Deny);
    }

    #[tokio::test]
    async fn re_setting_replaces_disposition_and_decided_at() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        store
            .set_screener_decision(&fresh_decision(
                &aid,
                "x@example.com",
                ScreenerDisposition::Feed,
            ))
            .await
            .unwrap();
        store
            .set_screener_decision(&fresh_decision(
                &aid,
                "x@example.com",
                ScreenerDisposition::PaperTrail,
            ))
            .await
            .unwrap();
        let got = store
            .get_screener_decision(&aid, "x@example.com")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.disposition, ScreenerDisposition::PaperTrail);
    }

    #[tokio::test]
    async fn delete_returns_true_for_existing_and_false_for_missing() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        assert!(!store
            .delete_screener_decision(&aid, "nope@example.com")
            .await
            .unwrap());
        store
            .set_screener_decision(&fresh_decision(
                &aid,
                "y@example.com",
                ScreenerDisposition::Allow,
            ))
            .await
            .unwrap();
        assert!(store
            .delete_screener_decision(&aid, "y@example.com")
            .await
            .unwrap());
        assert!(store
            .get_screener_decision(&aid, "y@example.com")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn list_decisions_orders_alphabetically() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        for email in ["zoe@example.com", "alice@example.com", "bob@example.com"] {
            store
                .set_screener_decision(&fresh_decision(&aid, email, ScreenerDisposition::Allow))
                .await
                .unwrap();
        }
        let listed = store.list_screener_decisions(&aid).await.unwrap();
        let emails: Vec<_> = listed.into_iter().map(|d| d.sender_email).collect();
        assert_eq!(
            emails,
            vec!["alice@example.com", "bob@example.com", "zoe@example.com"]
        );
    }

    #[tokio::test]
    async fn screener_queue_includes_inbound_senders_without_decision() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;

        // Two senders: alice (no decision) and bob (allowed).
        let mut env_a = TestEnvelopeBuilder::new().account_id(aid.clone()).build();
        env_a.id = MessageId::new();
        env_a.provider_id = "msg-a".into();
        env_a.from = mxr_core::types::Address {
            name: Some("Alice".into()),
            email: "alice@example.com".into(),
        };
        env_a.subject = "Hello from Alice".into();
        store
            .upsert_envelope_with_direction(&env_a, mxr_core::types::MessageDirection::Inbound)
            .await
            .unwrap();

        let mut env_b = TestEnvelopeBuilder::new().account_id(aid.clone()).build();
        env_b.id = MessageId::new();
        env_b.provider_id = "msg-b".into();
        env_b.from = mxr_core::types::Address {
            name: None,
            email: "bob@example.com".into(),
        };
        env_b.subject = "Bob's message".into();
        store
            .upsert_envelope_with_direction(&env_b, mxr_core::types::MessageDirection::Inbound)
            .await
            .unwrap();

        store
            .set_screener_decision(&fresh_decision(
                &aid,
                "bob@example.com",
                ScreenerDisposition::Allow,
            ))
            .await
            .unwrap();

        let queue = store.list_screener_queue(&aid, 10).await.unwrap();
        let emails: Vec<_> = queue.iter().map(|q| q.sender_email.clone()).collect();
        assert_eq!(emails, vec!["alice@example.com"]);
        assert_eq!(queue[0].latest_subject, "Hello from Alice");
    }

    async fn store_message(
        store: &Store,
        aid: &AccountId,
        n: i64,
        (from, direction): (&str, MessageDirection),
        [to, cc, bcc]: [&[&str]; 3],
    ) {
        use mxr_core::types::Address;
        let addresses = |emails: &[&str]| {
            emails
                .iter()
                .map(|email| Address {
                    name: None,
                    email: (*email).into(),
                })
                .collect()
        };
        let mut env = TestEnvelopeBuilder::new().account_id(aid.clone()).build();
        env.provider_id = format!("msg-{n}");
        env.from = Address {
            name: None,
            email: from.into(),
        };
        env.to = addresses(to);
        env.cc = addresses(cc);
        env.bcc = addresses(bcc);
        env.date = anchor() + chrono::Duration::minutes(n);
        store
            .upsert_envelope_with_direction(&env, direction)
            .await
            .unwrap();
    }

    const ME: (&str, MessageDirection) = ("test@example.com", MessageDirection::Outbound);

    fn sender(email: &str) -> (&str, MessageDirection) {
        (email, MessageDirection::Inbound)
    }

    async fn queued(store: &Store, aid: &AccountId) -> Vec<String> {
        let mut emails: Vec<_> = store
            .list_screener_queue(aid, 10)
            .await
            .unwrap()
            .into_iter()
            .map(|q| q.sender_email)
            .collect();
        emails.sort();
        emails
    }

    #[tokio::test]
    async fn screener_queue_skips_anyone_you_have_written_to() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        let senders = [
            "Carol@Example.com",
            "dave@example.com",
            "erin@example.com",
            "frank@example.com",
        ];
        for (n, email) in (1..).zip(senders) {
            store_message(&store, &aid, n, sender(email), [&[ME.0], &[], &[]]).await;
        }
        // You wrote to Carol, copied Erin and blind-copied Frank.
        store_message(
            &store,
            &aid,
            9,
            ME,
            [
                &["carol@example.com"],
                &["ERIN@example.com"],
                &["frank@example.com"],
            ],
        )
        .await;
        // No contacts refresh needed: the sent mail itself says so.
        assert_eq!(queued(&store, &aid).await, vec!["dave@example.com"]);
    }

    #[tokio::test]
    async fn screener_queue_counts_mail_from_an_alias_of_unknown_direction() {
        let store = Store::in_memory().await.unwrap();
        let aid = make_account(&store).await;
        store
            .add_account_address(&aid, "Me@Alias.example", false)
            .await
            .unwrap();
        for (n, email) in (1..).zip(["carol@example.com", "dave@example.com"]) {
            store_message(&store, &aid, n, sender(email), [&[ME.0], &[], &[]]).await;
        }
        // Synced before the alias was known: stored with no direction.
        store_message(
            &store,
            &aid,
            9,
            ("me@alias.example", MessageDirection::Unknown),
            [&["carol@example.com"], &[], &[]],
        )
        .await;
        // The same from a stranger is not your sent mail.
        store_message(
            &store,
            &aid,
            10,
            ("someone@else.example", MessageDirection::Unknown),
            [&["dave@example.com"], &[], &[]],
        )
        .await;
        assert_eq!(queued(&store, &aid).await, vec!["dave@example.com"]);
    }
}
