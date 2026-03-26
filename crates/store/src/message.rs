use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use crate::mxr_store::{
    decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query,
};
use sqlx::Row;

pub(crate) fn future_date_cutoff_timestamp() -> i64 {
    (chrono::Utc::now() + chrono::Duration::days(1)).timestamp()
}

impl super::Store {
    pub async fn upsert_envelope(&self, envelope: &Envelope) -> Result<(), sqlx::Error> {
        let id = envelope.id.as_str();
        let account_id = envelope.account_id.as_str();
        let thread_id = envelope.thread_id.as_str();
        let from_name = envelope.from.name.as_deref();
        let to_addrs = encode_json(&envelope.to)?;
        let cc_addrs = encode_json(&envelope.cc)?;
        let bcc_addrs = encode_json(&envelope.bcc)?;
        let refs = encode_json(&envelope.references)?;
        let date = envelope.date.timestamp();
        let flags = envelope.flags.bits() as i64;
        let unsub = encode_json(&envelope.unsubscribe)?;
        let has_attachments = envelope.has_attachments;
        let size_bytes = envelope.size_bytes as i64;

        sqlx::query!(
            "INSERT INTO messages
             (id, account_id, provider_id, thread_id, message_id_header, in_reply_to,
              reference_headers, from_name, from_email, to_addrs, cc_addrs, bcc_addrs,
              subject, date, flags, snippet, has_attachments, size_bytes, unsubscribe_method)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                provider_id = excluded.provider_id,
                thread_id = excluded.thread_id,
                message_id_header = excluded.message_id_header,
                in_reply_to = excluded.in_reply_to,
                reference_headers = excluded.reference_headers,
                from_name = excluded.from_name,
                from_email = excluded.from_email,
                to_addrs = excluded.to_addrs,
                cc_addrs = excluded.cc_addrs,
                bcc_addrs = excluded.bcc_addrs,
                subject = excluded.subject,
                date = excluded.date,
                flags = excluded.flags,
                snippet = excluded.snippet,
                has_attachments = excluded.has_attachments,
                size_bytes = excluded.size_bytes,
                unsubscribe_method = excluded.unsubscribe_method",
            id,
            account_id,
            envelope.provider_id,
            thread_id,
            envelope.message_id_header,
            envelope.in_reply_to,
            refs,
            from_name,
            envelope.from.email,
            to_addrs,
            cc_addrs,
            bcc_addrs,
            envelope.subject,
            date,
            flags,
            envelope.snippet,
            has_attachments,
            size_bytes,
            unsub,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_envelope(&self, id: &MessageId) -> Result<Option<Envelope>, sqlx::Error> {
        let id_str = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
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
             FROM messages WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("message.get_envelope", started_at, row.is_some());

        row.map(|r| {
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
        .transpose()
    }

    pub async fn list_envelopes_by_label(
        &self,
        label_id: &LabelId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let lid = label_id.as_str();
        let cutoff = future_date_cutoff_timestamp();
        let lim = limit as i64;
        let off = offset as i64;
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT
                m.id as "id!", m.account_id as "account_id!", m.provider_id as "provider_id!",
                m.thread_id as "thread_id!", m.message_id_header, m.in_reply_to,
                m.reference_headers, m.from_name, m.from_email as "from_email!",
                m.to_addrs as "to_addrs!", m.cc_addrs as "cc_addrs!", m.bcc_addrs as "bcc_addrs!",
                m.subject as "subject!", m.date as "date!", m.flags as "flags!",
                m.snippet as "snippet!", m.has_attachments as "has_attachments!: bool",
                m.size_bytes as "size_bytes!", m.unsubscribe_method,
                COALESCE((
                    SELECT GROUP_CONCAT(labels.provider_id, char(31))
                    FROM message_labels
                    JOIN labels ON labels.id = message_labels.label_id
                    WHERE message_labels.message_id = m.id
                ), '') as "label_provider_ids!: String"
             FROM messages m
             JOIN message_labels ml ON m.id = ml.message_id
             WHERE ml.label_id = ?
             ORDER BY CASE WHEN m.date > ? THEN 0 ELSE m.date END DESC, m.id DESC
             LIMIT ? OFFSET ?"#,
            lid,
            cutoff,
            lim,
            off,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("message.list_envelopes_by_label", started_at, rows.len());

        rows.into_iter()
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
            .collect()
    }

    pub async fn list_envelopes_by_account(
        &self,
        account_id: &AccountId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let aid = account_id.as_str();
        let cutoff = future_date_cutoff_timestamp();
        let lim = limit as i64;
        let off = offset as i64;
        let started_at = std::time::Instant::now();
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
             WHERE account_id = ?
             ORDER BY CASE WHEN date > ? THEN 0 ELSE date END DESC, id DESC
             LIMIT ? OFFSET ?"#,
            aid,
            cutoff,
            lim,
            off,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("message.list_envelopes_by_account", started_at, rows.len());

        rows.into_iter()
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
            .collect()
    }

    pub async fn list_envelopes_by_ids(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = message_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            r#"SELECT
                m.id as id, m.account_id as account_id, m.provider_id as provider_id,
                m.thread_id as thread_id, m.message_id_header, m.in_reply_to,
                m.reference_headers, m.from_name, m.from_email as from_email,
                m.to_addrs as to_addrs, m.cc_addrs as cc_addrs, m.bcc_addrs as bcc_addrs,
                m.subject as subject, m.date as date, m.flags as flags,
                m.snippet as snippet, m.has_attachments as has_attachments,
                m.size_bytes as size_bytes, m.unsubscribe_method,
                COALESCE((
                    SELECT GROUP_CONCAT(labels.provider_id, char(31))
                    FROM message_labels
                    JOIN labels ON labels.id = message_labels.label_id
                    WHERE message_labels.message_id = m.id
                ), '') as label_provider_ids
             FROM messages m
             WHERE m.id IN ({})"#,
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&sql);
        for message_id in message_ids {
            query = query.bind(message_id.as_str());
        }

        let started_at = std::time::Instant::now();
        let rows = query.fetch_all(self.reader()).await?;
        trace_query("message.list_envelopes_by_ids", started_at, rows.len());
        let mut by_id = std::collections::HashMap::with_capacity(rows.len());
        for row in rows {
            let id = row.get::<String, _>(0);
            let account_id = row.get::<String, _>(1);
            let provider_id = row.get::<String, _>(2);
            let thread_id = row.get::<String, _>(3);
            let message_id_header = row.get::<Option<String>, _>(4);
            let in_reply_to = row.get::<Option<String>, _>(5);
            let reference_headers = row.get::<Option<String>, _>(6);
            let from_name = row.get::<Option<String>, _>(7);
            let from_email = row.get::<String, _>(8);
            let to_addrs = row.get::<String, _>(9);
            let cc_addrs = row.get::<String, _>(10);
            let bcc_addrs = row.get::<String, _>(11);
            let subject = row.get::<String, _>(12);
            let date = row.get::<i64, _>(13);
            let flags = row.get::<i64, _>(14);
            let snippet = row.get::<String, _>(15);
            let has_attachments = row.get::<bool, _>(16);
            let size_bytes = row.get::<i64, _>(17);
            let unsubscribe_method = row.get::<Option<String>, _>(18);
            let label_provider_ids = row.get::<String, _>(19);
            let envelope = record_to_envelope(
                &id,
                &account_id,
                &provider_id,
                &thread_id,
                message_id_header.as_deref(),
                in_reply_to.as_deref(),
                reference_headers.as_deref(),
                from_name.as_deref(),
                &from_email,
                &to_addrs,
                &cc_addrs,
                &bcc_addrs,
                &subject,
                date,
                flags,
                &snippet,
                has_attachments,
                size_bytes,
                unsubscribe_method.as_deref(),
                &label_provider_ids,
            )?;
            by_id.insert(envelope.id.clone(), envelope);
        }

        Ok(message_ids
            .iter()
            .filter_map(|message_id| by_id.remove(message_id))
            .collect())
    }

    // Dynamic SQL -- kept as runtime query due to variable IN clause
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
        let mid = message_id.as_str();
        sqlx::query!("DELETE FROM message_labels WHERE message_id = ?", mid)
            .execute(self.writer())
            .await?;

        for label_id in label_ids {
            let mid = message_id.as_str();
            let lid = label_id.as_str();
            sqlx::query!(
                "INSERT INTO message_labels (message_id, label_id) VALUES (?, ?)",
                mid,
                lid,
            )
            .execute(self.writer())
            .await?;
        }

        Ok(())
    }

    pub async fn update_message_thread_id(
        &self,
        message_id: &MessageId,
        thread_id: &ThreadId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE messages SET thread_id = ? WHERE id = ?")
            .bind(thread_id.as_str())
            .bind(message_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn get_message_id_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<MessageId>, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query!(
            r#"SELECT id as "id!" FROM messages WHERE account_id = ? AND provider_id = ?"#,
            aid,
            provider_id,
        )
        .fetch_optional(self.reader())
        .await?;

        row.map(|r| decode_id(&r.id)).transpose()
    }

    pub async fn count_messages_by_account(
        &self,
        account_id: &AccountId,
    ) -> Result<u32, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query!(
            r#"SELECT COUNT(*) as "cnt!: i64" FROM messages WHERE account_id = ?"#,
            aid,
        )
        .fetch_one(self.reader())
        .await?;

        Ok(row.cnt as u32)
    }

    /// List all envelopes across all accounts, paginated. Used for reindexing.
    pub async fn list_all_envelopes_paginated(
        &self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        let cutoff = future_date_cutoff_timestamp();
        let lim = limit as i64;
        let off = offset as i64;
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
             ORDER BY CASE WHEN date > ? THEN 0 ELSE date END DESC, id DESC
             LIMIT ? OFFSET ?"#,
            cutoff,
            lim,
            off,
        )
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
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
            .collect()
    }

    /// Count all messages across all accounts. Used for reindexing.
    pub async fn count_all_messages(&self) -> Result<u32, sqlx::Error> {
        let row = sqlx::query!(r#"SELECT COUNT(*) as "cnt!: i64" FROM messages"#)
            .fetch_one(self.reader())
            .await?;
        Ok(row.cnt as u32)
    }

    pub async fn update_flags(
        &self,
        message_id: &MessageId,
        flags: MessageFlags,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let flags_val = flags.bits() as i64;
        sqlx::query!("UPDATE messages SET flags = ? WHERE id = ?", flags_val, mid)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Set the read flag on a message.
    pub async fn set_read(&self, message_id: &MessageId, read: bool) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT flags as "flags!" FROM messages WHERE id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;

        if let Some(r) = row {
            let mut flags = MessageFlags::from_bits_truncate(r.flags as u32);
            if read {
                flags.insert(MessageFlags::READ);
            } else {
                flags.remove(MessageFlags::READ);
            }
            let flags_val = flags.bits() as i64;
            sqlx::query!("UPDATE messages SET flags = ? WHERE id = ?", flags_val, mid)
                .execute(self.writer())
                .await?;
        }
        Ok(())
    }

    /// Set the starred flag on a message.
    pub async fn set_starred(
        &self,
        message_id: &MessageId,
        starred: bool,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT flags as "flags!" FROM messages WHERE id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;

        if let Some(r) = row {
            let mut flags = MessageFlags::from_bits_truncate(r.flags as u32);
            if starred {
                flags.insert(MessageFlags::STARRED);
            } else {
                flags.remove(MessageFlags::STARRED);
            }
            let flags_val = flags.bits() as i64;
            sqlx::query!("UPDATE messages SET flags = ? WHERE id = ?", flags_val, mid)
                .execute(self.writer())
                .await?;
        }
        Ok(())
    }

    /// Get the provider_id for a message.
    pub async fn get_provider_id(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<String>, sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT provider_id as "provider_id!" FROM messages WHERE id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;
        Ok(row.map(|r| r.provider_id))
    }

    /// Get the label IDs for a message.
    pub async fn get_message_label_ids(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<LabelId>, sqlx::Error> {
        let mid = message_id.as_str();
        let rows = sqlx::query!(
            r#"SELECT label_id as "label_id!" FROM message_labels WHERE message_id = ?"#,
            mid,
        )
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|r| decode_id(&r.label_id)).collect()
    }

    /// Add a label to a message.
    pub async fn add_message_label(
        &self,
        message_id: &MessageId,
        label_id: &LabelId,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let lid = label_id.as_str();
        sqlx::query!(
            "INSERT OR IGNORE INTO message_labels (message_id, label_id) VALUES (?, ?)",
            mid,
            lid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Remove a label from a message.
    pub async fn remove_message_label(
        &self,
        message_id: &MessageId,
        label_id: &LabelId,
    ) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let lid = label_id.as_str();
        sqlx::query!(
            "DELETE FROM message_labels WHERE message_id = ? AND label_id = ?",
            mid,
            lid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Count total rows in the message_labels junction table.
    pub async fn count_message_labels(&self) -> Result<u32, sqlx::Error> {
        let row = sqlx::query!(r#"SELECT COUNT(*) as "cnt!: i64" FROM message_labels"#,)
            .fetch_one(self.reader())
            .await?;
        Ok(row.cnt as u32)
    }

    /// Mark a message as trashed (update flags).
    pub async fn move_to_trash(&self, message_id: &MessageId) -> Result<(), sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query!(
            r#"SELECT flags as "flags!" FROM messages WHERE id = ?"#,
            mid,
        )
        .fetch_optional(self.reader())
        .await?;

        if let Some(r) = row {
            let mut flags = MessageFlags::from_bits_truncate(r.flags as u32);
            flags.insert(MessageFlags::TRASH);
            let flags_val = flags.bits() as i64;
            sqlx::query!("UPDATE messages SET flags = ? WHERE id = ?", flags_val, mid)
                .execute(self.writer())
                .await?;
        }
        Ok(())
    }

    /// Get distinct contacts (name + email) from message senders, ordered by frequency.
    pub async fn list_contacts(&self, limit: u32) -> Result<Vec<(String, String)>, sqlx::Error> {
        let lim = limit as i64;
        let rows = sqlx::query_as::<_, (String, String)>(
            r#"SELECT
                COALESCE(from_name, '') as name,
                from_email as email
             FROM messages
             WHERE from_email != ''
             GROUP BY from_email
             ORDER BY COUNT(*) DESC
             LIMIT ?"#,
        )
        .bind(lim)
        .fetch_all(self.reader())
        .await?;
        Ok(rows)
    }

    pub async fn list_subscriptions(
        &self,
        account_id: Option<&AccountId>,
        limit: u32,
    ) -> Result<Vec<SubscriptionSummary>, sqlx::Error> {
        let none_unsubscribe = encode_json(&UnsubscribeMethod::None)?;
        let trash_flag = MessageFlags::TRASH.bits() as i64;
        let spam_flag = MessageFlags::SPAM.bits() as i64;
        let cutoff = future_date_cutoff_timestamp();
        let lim = limit as i64;
        let account_id_str = account_id.map(|id| id.to_string());

        let rows = sqlx::query(
            r#"WITH ranked AS (
                SELECT
                    id,
                    account_id,
                    provider_id,
                    thread_id,
                    from_name,
                    from_email,
                    subject,
                    snippet,
                    date,
                    flags,
                    has_attachments,
                    size_bytes,
                    unsubscribe_method,
                    COUNT(*) OVER (
                        PARTITION BY account_id, LOWER(from_email)
                    ) AS message_count,
                    ROW_NUMBER() OVER (
                        PARTITION BY account_id, LOWER(from_email)
                        ORDER BY CASE WHEN date > ? THEN 0 ELSE date END DESC, id DESC
                    ) AS rn
                FROM messages
                WHERE from_email != ''
                  AND unsubscribe_method IS NOT NULL
                  AND unsubscribe_method != ?
                  AND (flags & ?) = 0
                  AND (flags & ?) = 0
                  AND (? IS NULL OR account_id = ?)
            )
            SELECT
                id,
                account_id,
                provider_id,
                thread_id,
                from_name,
                from_email,
                subject,
                snippet,
                date,
                flags,
                has_attachments,
                size_bytes,
                unsubscribe_method,
                message_count
            FROM ranked
            WHERE rn = 1
            ORDER BY CASE WHEN date > ? THEN 0 ELSE date END DESC, id DESC
            LIMIT ?"#,
        )
        .bind(cutoff)
        .bind(none_unsubscribe)
        .bind(trash_flag)
        .bind(spam_flag)
        .bind(&account_id_str)
        .bind(&account_id_str)
        .bind(cutoff)
        .bind(lim)
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(SubscriptionSummary {
                    account_id: decode_id(&row.get::<String, _>("account_id"))?,
                    sender_name: row.get::<Option<String>, _>("from_name"),
                    sender_email: row.get::<String, _>("from_email"),
                    message_count: row.get::<i64, _>("message_count") as u32,
                    latest_message_id: decode_id(&row.get::<String, _>("id"))?,
                    latest_provider_id: row.get::<String, _>("provider_id"),
                    latest_thread_id: decode_id(&row.get::<String, _>("thread_id"))?,
                    latest_subject: row.get::<String, _>("subject"),
                    latest_snippet: row.get::<String, _>("snippet"),
                    latest_date: decode_timestamp(row.get::<i64, _>("date"))?,
                    latest_flags: MessageFlags::from_bits_truncate(
                        row.get::<i64, _>("flags") as u32
                    ),
                    latest_has_attachments: row.get::<bool, _>("has_attachments"),
                    latest_size_bytes: row.get::<i64, _>("size_bytes") as u64,
                    unsubscribe: row
                        .get::<Option<String>, _>("unsubscribe_method")
                        .as_deref()
                        .map(decode_json::<UnsubscribeMethod>)
                        .transpose()?
                        .unwrap_or(UnsubscribeMethod::None),
                })
            })
            .collect()
    }
}

/// Shared helper to convert individual field values into an Envelope.
/// Used by both message.rs and thread.rs queries since the `query!` macro
/// returns different anonymous types for each call site.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_to_envelope(
    id: &str,
    account_id: &str,
    provider_id: &str,
    thread_id: &str,
    message_id_header: Option<&str>,
    in_reply_to: Option<&str>,
    reference_headers: Option<&str>,
    from_name: Option<&str>,
    from_email: &str,
    to_addrs: &str,
    cc_addrs: &str,
    bcc_addrs: &str,
    subject: &str,
    date: i64,
    flags: i64,
    snippet: &str,
    has_attachments: bool,
    size_bytes: i64,
    unsubscribe_method: Option<&str>,
    label_provider_ids: &str,
) -> Result<Envelope, sqlx::Error> {
    Ok(Envelope {
        id: decode_id(id)?,
        account_id: decode_id(account_id)?,
        provider_id: provider_id.to_string(),
        thread_id: decode_id(thread_id)?,
        message_id_header: message_id_header.map(|s| s.to_string()),
        in_reply_to: in_reply_to.map(|s| s.to_string()),
        references: reference_headers
            .map(decode_json::<Vec<String>>)
            .transpose()?
            .unwrap_or_default(),
        from: Address {
            name: from_name.map(|s| s.to_string()),
            email: from_email.to_string(),
        },
        to: decode_json(to_addrs)?,
        cc: decode_json(cc_addrs)?,
        bcc: decode_json(bcc_addrs)?,
        subject: subject.to_string(),
        date: decode_timestamp(date)?,
        flags: MessageFlags::from_bits_truncate(flags as u32),
        snippet: snippet.to_string(),
        has_attachments,
        size_bytes: size_bytes as u64,
        unsubscribe: unsubscribe_method
            .map(decode_json::<UnsubscribeMethod>)
            .transpose()?
            .unwrap_or(UnsubscribeMethod::None),
        label_provider_ids: if label_provider_ids.is_empty() {
            vec![]
        } else {
            label_provider_ids
                .split('\u{1f}')
                .filter(|provider_id| !provider_id.is_empty())
                .map(str::to_string)
                .collect()
        },
    })
}
