//! The user's own writing, as material for AI drafts written in their voice.
//!
//! Drafts that "sound like me" need real examples, not statistics: the
//! replies the user actually sent (with the message each one answered), and
//! the name they sign with. Everything here is read-only and scoped to one
//! account; `before` lets a caller rebuild what was known at a point in
//! time (the offline draft evaluation replays history this way).

use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

use crate::{decode_id, decode_timestamp, trace_query};

/// One reply the user sent, with the message it answered.
#[derive(Debug, Clone)]
pub struct MyReplySample {
    pub reply_message_id: MessageId,
    pub parent_message_id: MessageId,
    pub thread_id: ThreadId,
    pub counterparty_email: String,
    pub replied_at: DateTime<Utc>,
    /// Raw body of the user's reply (plain text, else HTML, else snippet).
    pub reply_body: String,
    /// Raw body of the message the user was answering.
    pub parent_body: String,
    pub parent_from_name: Option<String>,
    pub parent_from_email: String,
}

/// One message the user sent (reply or not) to a given address.
#[derive(Debug, Clone)]
pub struct MySentSample {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub date: DateTime<Utc>,
    pub body: String,
}

impl super::Store {
    /// The user's replies, newest first: to `counterparty` when given (their
    /// messages to this person), else across the account. `exclude_thread`
    /// and `before` keep the conversation being drafted, and anything after
    /// it, out of the examples.
    pub async fn my_reply_samples(
        &self,
        account_id: &AccountId,
        counterparty: Option<&str>,
        exclude_thread: Option<&ThreadId>,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<MyReplySample>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT rp.reply_message_id, rp.parent_message_id, r.thread_id, rp.counterparty_email, rp.replied_at,
                      rb.text_plain AS reply_plain, rb.text_html AS reply_html, r.snippet AS reply_snippet,
                      pb.text_plain AS parent_plain, pb.text_html AS parent_html, p.snippet AS parent_snippet,
                      p.from_name AS parent_from_name, p.from_email AS parent_from_email
               FROM reply_pairs rp
               JOIN messages r ON r.id = rp.reply_message_id
               JOIN messages p ON p.id = rp.parent_message_id
               LEFT JOIN bodies rb ON rb.message_id = r.id
               LEFT JOIN bodies pb ON pb.message_id = p.id
               WHERE rp.account_id = ?
                 AND rp.direction = 'i_replied'
                 AND (? IS NULL OR LOWER(rp.counterparty_email) = LOWER(?))
                 AND (? IS NULL OR r.thread_id != ?)
                 AND (? IS NULL OR rp.replied_at < ?)
               ORDER BY rp.replied_at DESC
               LIMIT ?"#,
        )
        .bind(account_id.as_str())
        .bind(counterparty)
        .bind(counterparty)
        .bind(exclude_thread.map(ThreadId::as_str))
        .bind(exclude_thread.map(ThreadId::as_str))
        .bind(before.map(|date| date.timestamp()))
        .bind(before.map(|date| date.timestamp()))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        trace_query("voice_samples.my_replies", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                Ok(MyReplySample {
                    reply_message_id: decode_id(row.get::<String, _>("reply_message_id").as_str())?,
                    parent_message_id: decode_id(
                        row.get::<String, _>("parent_message_id").as_str(),
                    )?,
                    thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
                    counterparty_email: row.get("counterparty_email"),
                    replied_at: decode_timestamp(row.get("replied_at"))?,
                    reply_body: body_of(&row, "reply"),
                    parent_body: body_of(&row, "parent"),
                    parent_from_name: row.get("parent_from_name"),
                    parent_from_email: row.get("parent_from_email"),
                })
            })
            .collect()
    }

    /// Messages the user sent to `email` (as To or Cc), newest first. Covers
    /// first messages and forwards that `my_reply_samples` can't.
    pub async fn my_sent_to(
        &self,
        account_id: &AccountId,
        email: &str,
        exclude_thread: Option<&ThreadId>,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<MySentSample>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT m.id, m.thread_id, m.date, m.snippet, b.text_plain, b.text_html
               FROM messages m
               LEFT JOIN bodies b ON b.message_id = m.id
               WHERE m.account_id = ?
                 AND m.direction = 'outbound'
                 AND (
                   EXISTS (SELECT 1 FROM json_each(m.to_addrs) WHERE LOWER(json_extract(value, '$.email')) = LOWER(?))
                   OR EXISTS (SELECT 1 FROM json_each(m.cc_addrs) WHERE LOWER(json_extract(value, '$.email')) = LOWER(?))
                 )
                 AND (? IS NULL OR m.thread_id != ?)
                 AND (? IS NULL OR m.date < ?)
               ORDER BY m.date DESC
               LIMIT ?"#,
        )
        .bind(account_id.as_str())
        .bind(email)
        .bind(email)
        .bind(exclude_thread.map(ThreadId::as_str))
        .bind(exclude_thread.map(ThreadId::as_str))
        .bind(before.map(|date| date.timestamp()))
        .bind(before.map(|date| date.timestamp()))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        trace_query("voice_samples.my_sent_to", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                let body = row
                    .get::<Option<String>, _>("text_plain")
                    .or_else(|| row.get::<Option<String>, _>("text_html"))
                    .unwrap_or_else(|| row.get::<String, _>("snippet"));
                Ok(MySentSample {
                    message_id: decode_id(row.get::<String, _>("id").as_str())?,
                    thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
                    date: decode_timestamp(row.get("date"))?,
                    body,
                })
            })
            .collect()
    }

    /// The display name the user sends as: the most common From name on
    /// their recent outbound mail. `None` when they never set one.
    pub async fn my_display_name(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<String>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT from_name, COUNT(*) AS uses
               FROM (SELECT from_name FROM messages
                     WHERE account_id = ? AND direction = 'outbound'
                       AND from_name IS NOT NULL AND TRIM(from_name) != ''
                     ORDER BY date DESC LIMIT 200)
               GROUP BY from_name
               ORDER BY uses DESC
               LIMIT 1"#,
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        trace_query(
            "voice_samples.my_display_name",
            started_at,
            usize::from(row.is_some()),
        );
        Ok(row.map(|row| row.get::<String, _>("from_name").trim().to_string()))
    }
}

