//! Raw reads behind Reading and Paper trail (`Request::ListPlace`,
//! `Request::SweepPlace`) and the local pins that keep a message out of a
//! sweep.
//!
//! A place is inbox mail of one kind. The daemon decides the kind; this
//! module fetches every inbox message of an account with the signals that
//! decision needs, in one query that starts from the account's INBOX label.
//! The inbox is what a sweep empties, so it is the candidate set whatever
//! the mailbox's total size.

use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};
use sqlx::Row;
use std::time::Instant;

/// One inbox message with what deciding its kind needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceMessage {
    pub id: MessageId,
    /// Storage order (the row's rowid), for "through the preview" bounds.
    pub seq: i64,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// `inbound`, `outbound` or `unknown`, as stored.
    pub direction: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub from_email: String,
    pub from_name: Option<String>,
    /// For "addressed to you, or only copied" (D119's never-bury rule).
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub evidence: mxr_core::mail_evidence::MailEvidence,
    pub snippet: String,
    pub list_id: Option<String>,
    pub unsubscribe: UnsubscribeMethod,
    pub is_delivery: bool,
    pub is_invite: bool,
    pub pinned: bool,
    pub snoozed: bool,
    pub in_inbox: bool,
}

impl super::Store {
    /// Threads with mail outside Trash and Spam dated `since` or later: the
    /// threads background model work may read.
    pub async fn thread_ids_with_live_mail_since(
        &self,
        account_id: &AccountId,
        since: DateTime<Utc>,
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        let sql = format!(
            "SELECT DISTINCT m.thread_id FROM messages m
             WHERE m.account_id = ?1 AND m.date >= ?3 AND {NOT_TRASHED}"
        );
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(hidden_flags())
            .bind(since.timestamp())
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(|id| decode_id(id))
            .collect()
    }

