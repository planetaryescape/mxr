use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn get_body(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<MessageBody>, sqlx::Error> {
        let mid = message_id.as_str();
        let row = sqlx::query(
            r#"SELECT message_id, text_plain, text_html, fetched_at, metadata_json FROM bodies WHERE message_id = ?"#,
        )
        .bind(mid)
        .fetch_optional(self.reader())
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let att_mid = message_id.as_str();
        let attachments_rows = sqlx::query!(
            r#"SELECT id as "id!", message_id as "message_id!", filename as "filename!", mime_type as "mime_type!", size_bytes as "size_bytes!", local_path, provider_id as "provider_id!" FROM attachments WHERE message_id = ?"#,
            att_mid,
        )
        .fetch_all(self.reader())
        .await?;

        let attachments: Vec<AttachmentMeta> = attachments_rows
            .into_iter()
            .map(|r| AttachmentMeta {
                id: AttachmentId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
                message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&r.message_id).unwrap()),
                filename: r.filename,
                mime_type: r.mime_type,
                size_bytes: r.size_bytes as u64,
                local_path: r.local_path.map(std::path::PathBuf::from),
                provider_id: r.provider_id,
            })
            .collect();

        let metadata_json: String = row.try_get("metadata_json")?;
        Ok(Some(MessageBody {
            message_id: MessageId::from_uuid(
                uuid::Uuid::parse_str(row.try_get::<&str, _>("message_id")?).unwrap(),
            ),
            text_plain: row.try_get("text_plain")?,
            text_html: row.try_get("text_html")?,
            attachments,
            fetched_at: chrono::DateTime::from_timestamp(row.try_get("fetched_at")?, 0)
                .unwrap_or_default(),
            metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        }))
    }

    pub async fn insert_body(&self, body: &MessageBody) -> Result<(), sqlx::Error> {
        let fetched_at = body.fetched_at.timestamp();
        let mid = body.message_id.as_str();
        let metadata_json = serde_json::to_string(&body.metadata).unwrap_or_else(|_| "{}".into());

        sqlx::query(
            "INSERT OR REPLACE INTO bodies (message_id, text_plain, text_html, fetched_at, metadata_json) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(mid)
        .bind(&body.text_plain)
        .bind(&body.text_html)
        .bind(fetched_at)
        .bind(metadata_json)
        .execute(self.writer())
        .await?;

        for att in &body.attachments {
            let att_id = att.id.as_str();
            let att_mid = att.message_id.as_str();
            let local_path = att
                .local_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());
            let size_bytes = att.size_bytes as i64;
            sqlx::query!(
                "INSERT OR REPLACE INTO attachments (id, message_id, filename, mime_type, size_bytes, local_path, provider_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                att_id,
                att_mid,
                att.filename,
                att.mime_type,
                size_bytes,
                local_path,
                att.provider_id,
            )
            .execute(self.writer())
            .await?;
        }

        Ok(())
    }
}
