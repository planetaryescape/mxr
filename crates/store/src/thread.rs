use mxr_core::id::*;
use mxr_core::types::*;

use crate::message::{future_date_cutoff_timestamp, record_to_envelope};

impl super::Store {
    pub async fn get_thread(&self, thread_id: &ThreadId) -> Result<Option<Thread>, sqlx::Error> {
        let tid = thread_id.as_str();
        let cutoff = future_date_cutoff_timestamp();
        let row = sqlx::query!(
            r#"SELECT
                thread_id as "thread_id!",
                account_id as "account_id!",
                MIN(subject) as "subject!: String",
                COUNT(*) as "message_count!: i64",
                SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END) as "unread_count!: i64",
                MAX(CASE WHEN date > ? THEN 0 ELSE date END) as "latest_date!: i64",
                snippet as "snippet!"
             FROM messages
             WHERE thread_id = ?
             GROUP BY thread_id"#,
            cutoff,
            tid,
        )
        .fetch_optional(self.reader())
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        // Get participants
        let tid2 = thread_id.as_str();
        let participant_rows = sqlx::query!(
            r#"SELECT DISTINCT from_name, from_email as "from_email!" FROM messages WHERE thread_id = ?"#,
            tid2,
        )
        .fetch_all(self.reader())
        .await?;

        let participants: Vec<Address> = participant_rows
            .into_iter()
            .map(|r| Address {
                name: r.from_name,
                email: r.from_email,
            })
            .collect();

        Ok(Some(Thread {
            id: ThreadId::from_uuid(uuid::Uuid::parse_str(&row.thread_id).unwrap()),
            account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&row.account_id).unwrap()),
            subject: row.subject,
            participants,
            message_count: row.message_count as u32,
            unread_count: row.unread_count as u32,
            latest_date: chrono::DateTime::from_timestamp(row.latest_date, 0).unwrap_or_default(),
            snippet: row.snippet,
        }))
    }

    pub async fn get_thread_envelopes(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let tid = thread_id.as_str();
        let cutoff = future_date_cutoff_timestamp();
        let rows = sqlx::query!(
            r#"SELECT
                id as "id!", account_id as "account_id!", provider_id as "provider_id!",
                thread_id as "thread_id!", message_id_header, in_reply_to,
                reference_headers, from_name, from_email as "from_email!",
                to_addrs as "to_addrs!", cc_addrs as "cc_addrs!", bcc_addrs as "bcc_addrs!",
                subject as "subject!", date as "date!", flags as "flags!",
                snippet as "snippet!", has_attachments as "has_attachments!: bool",
                size_bytes as "size_bytes!", unsubscribe_method,
                COALESCE((
                    SELECT GROUP_CONCAT(labels.provider_id, char(31))
                    FROM message_labels
                    JOIN labels ON labels.id = message_labels.label_id
                    WHERE message_labels.message_id = messages.id
                ), '') as "label_provider_ids!: String"
             FROM messages
             WHERE thread_id = ?
             ORDER BY CASE WHEN date > ? THEN 0 ELSE date END ASC, id ASC"#,
            tid,
            cutoff,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                record_to_envelope(
                    &r.id,
                    &r.account_id,
                    &r.provider_id,
                    &r.thread_id,
                    r.message_id_header.as_deref(),
                    r.in_reply_to.as_deref(),
                    r.reference_headers.as_deref(),
                    r.from_name.as_deref(),
                    &r.from_email,
                    &r.to_addrs,
                    &r.cc_addrs,
                    &r.bcc_addrs,
                    &r.subject,
                    r.date,
                    r.flags,
                    &r.snippet,
                    r.has_attachments,
                    r.size_bytes,
                    r.unsubscribe_method.as_deref(),
                    &r.label_provider_ids,
                )
            })
            .collect())
    }
}