    /// Every inbox message of `account_id` that is not trashed or spam.
    pub async fn place_candidates(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<PlaceMessage>, sqlx::Error> {
        let started_at = Instant::now();
        let rows = sqlx::query(sqlx::AssertSqlSafe(place_candidates_sql()))
            .bind(account_id.as_str())
            .bind(hidden_flags())
            .fetch_all(self.reader())
            .await?;
        let messages = rows
            .iter()
            .map(decode_place_message)
            .collect::<Result<Vec<_>, _>>()?;
        trace_query("places.candidates", started_at, messages.len());
        Ok(messages)
    }

    /// One message in the same shape, wherever it is (for "why is this
    /// here" on any message).
    pub async fn place_message(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<PlaceMessage>, sqlx::Error> {
        let row = sqlx::query(sqlx::AssertSqlSafe(place_message_sql()))
            .bind(message_id.as_str())
            .bind(hidden_flags())
            .fetch_optional(self.reader())
            .await?;
        row.as_ref().map(decode_place_message).transpose()
    }

    /// Recent mail from these senders anywhere but Trash and Spam, at most
    /// `per_sender` each, newest first: the history Updates compares a
    /// source's new mail with, inbox or not.
    pub async fn recent_from_senders(
        &self,
        account_id: &AccountId,
        from_emails: &[String],
        since: DateTime<Utc>,
        per_sender: u32,
    ) -> Result<Vec<PlaceMessage>, sqlx::Error> {
        if from_emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = encode_json(
            &from_emails
                .iter()
                .map(|email| email.to_ascii_lowercase())
                .collect::<Vec<_>>(),
        )?;
        let sql = format!(
            r#"SELECT * FROM (
                SELECT {PLACE_COLUMNS},
                    EXISTS (
                        SELECT 1 FROM message_labels iml JOIN labels il ON il.id = iml.label_id
                        WHERE iml.message_id = m.id AND il.provider_id = 'INBOX'
                    ) AS in_inbox,
                    ROW_NUMBER() OVER (
                        PARTITION BY lower(m.from_email) ORDER BY m.date DESC, m.id DESC
                    ) AS sender_rank
                FROM messages m
                WHERE m.account_id = ?1 AND m.date >= ?3
                  AND lower(m.from_email) IN (SELECT value FROM json_each(?4))
                  AND {NOT_TRASHED}
            ) WHERE sender_rank <= ?5"#
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(hidden_flags())
            .bind(since.timestamp())
            .bind(wanted)
            .bind(i64::from(per_sender))
            .fetch_all(self.reader())
            .await?;
        let messages = rows
            .iter()
            .map(decode_place_message)
            .collect::<Result<Vec<_>, _>>()?;
        trace_query("places.recent_from_senders", started_at, messages.len());
        Ok(messages)
    }

    /// Pin or unpin messages. Returns how many changed; unknown ids and
    /// messages already in the wanted state are skipped.
    pub async fn set_message_pins(
        &self,
        message_ids: &[MessageId],
        pinned: bool,
    ) -> Result<u64, sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let wanted = encode_json(
            &message_ids
                .iter()
                .map(MessageId::as_str)
                .collect::<Vec<_>>(),
        )?;
        let result = if pinned {
            sqlx::query(
                "INSERT INTO message_pins (message_id, account_id, pinned_at)
                 SELECT m.id, m.account_id, ?2
                 FROM json_each(?1) wanted
                 CROSS JOIN messages m ON m.id = wanted.value
                 WHERE true
                 ON CONFLICT(message_id) DO NOTHING",
            )
            .bind(&wanted)
            .bind(Utc::now().timestamp())
            .execute(self.writer())
            .await?
        } else {
            sqlx::query(
                "DELETE FROM message_pins WHERE message_id IN (SELECT value FROM json_each(?1))",
            )
            .bind(&wanted)
            .execute(self.writer())
            .await?
        };
        Ok(result.rows_affected())
    }
}

/// `alias`'s message is outside Trash and Spam, by flag and by label: the
/// mail background model work may read. The flag mask is a constant, so it
/// is written into the SQL rather than bound.
pub(crate) fn live_mail_sql(alias: &str) -> String {
    format!(
        "({alias}.flags & {flags}) = 0 AND NOT EXISTS (
            SELECT 1 FROM message_labels lml JOIN labels ll ON ll.id = lml.label_id
            WHERE lml.message_id = {alias}.id AND ll.provider_id IN ('TRASH', 'SPAM'))",
        flags = hidden_flags()
    )
}

fn hidden_flags() -> i64 {
    i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits())
}

/// Columns and per-message state shared by both reads. `?2` is the hidden
/// flag mask.
const PLACE_COLUMNS: &str = r#"
    m.rowid AS seq, m.id, m.account_id, m.thread_id, m.direction, m.date, m.flags,
    m.from_email, m.from_name, m.to_addrs, m.cc_addrs, m.subject, m.snippet, m.list_id,
    m.unsubscribe_method,
    m.in_reply_to AS evidence_reply,
    EXISTS (SELECT 1 FROM messages sent WHERE sent.account_id = m.account_id
        AND sent.thread_id = m.thread_id AND (sent.direction = 'outbound'
            OR (sent.direction != 'inbound' AND EXISTS (SELECT 1 FROM account_addresses own
                WHERE own.account_id = sent.account_id AND lower(own.email) = lower(sent.from_email))))
        AND sent.id != m.id) AS evidence_thread_correspondence,
    (SELECT json_object('list_id', json_extract(metadata_json, '$.list_id'),
        'raw_headers', substr(json_extract(metadata_json, '$.raw_headers'), 1, 131072))
        FROM bodies WHERE message_id = m.id) AS evidence_metadata,
    (SELECT substr(text_plain, 1, 8192) FROM bodies WHERE message_id = m.id) AS evidence_text,
    COALESCE(
        (SELECT substr(pb.text_plain, 1, 8192) FROM messages previous JOIN bodies pb ON pb.message_id = previous.id
            WHERE previous.account_id = m.account_id AND previous.thread_id = m.thread_id
            AND previous.id != m.id AND previous.message_id_header = m.in_reply_to
            ORDER BY previous.date DESC, previous.id DESC LIMIT 1),
        (SELECT substr(pb.text_plain, 1, 8192) FROM messages previous JOIN bodies pb ON pb.message_id = previous.id
            WHERE previous.account_id = m.account_id AND previous.thread_id = m.thread_id
            AND (previous.date < m.date OR (previous.date = m.date AND previous.id < m.id))
            ORDER BY previous.date DESC, previous.id DESC LIMIT 1)) AS evidence_previous_text,
    EXISTS (SELECT 1 FROM delivery_messages dm WHERE dm.message_id = m.id) AS is_delivery,
    EXISTS (SELECT 1 FROM calendar_invites ci WHERE ci.message_id = m.id) AS is_invite,
    EXISTS (SELECT 1 FROM message_pins p WHERE p.message_id = m.id) AS pinned,
    EXISTS (SELECT 1 FROM snoozed s WHERE s.message_id = m.id) AS snoozed"#;

