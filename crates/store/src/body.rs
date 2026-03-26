use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use crate::mxr_store::{
    decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query,
};
use sqlx::Row;

impl super::Store {
    pub async fn get_body(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<MessageBody>, sqlx::Error> {
        let mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT message_id, text_plain, text_html, fetched_at, metadata_json FROM bodies WHERE message_id = ?"#,
        )
        .bind(mid)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("body.get_body", started_at, row.is_some());

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let att_mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let attachments_rows = sqlx::query!(
            r#"SELECT id as "id!", message_id as "message_id!", filename as "filename!", mime_type as "mime_type!", size_bytes as "size_bytes!", local_path, provider_id as "provider_id!" FROM attachments WHERE message_id = ?"#,
            att_mid,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("body.get_body.attachments", started_at, attachments_rows.len());

        let attachments: Vec<AttachmentMeta> = attachments_rows
            .into_iter()
            .map(|r| {
                Ok(AttachmentMeta {
                    id: decode_id(&r.id)?,
                    message_id: decode_id(&r.message_id)?,
                    filename: r.filename,
                    mime_type: r.mime_type,
                    size_bytes: r.size_bytes as u64,
                    local_path: r.local_path.map(std::path::PathBuf::from),
                    provider_id: r.provider_id,
                })
            })
            .collect::<Result<_, sqlx::Error>>()?;

        let metadata_json: String = row.try_get("metadata_json")?;
        Ok(Some(MessageBody {
            message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
            text_plain: row.try_get("text_plain")?,
            text_html: row.try_get("text_html")?,
            attachments,
            fetched_at: decode_timestamp(row.try_get("fetched_at")?)?,
            metadata: decode_json(&metadata_json)?,
        }))
    }

    pub async fn insert_body(&self, body: &MessageBody) -> Result<(), sqlx::Error> {
        let fetched_at = body.fetched_at.timestamp();
        let mid = body.message_id.as_str();
        let metadata_json = encode_json(&body.metadata)?;

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
