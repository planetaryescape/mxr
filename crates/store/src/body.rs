use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
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
        .bind(&mid)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("body.get_body", started_at, row.is_some());

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let att_mid = message_id.as_str();
        let started_at = std::time::Instant::now();
        let attachments_rows = sqlx::query(
            r#"SELECT id, message_id, filename, mime_type, disposition, content_id, content_location, size_bytes, local_path, provider_id FROM attachments WHERE message_id = ?"#,
        )
        .bind(att_mid)
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "body.get_body.attachments",
            started_at,
            attachments_rows.len(),
        );

        let attachments: Vec<AttachmentMeta> = attachments_rows
            .into_iter()
            .map(|r| {
                Ok(AttachmentMeta {
                    id: decode_id(r.try_get::<&str, _>("id")?)?,
                    message_id: decode_id(r.try_get::<&str, _>("message_id")?)?,
                    filename: r.try_get("filename")?,
                    mime_type: r.try_get("mime_type")?,
                    disposition: decode_attachment_disposition(
                        r.try_get::<&str, _>("disposition")?,
                    )?,
                    content_id: r.try_get("content_id")?,
                    content_location: r.try_get("content_location")?,
                    size_bytes: r.try_get::<i64, _>("size_bytes")? as u64,
                    local_path: r
                        .try_get::<Option<String>, _>("local_path")?
                        .map(std::path::PathBuf::from),
                    provider_id: r.try_get("provider_id")?,
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
        .bind(&mid)
        .bind(&body.text_plain)
        .bind(&body.text_html)
        .bind(fetched_at)
        .bind(metadata_json)
        .execute(self.writer())
        .await?;

        // Promote List-Id from body metadata to the indexed `messages.list_id`
        // column. Cheap upsert; runs once per body and is overwritten if the
        // sender's headers change. Powers `mxr unsub --rank` grouping.
        if let Some(list_id) = body.metadata.list_id.as_ref() {
            sqlx::query("UPDATE messages SET list_id = ? WHERE id = ?")
                .bind(list_id)
                .bind(&mid)
                .execute(self.writer())
                .await?;
        }

        sqlx::query("DELETE FROM attachments WHERE message_id = ?")
            .bind(&mid)
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
            let disposition = encode_attachment_disposition(att.disposition);
            sqlx::query(
                "INSERT OR REPLACE INTO attachments (id, message_id, filename, mime_type, disposition, content_id, content_location, size_bytes, local_path, provider_id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(att_id)
            .bind(att_mid)
            .bind(&att.filename)
            .bind(&att.mime_type)
            .bind(disposition)
            .bind(&att.content_id)
            .bind(&att.content_location)
            .bind(size_bytes)
            .bind(local_path)
            .bind(&att.provider_id)
            .execute(self.writer())
            .await?;
        }

        Ok(())
    }
}

fn encode_attachment_disposition(disposition: AttachmentDisposition) -> &'static str {
    match disposition {
        AttachmentDisposition::Attachment => "attachment",
        AttachmentDisposition::Inline => "inline",
        AttachmentDisposition::Unspecified => "unspecified",
    }
}

fn decode_attachment_disposition(value: &str) -> Result<AttachmentDisposition, sqlx::Error> {
    match value {
        "attachment" => Ok(AttachmentDisposition::Attachment),
        "inline" => Ok(AttachmentDisposition::Inline),
        "unspecified" => Ok(AttachmentDisposition::Unspecified),
        other => Err(sqlx::Error::Protocol(format!(
            "invalid attachment disposition: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::super::Store;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use mxr_core::{
        AttachmentDisposition, AttachmentId, AttachmentMeta, MessageBody, MessageMetadata,
    };

    fn attachment(message_id: mxr_core::MessageId, provider_id: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: AttachmentId::from_provider_id("test", provider_id),
            message_id,
            filename: format!("{provider_id}.txt"),
            mime_type: "text/plain".into(),
            disposition: AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: 10,
            local_path: None,
            provider_id: provider_id.into(),
        }
    }

    fn body(message_id: mxr_core::MessageId, attachments: Vec<AttachmentMeta>) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: Some("body".into()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[tokio::test]
    async fn body_refresh_removes_attachments_no_longer_returned_by_provider() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();

        store
            .insert_body(&body(
                envelope.id.clone(),
                vec![
                    attachment(envelope.id.clone(), "first"),
                    attachment(envelope.id.clone(), "removed"),
                ],
            ))
            .await
            .unwrap();

        store
            .insert_body(&body(
                envelope.id.clone(),
                vec![attachment(envelope.id.clone(), "first")],
            ))
            .await
            .unwrap();

        let refreshed = store.get_body(&envelope.id).await.unwrap().unwrap();
        let provider_ids = refreshed
            .attachments
            .iter()
            .map(|attachment| attachment.provider_id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(provider_ids, vec!["first"]);
    }
}
