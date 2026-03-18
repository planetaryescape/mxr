use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn get_body(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<MessageBody>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM bodies WHERE message_id = ?")
            .bind(message_id.as_str())
            .fetch_optional(self.reader())
            .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let msg_id_str: String = row.get("message_id");
        let fetched_at_ts: i64 = row.get("fetched_at");

        let attachments_rows = sqlx::query("SELECT * FROM attachments WHERE message_id = ?")
            .bind(message_id.as_str())
            .fetch_all(self.reader())
            .await?;

        let attachments: Vec<AttachmentMeta> = attachments_rows
            .iter()
            .map(|r| {
                let id_str: String = r.get("id");
                let mid_str: String = r.get("message_id");
                let size: i64 = r.get("size_bytes");
                let local_path: Option<String> = r.get("local_path");
                AttachmentMeta {
                    id: AttachmentId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
                    message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&mid_str).unwrap()),
                    filename: r.get("filename"),
                    mime_type: r.get("mime_type"),
                    size_bytes: size as u64,
                    local_path: local_path.map(std::path::PathBuf::from),
                    provider_id: r.get("provider_id"),
                }
            })
            .collect();

        Ok(Some(MessageBody {
            message_id: MessageId::from_uuid(uuid::Uuid::parse_str(&msg_id_str).unwrap()),
            text_plain: row.get("text_plain"),
            text_html: row.get("text_html"),
            attachments,
            fetched_at: chrono::DateTime::from_timestamp(fetched_at_ts, 0).unwrap_or_default(),
        }))
    }

    pub async fn insert_body(&self, body: &MessageBody) -> Result<(), sqlx::Error> {
        let fetched_at = body.fetched_at.timestamp();

        sqlx::query(
            "INSERT OR REPLACE INTO bodies (message_id, text_plain, text_html, fetched_at) VALUES (?, ?, ?, ?)",
        )
        .bind(body.message_id.as_str())
        .bind(&body.text_plain)
        .bind(&body.text_html)
        .bind(fetched_at)
        .execute(self.writer())
        .await?;

        for att in &body.attachments {
            let local_path = att
                .local_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());
            sqlx::query(
                "INSERT OR REPLACE INTO attachments (id, message_id, filename, mime_type, size_bytes, local_path, provider_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(att.id.as_str())
            .bind(att.message_id.as_str())
            .bind(&att.filename)
            .bind(&att.mime_type)
            .bind(att.size_bytes as i64)
            .bind(&local_path)
            .bind(&att.provider_id)
            .execute(self.writer())
            .await?;
        }

        Ok(())
    }
}
