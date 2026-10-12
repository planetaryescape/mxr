use crate::{decode_json, encode_json, Store};
use mxr_core::{DraftAttachment, DraftAttachmentId};
use sqlx::Row;

impl Store {
    pub async fn insert_draft_attachment(
        &self,
        metadata: &DraftAttachment,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO draft_attachment_resources (id, draft_id, account_id, metadata_json, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(metadata.id.as_str()).bind(metadata.draft_id.as_str())
            .bind(metadata.account_id.as_str()).bind(encode_json(metadata)?)
            .bind(chrono::Utc::now().timestamp()).execute(self.writer()).await?;
        Ok(())
    }

    pub async fn get_draft_attachment(
        &self,
        id: &DraftAttachmentId,
    ) -> Result<Option<(DraftAttachment, bool)>, sqlx::Error> {
        let row =
            sqlx::query("SELECT metadata_json, ready FROM draft_attachment_resources WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(self.reader())
                .await?;
        row.map(|row| {
            Ok((
                decode_json(&row.get::<String, _>("metadata_json"))?,
                row.get::<bool, _>("ready"),
            ))
        })
        .transpose()
    }

    pub async fn finalize_draft_attachment(
        &self,
        id: &DraftAttachmentId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE draft_attachment_resources SET ready = 1, updated_at = ? WHERE id = ?")
            .bind(chrono::Utc::now().timestamp())
            .bind(id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Bounded candidates only; the daemon rechecks draft ownership under its operation guard.
    pub async fn stale_draft_attachments(
        &self,
        cutoff: i64,
    ) -> Result<Vec<DraftAttachment>, sqlx::Error> {
        let rows = sqlx::query("SELECT metadata_json FROM draft_attachment_resources WHERE updated_at < ? AND NOT EXISTS (SELECT 1 FROM drafts WHERE drafts.id = draft_attachment_resources.draft_id) AND NOT EXISTS (SELECT 1 FROM draft_send_outcomes WHERE draft_send_outcomes.draft_id = draft_attachment_resources.draft_id AND (state = 'unknown' OR (state = 'accepted' AND local_message_id IS NULL))) ORDER BY updated_at LIMIT 32")
            .bind(cutoff).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|row| decode_json(&row.get::<String, _>("metadata_json")))
            .collect()
    }

    pub async fn delete_draft_attachment(&self, id: &DraftAttachmentId) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM draft_attachment_resources WHERE id = ?")
            .bind(id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
