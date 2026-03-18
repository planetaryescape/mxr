use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    pub async fn insert_draft(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        let id = draft.id.as_str();
        let account_id = draft.account_id.as_str();
        let to_addrs = serde_json::to_string(&draft.to).unwrap();
        let cc_addrs = serde_json::to_string(&draft.cc).unwrap();
        let bcc_addrs = serde_json::to_string(&draft.bcc).unwrap();
        let attachments = serde_json::to_string(&draft.attachments).unwrap();
        let in_reply_to = draft.in_reply_to.as_ref().map(|id| id.as_str());
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

        Ok(row.map(|r| Draft {
            id: DraftId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
            account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
            in_reply_to: r
                .in_reply_to
                .map(|s| MessageId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
            to: serde_json::from_str(&r.to_addrs).unwrap_or_default(),
            cc: serde_json::from_str(&r.cc_addrs).unwrap_or_default(),
            bcc: serde_json::from_str(&r.bcc_addrs).unwrap_or_default(),
            subject: r.subject,
            body_markdown: r.body_markdown,
            attachments: serde_json::from_str(&r.attachments).unwrap_or_default(),
            created_at: chrono::DateTime::from_timestamp(r.created_at, 0).unwrap_or_default(),
            updated_at: chrono::DateTime::from_timestamp(r.updated_at, 0).unwrap_or_default(),
        }))
    }

    pub async fn list_drafts(&self, account_id: &AccountId) -> Result<Vec<Draft>, sqlx::Error> {
        let aid = account_id.as_str();
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

        Ok(rows
            .into_iter()
            .map(|r| Draft {
                id: DraftId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
                in_reply_to: r
                    .in_reply_to
                    .map(|s| MessageId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
                to: serde_json::from_str(&r.to_addrs).unwrap_or_default(),
                cc: serde_json::from_str(&r.cc_addrs).unwrap_or_default(),
                bcc: serde_json::from_str(&r.bcc_addrs).unwrap_or_default(),
                subject: r.subject,
                body_markdown: r.body_markdown,
                attachments: serde_json::from_str(&r.attachments).unwrap_or_default(),
                created_at: chrono::DateTime::from_timestamp(r.created_at, 0).unwrap_or_default(),
                updated_at: chrono::DateTime::from_timestamp(r.updated_at, 0).unwrap_or_default(),
            })
            .collect())
    }

    pub async fn delete_draft(&self, id: &DraftId) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        sqlx::query!("DELETE FROM drafts WHERE id = ?", id_str)
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
