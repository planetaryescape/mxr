//! What a sender's own recent mail says about them: the subjects of their
//! latest messages and whether their latest one came through bulk-mail
//! infrastructure. Mail kinds read it to tell a person from a machine that
//! writes like one (a bank's "Payment Confirmation" forty times over).

use crate::trace_query;
use mxr_core::id::AccountId;
use sqlx::Row;
use std::collections::HashMap;
use std::time::Instant;

/// How many of a sender's latest subjects are read.
pub const SENDER_HISTORY_SUBJECTS: i64 = 20;

/// Headers that bulk-mail services and marketing platforms add and people's
/// mail clients don't: SendGrid, Mailgun, Amazon SES, Salesforce Marketing
/// Cloud, Mailchimp/Mandrill, HubSpot, Marketo, Gmail's sender feedback
/// loop, and the bulk precedence and auto-generated markers.
const BULK_HEADER_MARKERS: &[&str] = &[
    "Feedback-ID:",
    "X-SG-EID:",
    "X-Mailgun-",
    "X-SES-Outgoing:",
    "X-SFMC-",
    "X-Campaign",
    "X-CSA-Complaints:",
    "X-MC-User:",
    "X-Mandrill-",
    "X-Marketo",
    "X-HS-",
    "Precedence: bulk",
    "Precedence: list",
    "Auto-Submitted: auto-generated",
];

/// One sender's recent inbound mail, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SenderHistory {
    /// Up to [`SENDER_HISTORY_SUBJECTS`] subjects, newest first.
    pub subjects: Vec<String>,
    /// Their latest message carries a bulk-mail header.
    pub bulk_headers: bool,
}

impl super::Store {
    /// Recent inbound history for each of `senders`, keyed by lowercased
    /// address. Pass addresses as the messages spell them: the lookup uses
    /// the sender index, which is case-sensitive.
    pub async fn sender_histories(
        &self,
        account_id: &AccountId,
        senders: &[String],
    ) -> Result<HashMap<String, SenderHistory>, sqlx::Error> {
        if senders.is_empty() {
            return Ok(HashMap::new());
        }
        let started_at = Instant::now();
        let wanted =
            serde_json::to_string(senders).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        // The unary plus keeps the planner on (from_email, date): the
        // account/direction index would walk the whole mailbox per sender.
        let rows = sqlx::query(
            r#"SELECT w.value AS sender, m.id, m.subject, m.date
               FROM json_each(?2) w
               CROSS JOIN messages m ON m.id IN (
                   SELECT r.id FROM messages r
                   WHERE r.from_email = w.value
                     AND +r.account_id = ?1
                     AND +r.direction = 'inbound'
                   ORDER BY r.date DESC
                   LIMIT ?3
               )
               ORDER BY w.value, m.date DESC"#,
        )
        .bind(account_id.as_str())
        .bind(&wanted)
        .bind(SENDER_HISTORY_SUBJECTS)
        .fetch_all(self.reader())
        .await?;
        let mut histories: HashMap<String, SenderHistory> = HashMap::new();
        let mut latest: HashMap<String, (i64, String)> = HashMap::new();
        for row in &rows {
            let sender = row.try_get::<String, _>("sender")?.to_ascii_lowercase();
            let date: i64 = row.try_get("date")?;
            let id: String = row.try_get("id")?;
            histories
                .entry(sender.clone())
                .or_default()
                .subjects
                .push(row.try_get("subject")?);
            let newest = latest.entry(sender).or_insert((date, id.clone()));
            if date > newest.0 {
                *newest = (date, id);
            }
        }
        if !latest.is_empty() {
            let ids: Vec<&str> = latest.values().map(|(_, id)| id.as_str()).collect();
            let ids = serde_json::to_string(&ids).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
            let markers = BULK_HEADER_MARKERS
                .iter()
                .map(|_| "b.metadata_json LIKE ?")
                .collect::<Vec<_>>()
                .join(" OR ");
            let sql = format!(
                "SELECT b.message_id FROM bodies b
                 WHERE b.message_id IN (SELECT value FROM json_each(?1)) AND ({markers})"
            );
            let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(ids);
            for marker in BULK_HEADER_MARKERS {
                query = query.bind(format!("%{marker}%"));
            }
            let bulk: std::collections::HashSet<String> =
                query.fetch_all(self.reader()).await?.into_iter().collect();
            for (sender, (_, id)) in &latest {
                if bulk.contains(id) {
                    if let Some(history) = histories.get_mut(sender) {
                        history.bulk_headers = true;
                    }
                }
            }
        }
        trace_query("sender.histories", started_at, rows.len());
        Ok(histories)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;
    use crate::Store;
    use chrono::{Duration, Utc};
    use mxr_core::types::{MessageBody, MessageDirection, MessageMetadata};

    #[tokio::test]
    async fn reads_a_senders_latest_subjects_and_bulk_headers() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        for (index, (from, subject, headers)) in [
            ("Alerts@Bank.example", "Payment Confirmation", ""),
            (
                "Alerts@Bank.example",
                "Foreign Payment",
                "Received: x\r\nX-SG-EID: abc\r\n",
            ),
            ("maya@orbit.example", "Dinner?", "Received: x\r\n"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut envelope = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            envelope.provider_id = format!("history-{index}");
            envelope.from.email = from.to_string();
            envelope.subject = subject.to_string();
            envelope.date = now - Duration::hours(10 - index as i64);
            store
                .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
                .await
                .unwrap();
            store
                .insert_body(&MessageBody {
                    message_id: envelope.id.clone(),
                    text_plain: Some("body".into()),
                    text_html: None,
                    attachments: vec![],
                    fetched_at: now,
                    metadata: MessageMetadata {
                        raw_headers: Some(headers.to_string()),
                        ..MessageMetadata::default()
                    },
                })
                .await
                .unwrap();
        }
        let histories = store
            .sender_histories(
                &account.id,
                &["Alerts@Bank.example".into(), "maya@orbit.example".into()],
            )
            .await
            .unwrap();
        let bank = &histories["alerts@bank.example"];
        assert_eq!(
            bank.subjects,
            vec!["Foreign Payment", "Payment Confirmation"]
        );
        assert!(bank.bulk_headers);
        let maya = &histories["maya@orbit.example"];
        assert_eq!(maya.subjects, vec!["Dinner?"]);
        assert!(!maya.bulk_headers);
    }
}
