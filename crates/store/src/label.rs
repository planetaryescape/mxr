use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    pub async fn upsert_label(&self, label: &Label) -> Result<(), sqlx::Error> {
        let id = label.id.as_str();
        let account_id = label.account_id.as_str();
        let kind = match label.kind {
            LabelKind::System => "system",
            LabelKind::Folder => "folder",
            LabelKind::User => "user",
        };
        let unread_count = label.unread_count as i64;
        let total_count = label.total_count as i64;

        sqlx::query!(
            "INSERT INTO labels (id, account_id, name, kind, color, provider_id, unread_count, total_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                color = excluded.color,
                provider_id = excluded.provider_id,
                unread_count = excluded.unread_count,
                total_count = excluded.total_count",
            id,
            account_id,
            label.name,
            kind,
            label.color,
            label.provider_id,
            unread_count,
            total_count,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn list_labels_by_account(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<Label>, sqlx::Error> {
        let aid = account_id.as_str();
        let rows = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!", name as "name!",
                      kind as "kind!", color, provider_id as "provider_id!",
                      unread_count as "unread_count!", total_count as "total_count!"
               FROM labels WHERE account_id = ?"#,
            aid,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Label {
                id: LabelId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
                account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
                name: r.name,
                kind: match r.kind.as_str() {
                    "system" => LabelKind::System,
                    "folder" => LabelKind::Folder,
                    _ => LabelKind::User,
                },
                color: r.color,
                provider_id: r.provider_id,
                unread_count: r.unread_count as u32,
                total_count: r.total_count as u32,
            })
            .collect())
    }

    pub async fn update_label_counts(
        &self,
        label_id: &LabelId,
        unread_count: u32,
        total_count: u32,
    ) -> Result<(), sqlx::Error> {
        let lid = label_id.as_str();
        let unread = unread_count as i64;
        let total = total_count as i64;
        sqlx::query!(
            "UPDATE labels SET unread_count = ?, total_count = ? WHERE id = ?",
            unread,
            total,
            lid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn recalculate_label_counts(
        &self,
        account_id: &AccountId,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        sqlx::query!(
            "UPDATE labels SET
                total_count = (SELECT COUNT(*) FROM message_labels WHERE label_id = labels.id),
                unread_count = (SELECT COUNT(*) FROM message_labels ml
                    JOIN messages m ON m.id = ml.message_id
                    WHERE ml.label_id = labels.id AND (m.flags & 1) = 0)
            WHERE account_id = ?",
            aid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Look up LabelIds from a list of provider_ids (e.g. ["INBOX", "SENT"]).
    /// Returns only the IDs for labels that exist in the store.
    pub async fn find_labels_by_provider_ids(
        &self,
        account_id: &AccountId,
        provider_ids: &[String],
    ) -> Result<Vec<LabelId>, sqlx::Error> {
        if provider_ids.is_empty() {
            return Ok(vec![]);
        }
        let aid = account_id.as_str();
        let placeholders: Vec<String> = provider_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT id FROM labels WHERE account_id = ? AND provider_id IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query_scalar::<_, String>(&sql).bind(aid);
        for pid in provider_ids {
            query = query.bind(pid);
        }
        let rows = query.fetch_all(self.reader()).await?;
        Ok(rows
            .into_iter()
            .map(|id| LabelId::from_uuid(uuid::Uuid::parse_str(&id).unwrap()))
            .collect())
    }

    pub async fn find_label_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<Label>, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id as "account_id!", name as "name!",
                      kind as "kind!", color, provider_id as "provider_id!",
                      unread_count as "unread_count!", total_count as "total_count!"
               FROM labels WHERE account_id = ? AND provider_id = ?"#,
            aid,
            provider_id,
        )
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(|r| Label {
            id: LabelId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
            account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.account_id).unwrap()),
            name: r.name,
            kind: match r.kind.as_str() {
                "system" => LabelKind::System,
                "folder" => LabelKind::Folder,
                _ => LabelKind::User,
            },
            color: r.color,
            provider_id: r.provider_id,
            unread_count: r.unread_count as u32,
            total_count: r.total_count as u32,
        }))
    }
}
