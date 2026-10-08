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
            let sql = format!(
                "SELECT id, account_id, LOWER(from_email) AS from_email, subject, date
                 FROM messages
                 WHERE LOWER(from_email) IN ({}) AND date >= ?
                   AND (subject LIKE '%cancel%' OR subject LIKE '%ended%'
                        OR subject LIKE '%terminated%' OR subject LIKE '%see you go%')
                 ORDER BY date, id",
                in_list(chunk.len())
            );
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
