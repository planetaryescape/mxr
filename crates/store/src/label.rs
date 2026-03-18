use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn upsert_label(&self, label: &Label) -> Result<(), sqlx::Error> {
        let id = label.id.as_str();
        let account_id = label.account_id.as_str();
        let kind = match label.kind {
            LabelKind::System => "system",
            LabelKind::Folder => "folder",
            LabelKind::User => "user",
        };

        sqlx::query(
            "INSERT OR REPLACE INTO labels (id, account_id, name, kind, color, provider_id, unread_count, total_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&account_id)
        .bind(&label.name)
        .bind(kind)
        .bind(&label.color)
        .bind(&label.provider_id)
        .bind(label.unread_count)
        .bind(label.total_count)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn list_labels_by_account(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<Label>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM labels WHERE account_id = ?")
            .bind(account_id.as_str())
            .fetch_all(self.reader())
            .await?;

        Ok(rows.iter().map(row_to_label).collect())
    }

    pub async fn update_label_counts(
        &self,
        label_id: &LabelId,
        unread_count: u32,
        total_count: u32,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE labels SET unread_count = ?, total_count = ? WHERE id = ?")
            .bind(unread_count)
            .bind(total_count)
            .bind(label_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn find_label_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<Label>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM labels WHERE account_id = ? AND provider_id = ?")
            .bind(account_id.as_str())
            .bind(provider_id)
            .fetch_optional(self.reader())
            .await?;

        Ok(row.as_ref().map(row_to_label))
    }
}

fn row_to_label(row: &sqlx::sqlite::SqliteRow) -> Label {
    let id_str: String = row.get("id");
    let account_id_str: String = row.get("account_id");
    let kind_str: String = row.get("kind");

    Label {
        id: LabelId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
        account_id: AccountId::from_uuid(uuid::Uuid::parse_str(&account_id_str).unwrap()),
        name: row.get("name"),
        kind: match kind_str.as_str() {
            "system" => LabelKind::System,
            "folder" => LabelKind::Folder,
            _ => LabelKind::User,
        },
        color: row.get("color"),
        provider_id: row.get("provider_id"),
        unread_count: row.get::<u32, _>("unread_count"),
        total_count: row.get::<u32, _>("total_count"),
    }
}