const NOT_TRASHED: &str = r#"(m.flags & ?2) = 0
    AND NOT EXISTS (
        SELECT 1 FROM message_labels tml JOIN labels tl ON tl.id = tml.label_id
        WHERE tml.message_id = m.id AND tl.provider_id IN ('TRASH', 'SPAM')
    )"#;

/// `?1` account, `?2` hidden flags.
/// Starts from the account's INBOX label so the read is proportional to the
/// inbox, never to the mailbox; CROSS JOIN keeps that order.
fn place_candidates_sql() -> String {
    format!(
        r#"SELECT {PLACE_COLUMNS}, 1 AS in_inbox
        FROM labels l
        CROSS JOIN message_labels ml ON ml.label_id = l.id
        CROSS JOIN messages m ON m.id = ml.message_id
        WHERE l.account_id = ?1 AND l.provider_id = 'INBOX'
          AND m.account_id = ?1
          AND {NOT_TRASHED}"#
    )
}

/// `?1` message id, `?2` hidden flags. A trashed message is not returned.
fn place_message_sql() -> String {
    format!(
        r#"SELECT {PLACE_COLUMNS},
            EXISTS (
                SELECT 1 FROM message_labels iml JOIN labels il ON il.id = iml.label_id
                WHERE iml.message_id = m.id AND il.provider_id = 'INBOX'
            ) AS in_inbox
        FROM messages m
        WHERE m.id = ?1 AND {NOT_TRASHED}"#
    )
}

