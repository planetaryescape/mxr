//! What subscription detection reads beyond the records themselves: which
//! titles were stated (schema.org or you), who sent a record's emails, and
//! the mail those senders sent later that might end a subscription.
//!
//! Subscriptions are recomputed from the records on every read
//! (`mxr_records::subscriptions`), so there is no table here.

use crate::{decode_id, decode_timestamp, in_list, SQLITE_BIND_CHUNK};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use sqlx::Row;
use std::collections::HashSet;

/// An email from a sender of a subscription's charges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderMessage {
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub from_email: String,
    pub subject: String,
    pub date: DateTime<Utc>,
}

impl super::Store {
    /// The records among `record_ids` whose title came from schema.org or
    /// the user: a stated plan. Either outranks a rule's title, so it is
    /// the winner whenever it exists.
    pub async fn records_with_stated_title(
        &self,
        record_ids: &[String],
    ) -> Result<HashSet<String>, sqlx::Error> {
        let mut out = HashSet::new();
        for chunk in record_ids.chunks(SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT DISTINCT record_id FROM record_fields
                 WHERE field = 'title' AND source IN ('schema', 'user')
                   AND record_id IN ({})",
                in_list(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.insert(row.try_get::<String, _>("record_id")?);
            }
        }
        Ok(out)
    }

    /// The sender address of each record's source emails, as
    /// (record id, address) pairs.
    pub async fn record_sender_emails(
        &self,
        record_ids: &[String],
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in record_ids.chunks(SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT DISTINCT rm.record_id, LOWER(m.from_email) AS from_email
                 FROM record_messages rm JOIN messages m ON m.id = rm.message_id
                 WHERE rm.record_id IN ({})",
                in_list(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push((row.try_get("record_id")?, row.try_get("from_email")?));
            }
        }
        Ok(out)
    }

    /// Mail from these senders on or after `since` whose subject mentions
    /// a cancellation or an ending, oldest first. The caller decides which
    /// ones end a subscription; this only narrows the rows.
    pub async fn ending_mail_from_senders(
        &self,
        senders: &[String],
        since: DateTime<Utc>,
    ) -> Result<Vec<SenderMessage>, sqlx::Error> {
        let mut out = Vec::new();
        for chunk in senders.chunks(SQLITE_BIND_CHUNK) {
            let sql = ending_mail_sql(chunk.len());
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for sender in chunk {
                query = query.bind(sender.as_str());
            }
            for row in query
                .bind(since.timestamp())
                .fetch_all(self.reader())
                .await?
            {
                out.push(SenderMessage {
                    account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                    message_id: decode_id(&row.try_get::<String, _>("id")?)?,
                    from_email: row.try_get("from_email")?,
                    subject: row.try_get("subject")?,
                    date: decode_timestamp(row.try_get("date")?)?,
                });
            }
        }
        out.sort_by(|a, b| {
            a.date
                .cmp(&b.date)
                .then_with(|| a.message_id.as_str().cmp(&b.message_id.as_str()))
        });
        Ok(out)
    }
}

fn ending_mail_sql(senders: usize) -> String {
    // Enumerating the small account table makes the existing account/sender/
    // date index usable. Without the account prefix this was a mailbox scan
    // on every Now refresh. Disabled accounts still participate, as before.
    format!(
        "SELECT m.id, m.account_id, LOWER(m.from_email) AS from_email, m.subject, m.date
         FROM accounts own CROSS JOIN messages m ON m.account_id = own.id
         WHERE LOWER(m.from_email) IN ({}) AND m.date >= ?
           AND (m.subject LIKE '%cancel%' OR m.subject LIKE '%ended%'
                OR m.subject LIKE '%terminated%' OR m.subject LIKE '%see you go%')
         ORDER BY m.date, m.id",
        in_list(senders)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use chrono::Duration;

    #[tokio::test]
    async fn ending_mail_keeps_sender_date_and_disabled_account_boundaries() {
        let store = Store::in_memory().await.unwrap();
        let first = test_account();
        let mut disabled = test_account();
        disabled.enabled = false;
        for account in [&first, &disabled] {
            store.insert_account(account).await.unwrap();
        }
        let since = Utc::now() - Duration::days(1);
        let mut expected = HashSet::new();
        for (index, (account, sender, subject, date, included)) in [
            (
                &first,
                "BILLING@example.test",
                "Plan cancelled",
                since,
                true,
            ),
            (
                &disabled,
                "billing@example.test",
                "Your plan ended",
                since + Duration::hours(1),
                true,
            ),
            (
                &first,
                "billing@example.test",
                "Plan terminated",
                since - Duration::seconds(1),
                false,
            ),
            (
                &first,
                "different@example.test",
                "Sorry to see you go",
                since,
                false,
            ),
            (
                &first,
                "billing@example.test",
                "Regular charge",
                since,
                false,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut envelope = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            envelope.provider_id = format!("ending-{index}");
            envelope.from.email = sender.into();
            envelope.subject = subject.into();
            envelope.date = date;
            store.upsert_envelope(&envelope).await.unwrap();
            if included {
                expected.insert(envelope.id);
            }
        }
        let mail = store
            .ending_mail_from_senders(
                &["billing@example.test".into(), "billing@example.test".into()],
                since,
            )
            .await
            .unwrap();
        assert_eq!(
            mail.iter()
                .map(|m| m.message_id.clone())
                .collect::<HashSet<_>>(),
            expected
        );
        assert_eq!(
            mail.len(),
            2,
            "duplicate requested senders do not duplicate mail"
        );
        assert!(mail.iter().all(|m| m.from_email == "billing@example.test"));
        assert_eq!(mail[0].account_id, first.id);
        assert_eq!(mail[1].account_id, disabled.id);
    }

    #[tokio::test]
    async fn ending_mail_uses_account_sender_and_date_instead_of_a_mailbox_scan() {
        let store = Store::in_memory().await.unwrap();
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "EXPLAIN QUERY PLAN {}",
            ending_mail_sql(1)
        )))
        .bind("billing@example.test")
        .bind(0_i64)
        .fetch_all(store.reader())
        .await
        .unwrap();
        let steps: Vec<String> = rows.iter().map(|row| row.get("detail")).collect();
        assert!(
            steps
                .iter()
                .any(|s| s.contains("idx_messages_sender_history")
                    && s.contains("account_id=? AND <expr>=? AND date>?")),
            "{steps:#?}"
        );
        assert!(!steps.iter().any(|s| s.starts_with("SCAN m")), "{steps:#?}");
    }
}
