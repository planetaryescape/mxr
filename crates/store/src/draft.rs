use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    pub async fn insert_draft(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        let id = draft.id.as_str();
        let account_id = draft.account_id.as_str();
        let to_addrs = encode_json(&draft.to)?;
        let cc_addrs = encode_json(&draft.cc)?;
        let bcc_addrs = encode_json(&draft.bcc)?;
        let attachments = encode_json(&draft.attachments)?;
        let in_reply_to = draft.reply_headers.as_ref().map(encode_json).transpose()?;
        let created_at = draft.created_at.timestamp();
        let updated_at = draft.updated_at.timestamp();

        sqlx::query!(
            "INSERT INTO drafts (id, account_id, in_reply_to, to_addrs, cc_addrs, bcc_addrs, subject, body_markdown, attachments, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            account_id,
            in_reply_to,
            to_addrs,
            cc_addrs,
            bcc_addrs,
            draft.subject,
            draft.body_markdown,
            attachments,
            created_at,
            updated_at,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_draft(&self, id: &DraftId) -> Result<Option<Draft>, sqlx::Error> {
        let id_str = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!", in_reply_to,
                      to_addrs as "to_addrs!", cc_addrs as "cc_addrs!", bcc_addrs as "bcc_addrs!",
                      subject as "subject!", body_markdown as "body_markdown!",
                      attachments as "attachments!", created_at as "created_at!", updated_at as "updated_at!"
               FROM drafts WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("draft.get_draft", started_at, row.is_some());

        row.map(|r| {
            Ok(Draft {
                id: decode_id(&r.id)?,
                account_id: decode_id(&r.account_id)?,
                reply_headers: parse_reply_headers(r.in_reply_to),
                to: decode_json(&r.to_addrs)?,
                cc: decode_json(&r.cc_addrs)?,
                bcc: decode_json(&r.bcc_addrs)?,
                subject: r.subject,
                body_markdown: r.body_markdown,
                attachments: decode_json(&r.attachments)?,
                created_at: decode_timestamp(r.created_at)?,
                updated_at: decode_timestamp(r.updated_at)?,
            })
        })
        .transpose()
    }

    pub async fn list_drafts(&self, account_id: &AccountId) -> Result<Vec<Draft>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!", in_reply_to,
                      to_addrs as "to_addrs!", cc_addrs as "cc_addrs!", bcc_addrs as "bcc_addrs!",
                      subject as "subject!", body_markdown as "body_markdown!",
                      attachments as "attachments!", created_at as "created_at!", updated_at as "updated_at!"
               FROM drafts WHERE account_id = ? ORDER BY updated_at DESC"#,
            aid,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("draft.list_drafts", started_at, rows.len());

        rows.into_iter()
            .map(|r| {
                Ok(Draft {
                    id: decode_id(&r.id)?,
                    account_id: decode_id(&r.account_id)?,
                    reply_headers: parse_reply_headers(r.in_reply_to),
                    to: decode_json(&r.to_addrs)?,
                    cc: decode_json(&r.cc_addrs)?,
                    bcc: decode_json(&r.bcc_addrs)?,
                    subject: r.subject,
                    body_markdown: r.body_markdown,
                    attachments: decode_json(&r.attachments)?,
                    created_at: decode_timestamp(r.created_at)?,
                    updated_at: decode_timestamp(r.updated_at)?,
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