fn body_of(row: &sqlx::sqlite::SqliteRow, prefix: &str) -> String {
    row.get::<Option<String>, _>(format!("{prefix}_plain").as_str())
        .or_else(|| row.get::<Option<String>, _>(format!("{prefix}_html").as_str()))
        .unwrap_or_else(|| row.get::<String, _>(format!("{prefix}_snippet").as_str()))
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::Store;
    use chrono::TimeZone;
    use mxr_core::id::MessageId;
    use mxr_core::types::{Address, Envelope, MessageBody, MessageDirection, MessageMetadata};

    async fn message(
        store: &Store,
        account: &mxr_core::types::Account,
        from: (&str, &str),
        to: &str,
        header: &str,
        in_reply_to: Option<&str>,
        hour: u32,
        text: &str,
        direction: MessageDirection,
    ) -> Envelope {
        let mut env = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        env.id = MessageId::new();
        env.provider_id = header.to_string();
        env.from = Address {
            name: Some(from.0.to_string()),
            email: from.1.to_string(),
        };
        env.to = vec![Address {
            name: None,
            email: to.to_string(),
        }];
        env.message_id_header = Some(format!("<{header}>"));
        env.in_reply_to = in_reply_to.map(|parent| format!("<{parent}>"));
        env.date = chrono::Utc
            .with_ymd_and_hms(2026, 5, 1, hour, 0, 0)
            .unwrap();
        store
            .upsert_envelope_with_direction(&env, direction)
            .await
            .unwrap();
        store
            .insert_body(&MessageBody {
                message_id: env.id.clone(),
                text_plain: Some(text.to_string()),
                text_html: None,
                attachments: Vec::new(),
                fetched_at: env.date,
                metadata: MessageMetadata::default(),
            })
            .await
            .unwrap();
        if direction == MessageDirection::Outbound {
            store.try_create_reply_pair(&env, direction).await.unwrap();
        }
        env
    }

    #[tokio::test]
    async fn my_replies_pair_what_they_wrote_with_what_i_answered() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let me = ("Sam Rivers", "me@example.com");
        let alice = ("Alice", "alice@example.com");
        message(
            &store,
            &account,
            alice,
            me.1,
            "a1",
            None,
            9,
            "Lunch Friday?",
            MessageDirection::Inbound,
        )
        .await;
        let first = message(
            &store,
            &account,
            me,
            alice.1,
            "r1",
            Some("a1"),
            10,
            "yes! 1pm?",
            MessageDirection::Outbound,
        )
        .await;
        message(
            &store,
            &account,
            alice,
            me.1,
            "a2",
            None,
            11,
            "Can you review the deck?",
            MessageDirection::Inbound,
        )
        .await;
        message(
            &store,
            &account,
            me,
            alice.1,
            "r2",
            Some("a2"),
            12,
            "on it, by 5",
            MessageDirection::Outbound,
        )
        .await;

        let all = store
            .my_reply_samples(&account.id, Some("ALICE@example.com"), None, None, 10)
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].reply_body, "on it, by 5");
        assert_eq!(all[0].parent_body, "Can you review the deck?");
        assert_eq!(all[0].parent_from_name.as_deref(), Some("Alice"));

        // History as it stood before the second reply: only the first.
        let before = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let earlier = store
            .my_reply_samples(&account.id, None, None, Some(before), 10)
            .await
            .unwrap();
        assert_eq!(earlier.len(), 1);
        assert_eq!(earlier[0].reply_message_id, first.id);

        let sent = store
            .my_sent_to(&account.id, alice.1, None, None, 10)
            .await
            .unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            store.my_display_name(&account.id).await.unwrap().as_deref(),
            Some("Sam Rivers")
        );
    }
}