fn decode_place_message(row: &sqlx::sqlite::SqliteRow) -> Result<PlaceMessage, sqlx::Error> {
    let unsubscribe = row
        .try_get::<Option<String>, _>("unsubscribe_method")?
        .as_deref()
        .map(decode_json::<UnsubscribeMethod>)
        .transpose()?
        .unwrap_or(UnsubscribeMethod::None);
    Ok(PlaceMessage {
        id: decode_id(row.try_get::<&str, _>("id")?)?,
        seq: row.try_get("seq")?,
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
        direction: row.try_get("direction")?,
        date: decode_timestamp(row.try_get("date")?)?,
        flags: MessageFlags::from_bits_truncate(row.try_get::<i64, _>("flags")? as u32),
        from_email: row.try_get("from_email")?,
        from_name: row.try_get("from_name")?,
        to: decode_json(row.try_get::<&str, _>("to_addrs")?)?,
        cc: decode_json(row.try_get::<&str, _>("cc_addrs")?)?,
        subject: row.try_get("subject")?,
        evidence: crate::body::evidence_from_row(row)?,
        snippet: row.try_get("snippet")?,
        list_id: row.try_get("list_id")?,
        unsubscribe,
        is_delivery: row.try_get("is_delivery")?,
        is_invite: row.try_get("is_invite")?,
        pinned: row.try_get("pinned")?,
        snoozed: row.try_get("snoozed")?,
        in_inbox: row.try_get("in_inbox")?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::test_fixtures::*;
    use crate::Store;
    use mxr_core::id::LabelId;
    use mxr_core::types::{EventSource, Label, LabelKind, MessageDirection};

    async fn plan(store: &Store, sql: &str) -> Vec<String> {
        sqlx::query(sqlx::AssertSqlSafe(format!("EXPLAIN QUERY PLAN {sql}")))
            .fetch_all(store.reader())
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect()
    }

    /// The candidate read must walk the inbox label, not the mailbox: on a
    /// 110k-message store a scan of `messages` is the difference between a
    /// few milliseconds and a second.
    #[tokio::test]
    async fn place_reads_start_from_the_inbox_label() {
        let store = Store::in_memory().await.unwrap();
        let steps = plan(&store, &place_candidates_sql()).await;
        assert!(
            steps[0].starts_with("SEARCH l USING INDEX"),
            "labels first: {steps:#?}"
        );
        assert!(
            steps
                .iter()
                .any(|s| s.starts_with("SEARCH ml USING INDEX idx_message_labels_label")),
            "{steps:#?}"
        );
        assert!(
            steps
                .iter()
                .any(|s| s.starts_with("SEARCH m USING INDEX sqlite_autoindex_messages_1")),
            "{steps:#?}"
        );
        assert!(
            !steps.iter().any(|s| s.starts_with("SCAN")),
            "no table scans: {steps:#?}"
        );

        let one = plan(&store, &place_message_sql()).await;
        assert!(!one.iter().any(|s| s.starts_with("SCAN")), "{one:#?}");

        assert_thread_lookups_use_the_thread(&steps);
        assert_thread_lookups_use_the_thread(&one);
    }

    /// The evidence columns look up each row's thread several times. Searching
    /// by account alone walks the whole mailbox per row: on a 110k-message
    /// store that was ~0.3s a message and starved the reader pool.
    pub(crate) fn assert_thread_lookups_use_the_thread(steps: &[String]) {
        let lookups: Vec<_> = steps
            .iter()
            .filter(|s| s.starts_with("SEARCH sent ") || s.starts_with("SEARCH previous "))
            .collect();
        assert!(!lookups.is_empty(), "{steps:#?}");
        assert!(
            lookups
                .iter()
                .all(|s| s.contains("(account_id=? AND thread_id=?)")),
            "thread lookups must use account and thread: {steps:#?}"
        );
    }

    #[tokio::test]
    async fn candidates_are_inbox_mail_with_pins() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let inbox = LabelId::from_scoped_provider_id(&account.id, "fake", "INBOX");
        store
            .upsert_label(&Label {
                id: inbox.clone(),
                account_id: account.id.clone(),
                name: "Inbox".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".into(),
                unread_count: 0,
                total_count: 0,
                role: None,
            })
            .await
            .unwrap();
        let mut ids = Vec::new();
        for (index, in_inbox) in [true, true, false].into_iter().enumerate() {
            let mut envelope = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            envelope.provider_id = format!("p-{index}");
            envelope.from.email = "news@lists.example.com".into();
            store
                .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
                .await
                .unwrap();
            if in_inbox {
                store
                    .set_message_labels(
                        &envelope.id,
                        std::slice::from_ref(&inbox),
                        EventSource::User,
                    )
                    .await
                    .unwrap();
            }
            ids.push(envelope.id);
        }

        let all = store.place_candidates(&account.id).await.unwrap();
        assert_eq!(all.len(), 2, "archived mail is not a candidate");
        assert!(all.iter().all(|m| m.in_inbox && !m.pinned));

        assert_eq!(store.set_message_pins(&ids[..1], true).await.unwrap(), 1);
        assert_eq!(
            store.set_message_pins(&ids[..1], true).await.unwrap(),
            0,
            "pinning twice changes nothing"
        );
        let pinned = store.place_candidates(&account.id).await.unwrap();
        assert!(pinned.iter().any(|m| m.id == ids[0] && m.pinned));

        assert_eq!(store.set_message_pins(&ids[..1], false).await.unwrap(), 1);
        let archived = store.place_message(&ids[2]).await.unwrap().unwrap();
        assert!(!archived.in_inbox);
    }
}
