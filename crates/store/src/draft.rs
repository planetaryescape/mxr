use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn insert_draft(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        let to_addrs = serde_json::to_string(&draft.to).unwrap();
        let cc_addrs = serde_json::to_string(&draft.cc).unwrap();
        let bcc_addrs = serde_json::to_string(&draft.bcc).unwrap();
        let attachments = serde_json::to_string(&draft.attachments).unwrap();
        let in_reply_to = draft.in_reply_to.as_ref().map(|id| id.as_str());

        sqlx::query(
            "INSERT INTO drafts (id, account_id, in_reply_to, to_addrs, cc_addrs, bcc_addrs, subject, body_markdown, attachments, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(draft.id.as_str())
        .bind(draft.account_id.as_str())
        .bind(&in_reply_to)
        .bind(&to_addrs)
        .bind(&cc_addrs)
        .bind(&bcc_addrs)
        .bind(&draft.subject)
        .bind(&draft.body_markdown)
        .bind(&attachments)
        .bind(draft.created_at.timestamp())
        .bind(draft.updated_at.timestamp())
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_draft(&self, id: &DraftId) -> Result<Option<Draft>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM drafts WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(self.reader())
            .await?;

        Ok(row.as_ref().map(row_to_draft))
    }

    pub async fn list_drafts(&self, account_id: &AccountId) -> Result<Vec<Draft>, sqlx::Error> {
        let rows =
            sqlx::query("SELECT * FROM drafts WHERE account_id = ? ORDER BY updated_at DESC")
                .bind(account_id.as_str())
                .fetch_all(self.reader())
                .await?;

        Ok(rows.iter().map(row_to_draft).collect())
    }

    pub async fn delete_draft(&self, id: &DraftId) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM drafts WHERE id = ?")
            .bind(id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }
}

fn row_to_draft(row: &sqlx::sqlite::SqliteRow) -> Draft {
    let id_str: String = row.get("id");
    let account_id_str: String = row.get("account_id");
    let in_reply_to: Option<String> = row.get("in_reply_to");
    let to_json: String = row.get("to_addrs");
    let cc_json: String = row.get("cc_addrs");
    let bcc_json: String = row.get("bcc_addrs");
    let attachments_json: String = row.get("attachments");
    let created_at_ts: i64 = row.get("created_at");
    let updated_at_ts: i64 = row.get("updated_at");

    Draft {
        id: DraftId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
        account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id_str).unwrap()),
        in_reply_to: in_reply_to.map(|s| MessageId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
        to: serde_json::from_str(&to_json).unwrap_or_default(),
        cc: serde_json::from_str(&cc_json).unwrap_or_default(),
        bcc: serde_json::from_str(&bcc_json).unwrap_or_default(),
        subject: row.get("subject"),
        body_markdown: row.get("body_markdown"),
        attachments: serde_json::from_str(&attachments_json).unwrap_or_default(),
        created_at: chrono::DateTime::from_timestamp(created_at_ts, 0).unwrap_or_default(),
        updated_at: chrono::DateTime::from_timestamp(updated_at_ts, 0).unwrap_or_default(),
    }
}
