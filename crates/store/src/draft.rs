use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentDraftReceipt {
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub local_message_id: MessageId,
    pub provider_message_id: Option<String>,
    pub rfc2822_message_id: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

impl super::Store {
    pub async fn insert_draft(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        let id = draft.id.as_str();
        let account_id = draft.account_id.as_str();
        let to_addrs = encode_json(&draft.to)?;
        let cc_addrs = encode_json(&draft.cc)?;
        let bcc_addrs = encode_json(&draft.bcc)?;
        let attachments = encode_json(&draft.attachments)?;
        let in_reply_to = draft.reply_headers.as_ref().map(encode_json).transpose()?;
        let intent = draft.intent.as_db_str();
        let inline_calendar_reply_json = draft
            .inline_calendar_reply
            .as_ref()
            .map(encode_json)
            .transpose()?;
        let created_at = draft.created_at.timestamp();
        let updated_at = draft.updated_at.timestamp();

        sqlx::query(
            "INSERT INTO drafts (id, account_id, in_reply_to, intent, to_addrs, cc_addrs, bcc_addrs, subject, body_markdown, attachments, inline_calendar_reply_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(in_reply_to)
        .bind(intent)
        .bind(to_addrs)
        .bind(cc_addrs)
        .bind(bcc_addrs)
        .bind(&draft.subject)
        .bind(&draft.body_markdown)
        .bind(attachments)
        .bind(inline_calendar_reply_json)
        .bind(created_at)
        .bind(updated_at)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn insert_draft_if_absent(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        let id = draft.id.as_str();
        let account_id = draft.account_id.as_str();
        let to_addrs = encode_json(&draft.to)?;
        let cc_addrs = encode_json(&draft.cc)?;
        let bcc_addrs = encode_json(&draft.bcc)?;
        let attachments = encode_json(&draft.attachments)?;
        let in_reply_to = draft.reply_headers.as_ref().map(encode_json).transpose()?;
        let intent = draft.intent.as_db_str();
        let inline_calendar_reply_json = draft
            .inline_calendar_reply
            .as_ref()
            .map(encode_json)
            .transpose()?;
        let created_at = draft.created_at.timestamp();
        let updated_at = draft.updated_at.timestamp();

        sqlx::query(
            "INSERT OR IGNORE INTO drafts (id, account_id, in_reply_to, intent, to_addrs, cc_addrs, bcc_addrs, subject, body_markdown, attachments, inline_calendar_reply_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(in_reply_to)
        .bind(intent)
        .bind(to_addrs)
        .bind(cc_addrs)
        .bind(bcc_addrs)
        .bind(&draft.subject)
        .bind(&draft.body_markdown)
        .bind(attachments)
        .bind(inline_calendar_reply_json)
        .bind(created_at)
        .bind(updated_at)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_draft(&self, id: &DraftId) -> Result<Option<Draft>, sqlx::Error> {
        let id_str = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT id, account_id, in_reply_to, intent,
                      to_addrs, cc_addrs, bcc_addrs, subject, body_markdown,
                      attachments, inline_calendar_reply_json,
                      created_at, updated_at
               FROM drafts WHERE id = ?"#,
        )
        .bind(id_str)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("draft.get_draft", started_at, row.is_some());

        row.map(|r| {
            Ok(Draft {
                id: decode_id(&r.get::<String, _>("id"))?,
                account_id: decode_id(&r.get::<String, _>("account_id"))?,
                reply_headers: parse_reply_headers(r.get::<Option<String>, _>("in_reply_to")),
                intent: DraftIntent::from_db_str(&r.get::<String, _>("intent")),
                to: decode_json(&r.get::<String, _>("to_addrs"))?,
                cc: decode_json(&r.get::<String, _>("cc_addrs"))?,
                bcc: decode_json(&r.get::<String, _>("bcc_addrs"))?,
                subject: r.get::<String, _>("subject"),
                body_markdown: r.get::<String, _>("body_markdown"),
                attachments: decode_json(&r.get::<String, _>("attachments"))?,
                inline_calendar_reply: r
                    .get::<Option<String>, _>("inline_calendar_reply_json")
                    .as_deref()
                    .map(decode_json)
                    .transpose()?,
                created_at: decode_timestamp(r.get::<i64, _>("created_at"))?,
                updated_at: decode_timestamp(r.get::<i64, _>("updated_at"))?,
            })
        })
        .transpose()
    }

    /// Update an existing draft in place, preserving `created_at`. Only rows
    /// still in `'draft'` status are editable — a draft mid-send (`'sending'`)
    /// or already `'sent'` is left untouched. Returns `true` when a row was
    /// updated, `false` when no editable draft with this id exists (caller
    /// distinguishes "not found" from "not editable" via `get_draft`).
    pub async fn update_draft(&self, draft: &Draft) -> Result<bool, sqlx::Error> {
        let id = draft.id.as_str();
        let account_id = draft.account_id.as_str();
        let to_addrs = encode_json(&draft.to)?;
        let cc_addrs = encode_json(&draft.cc)?;
        let bcc_addrs = encode_json(&draft.bcc)?;
        let attachments = encode_json(&draft.attachments)?;
        let in_reply_to = draft.reply_headers.as_ref().map(encode_json).transpose()?;
        let intent = draft.intent.as_db_str();
        let inline_calendar_reply_json = draft
            .inline_calendar_reply
            .as_ref()
            .map(encode_json)
            .transpose()?;
        let updated_at = draft.updated_at.timestamp();

        // `created_at` is intentionally absent from SET so the original
        // creation time survives the edit. Plain UPDATE (not INSERT OR
        // REPLACE) so the account_id FK's ON DELETE CASCADE is never armed.
        let result = sqlx::query(
            "UPDATE drafts
                SET account_id = ?, in_reply_to = ?, intent = ?,
                    to_addrs = ?, cc_addrs = ?, bcc_addrs = ?, subject = ?,
                    body_markdown = ?, attachments = ?, inline_calendar_reply_json = ?,
                    updated_at = ?
              WHERE id = ? AND status = 'draft'",
        )
        .bind(account_id)
        .bind(in_reply_to)
        .bind(intent)
        .bind(to_addrs)
        .bind(cc_addrs)
        .bind(bcc_addrs)
        .bind(&draft.subject)
        .bind(&draft.body_markdown)
        .bind(attachments)
        .bind(inline_calendar_reply_json)
        .bind(updated_at)
        .bind(id)
        .execute(self.writer())
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn list_drafts(&self, account_id: &AccountId) -> Result<Vec<Draft>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT id, account_id, in_reply_to, intent,
                      to_addrs, cc_addrs, bcc_addrs, subject, body_markdown,
                      attachments, inline_calendar_reply_json,
                      created_at, updated_at
               FROM drafts WHERE account_id = ? ORDER BY updated_at DESC"#,
        )
        .bind(aid)
        .fetch_all(self.reader())
        .await?;
        trace_query("draft.list_drafts", started_at, rows.len());

        rows.into_iter()
            .map(|r| {
                Ok(Draft {
                    id: decode_id(&r.get::<String, _>("id"))?,
                    account_id: decode_id(&r.get::<String, _>("account_id"))?,
                    reply_headers: parse_reply_headers(r.get::<Option<String>, _>("in_reply_to")),
                    intent: DraftIntent::from_db_str(&r.get::<String, _>("intent")),
                    to: decode_json(&r.get::<String, _>("to_addrs"))?,
                    cc: decode_json(&r.get::<String, _>("cc_addrs"))?,
                    bcc: decode_json(&r.get::<String, _>("bcc_addrs"))?,
                    subject: r.get::<String, _>("subject"),
                    body_markdown: r.get::<String, _>("body_markdown"),
                    attachments: decode_json(&r.get::<String, _>("attachments"))?,
                    inline_calendar_reply: r
                        .get::<Option<String>, _>("inline_calendar_reply_json")
                        .as_deref()
                        .map(decode_json)
                        .transpose()?,
                    created_at: decode_timestamp(r.get::<i64, _>("created_at"))?,
                    updated_at: decode_timestamp(r.get::<i64, _>("updated_at"))?,
                })
            })
            .collect()
    }

    pub async fn delete_draft(&self, id: &DraftId) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        sqlx::query!("DELETE FROM drafts WHERE id = ?", id_str)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Read the current send-pipeline status for a draft.
    pub async fn get_draft_status(&self, id: &DraftId) -> Result<Option<DraftStatus>, sqlx::Error> {
        let id_str = id.as_str();
        let row = sqlx::query!(
            r#"SELECT status as "status!" FROM drafts WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;
        Ok(row.and_then(|r| DraftStatus::from_db_str(&r.status)))
    }

    /// Read the persisted RFC 5322 Message-ID header for a draft, if any.
    pub async fn get_draft_message_id_header(
        &self,
        id: &DraftId,
    ) -> Result<Option<String>, sqlx::Error> {
        let id_str = id.as_str();
        let row = sqlx::query!(
            r#"SELECT message_id_header FROM drafts WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;
        Ok(row.and_then(|r| r.message_id_header))
    }

    /// Persist the RFC 5322 Message-ID header on the draft. Idempotent.
    pub async fn set_draft_message_id_header(
        &self,
        id: &DraftId,
        header: &str,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        sqlx::query!(
            "UPDATE drafts SET message_id_header = ? WHERE id = ?",
            header,
            id_str,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Atomically transition a draft's status from `expected` to `new`.
    /// Returns Ok(true) if the transition happened; Ok(false) if the row's
    /// current status didn't match (or the draft is missing).
    pub async fn cas_draft_status(
        &self,
        id: &DraftId,
        expected: DraftStatus,
        new: DraftStatus,
    ) -> Result<bool, sqlx::Error> {
        let id_str = id.as_str();
        let expected_str = expected.as_db_str();
        let new_str = new.as_db_str();
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query!(
            "UPDATE drafts SET status = ?, status_updated_at = ? WHERE id = ? AND status = ?",
            new_str,
            now,
            id_str,
            expected_str,
        )
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Unconditionally update a draft's status. Used by error-recovery paths
    /// that need to revert `Sending` → `Draft` after a send failure.
    pub async fn update_draft_status(
        &self,
        id: &DraftId,
        status: DraftStatus,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        let status_str = status.as_db_str();
        let now = chrono::Utc::now().timestamp();
        sqlx::query!(
            "UPDATE drafts SET status = ?, status_updated_at = ? WHERE id = ?",
            status_str,
            now,
            id_str,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn record_sent_draft_receipt(
        &self,
        draft_id: &DraftId,
        account_id: &AccountId,
        local_message_id: &MessageId,
        provider_message_id: Option<&str>,
        rfc2822_message_id: &str,
        sent_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR IGNORE INTO sent_draft_receipts
             (draft_id, account_id, local_message_id, provider_message_id, rfc2822_message_id, sent_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(draft_id.as_str())
        .bind(account_id.as_str())
        .bind(local_message_id.as_str())
        .bind(provider_message_id)
        .bind(rfc2822_message_id)
        .bind(sent_at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn get_sent_draft_receipt(
        &self,
        draft_id: &DraftId,
    ) -> Result<Option<SentDraftReceipt>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT draft_id, account_id, local_message_id,
                      provider_message_id, rfc2822_message_id, sent_at
               FROM sent_draft_receipts
               WHERE draft_id = ?"#,
        )
        .bind(draft_id.as_str())
        .fetch_optional(self.reader())
        .await?;

        row.map(|r| {
            Ok(SentDraftReceipt {
                draft_id: decode_id(&r.get::<String, _>("draft_id"))?,
                account_id: decode_id(&r.get::<String, _>("account_id"))?,
                local_message_id: decode_id(&r.get::<String, _>("local_message_id"))?,
                provider_message_id: r.get::<Option<String>, _>("provider_message_id"),
                rfc2822_message_id: r.get::<String, _>("rfc2822_message_id"),
                sent_at: decode_timestamp(r.get::<i64, _>("sent_at"))?,
            })
        })
        .transpose()
    }
}

fn parse_reply_headers(raw: Option<String>) -> Option<ReplyHeaders> {
    let raw = raw?;
    serde_json::from_str(&raw).ok().or_else(|| {
        if raw.trim().is_empty() {
            None
        } else {
            Some(ReplyHeaders {
                in_reply_to: raw,
                references: Vec::new(),
                thread_id: None,
            })
        }
    })
}
