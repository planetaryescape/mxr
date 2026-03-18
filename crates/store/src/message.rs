use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn upsert_envelope(&self, envelope: &Envelope) -> Result<(), sqlx::Error> {
        let id = envelope.id.as_str();
        let account_id = envelope.account_id.as_str();
        let thread_id = envelope.thread_id.as_str();
        let from_name = envelope.from.name.as_deref();
        let to_addrs = serde_json::to_string(&envelope.to).unwrap();
        let cc_addrs = serde_json::to_string(&envelope.cc).unwrap();
        let bcc_addrs = serde_json::to_string(&envelope.bcc).unwrap();
        let refs = serde_json::to_string(&envelope.references).unwrap();
        let date = envelope.date.timestamp();
        let flags = envelope.flags.bits() as i64;
        let unsub = serde_json::to_string(&envelope.unsubscribe).unwrap();

        sqlx::query(
            "INSERT OR REPLACE INTO messages
             (id, account_id, provider_id, thread_id, message_id_header, in_reply_to,
              reference_headers, from_name, from_email, to_addrs, cc_addrs, bcc_addrs,
              subject, date, flags, snippet, has_attachments, size_bytes, unsubscribe_method)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&account_id)
        .bind(&envelope.provider_id)
        .bind(&thread_id)
        .bind(&envelope.message_id_header)
        .bind(&envelope.in_reply_to)
        .bind(&refs)
        .bind(from_name)
        .bind(&envelope.from.email)
        .bind(&to_addrs)
        .bind(&cc_addrs)
        .bind(&bcc_addrs)
        .bind(&envelope.subject)
        .bind(date)
        .bind(flags)
        .bind(&envelope.snippet)
        .bind(envelope.has_attachments)
        .bind(envelope.size_bytes as i64)
        .bind(&unsub)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_envelope(&self, id: &MessageId) -> Result<Option<Envelope>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM messages WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(self.reader())
            .await?;

        match row {
            Some(row) => Ok(Some(row_to_envelope(&row))),
            None => Ok(None),
        }
    }

    pub async fn list_envelopes_by_label(
        &self,
        label_id: &LabelId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT m.* FROM messages m
             JOIN message_labels ml ON m.id = ml.message_id
             WHERE ml.label_id = ?
             ORDER BY m.date DESC
             LIMIT ? OFFSET ?",
        )
        .bind(label_id.as_str())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.reader())
        .await?;

        Ok(rows.iter().map(row_to_envelope).collect())
    }

    pub async fn list_envelopes_by_account(
        &self,
        account_id: &AccountId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT * FROM messages WHERE account_id = ? ORDER BY date DESC LIMIT ? OFFSET ?",
        )
        .bind(account_id.as_str())
        .bind(limit)
        .bind(offset)
        .fetch_all(self.reader())
        .await?;

        Ok(rows.iter().map(row_to_envelope).collect())
    }

    pub async fn delete_messages_by_provider_ids(
        &self,
        account_id: &AccountId,
        provider_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if provider_ids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = provider_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM messages WHERE account_id = ? AND provider_id IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql).bind(account_id.as_str());
        for pid in provider_ids {
            query = query.bind(pid);
        }
        let result = query.execute(self.writer()).await?;
        Ok(result.rows_affected())
    }

    pub async fn set_message_labels(
        &self,
        message_id: &MessageId,
        label_ids: &[LabelId],
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM message_labels WHERE message_id = ?")
            .bind(message_id.as_str())
            .execute(self.writer())
            .await?;

        for label_id in label_ids {
            sqlx::query("INSERT INTO message_labels (message_id, label_id) VALUES (?, ?)")
                .bind(message_id.as_str())
                .bind(label_id.as_str())
                .execute(self.writer())
                .await?;
        }

        Ok(())
    }

    pub async fn update_flags(
        &self,
        message_id: &MessageId,
        flags: MessageFlags,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE messages SET flags = ? WHERE id = ?")
            .bind(flags.bits() as i64)
            .bind(message_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
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
