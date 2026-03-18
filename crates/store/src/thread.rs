use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn get_thread(&self, thread_id: &ThreadId) -> Result<Option<Thread>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT
                thread_id,
                account_id,
                MIN(subject) as subject,
                COUNT(*) as message_count,
                SUM(CASE WHEN (flags & 1) = 0 THEN 1 ELSE 0 END) as unread_count,
                MAX(date) as latest_date,
                snippet
             FROM messages
             WHERE thread_id = ?
             GROUP BY thread_id",
        )
        .bind(thread_id.as_str())
        .fetch_optional(self.reader())
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let thread_id_str: String = row.get("thread_id");
        let account_id_str: String = row.get("account_id");
        let latest_date_ts: i64 = row.get("latest_date");
        let message_count: i32 = row.get("message_count");
        let unread_count: i32 = row.get("unread_count");

        // Get participants
        let participant_rows =
            sqlx::query("SELECT DISTINCT from_name, from_email FROM messages WHERE thread_id = ?")
                .bind(thread_id.as_str())
                .fetch_all(self.reader())
                .await?;

        let participants: Vec<Address> = participant_rows
            .iter()
            .map(|r| Address {
                name: r.get("from_name"),
                email: r.get("from_email"),
            })
            .collect();

        Ok(Some(Thread {
            id: ThreadId::from_uuid(uuid::Uuid::parse_str(&thread_id_str).unwrap()),
            account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id_str).unwrap()),
            subject: row.get("subject"),
            participants,
            message_count: message_count as u32,
            unread_count: unread_count as u32,
            latest_date: chrono::DateTime::from_timestamp(latest_date_ts, 0).unwrap_or_default(),
            snippet: row.get("snippet"),
        }))
    }

    pub async fn get_thread_envelopes(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM messages WHERE thread_id = ? ORDER BY date ASC")
            .bind(thread_id.as_str())
            .fetch_all(self.reader())
            .await?;

        // Reuse row_to_envelope from message module — but it's private there.
        // We'll just duplicate the conversion inline since it's the same table.
        Ok(rows.iter().map(row_to_envelope).collect())
    }
}

fn row_to_envelope(row: &sqlx::sqlite::SqliteRow) -> Envelope {
    let id_str: String = row.get("id");
    let account_id_str: String = row.get("account_id");
    let thread_id_str: String = row.get("thread_id");
    let from_name: Option<String> = row.get("from_name");
    let from_email: String = row.get("from_email");
    let to_json: String = row.get("to_addrs");
    let cc_json: String = row.get("cc_addrs");
    let bcc_json: String = row.get("bcc_addrs");
    let refs_json: Option<String> = row.get("reference_headers");
    let date_ts: i64 = row.get("date");
    let flags_bits: i64 = row.get("flags");
    let has_attachments: bool = row.get("has_attachments");
    let size_bytes: i64 = row.get("size_bytes");
    let unsub_json: Option<String> = row.get("unsubscribe_method");

    Envelope {
        id: MessageId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
        account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id_str).unwrap()),
        provider_id: row.get("provider_id"),
        thread_id: ThreadId::from_uuid(uuid::Uuid::parse_str(&thread_id_str).unwrap()),
        message_id_header: row.get("message_id_header"),
        in_reply_to: row.get("in_reply_to"),
        references: refs_json
            .map(|r| serde_json::from_str(&r).unwrap_or_default())
            .unwrap_or_default(),
        from: Address {
            name: from_name,
            email: from_email,
        },
        to: serde_json::from_str(&to_json).unwrap_or_default(),
        cc: serde_json::from_str(&cc_json).unwrap_or_default(),
        bcc: serde_json::from_str(&bcc_json).unwrap_or_default(),
        subject: row.get("subject"),
        date: chrono::DateTime::from_timestamp(date_ts, 0).unwrap_or_default(),
        flags: MessageFlags::from_bits_truncate(flags_bits as u32),
        snippet: row.get("snippet"),
        has_attachments,
        size_bytes: size_bytes as u64,
        unsubscribe: unsub_json
            .map(|u| serde_json::from_str(&u).unwrap_or(UnsubscribeMethod::None))
            .unwrap_or(UnsubscribeMethod::None),
    }
}
