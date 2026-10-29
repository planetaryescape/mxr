use crate::decode_id;
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
        let role = label.role.map(Role::storage_name);
        let unread_count = label.unread_count as i64;
        let total_count = label.total_count as i64;

        let mut tx = self.writer().begin().await?;

        let existing_id: Option<String> =
            sqlx::query_scalar("SELECT id FROM labels WHERE account_id = ? AND provider_id = ?")
                .bind(&account_id)
                .bind(&label.provider_id)
                .fetch_optional(&mut *tx)
                .await?;

        if let Some(existing_id) = existing_id {
            // Only a real change writes: sync upserts every label on every
            // pass, and a rewrite of the same values still counts as a change
            // for the cached mode views (and grows the WAL).
            sqlx::query(
                "UPDATE labels SET
                    name = ?1,
                    kind = ?2,
                    color = ?3,
                    role = ?5
                 WHERE id = ?4
                   AND (name IS NOT ?1 OR kind IS NOT ?2 OR color IS NOT ?3 OR role IS NOT ?5)",
            )
            .bind(&label.name)
            .bind(kind)
            .bind(&label.color)
            .bind(existing_id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(());
        }

        // On conflict, do NOT overwrite counts — those are managed by
        // recalculate_label_counts() from the junction table. Only update
        // metadata (name, kind, color, provider_id).
        sqlx::query(
            "INSERT INTO labels (id, account_id, name, kind, color, provider_id, unread_count, total_count, role)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                kind = excluded.kind,
                color = excluded.color,
                provider_id = excluded.provider_id,
                role = excluded.role",
        )
        .bind(&id)
        .bind(&account_id)
        .bind(&label.name)
        .bind(kind)
        .bind(&label.color)
        .bind(&label.provider_id)
        .bind(unread_count)
        .bind(total_count)
        .bind(role)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_labels_by_account(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<Label>, sqlx::Error> {
        let aid = account_id.as_str();
        let rows = sqlx::query(
            r#"SELECT id, account_id, name, kind, color, provider_id, unread_count, total_count, role
               FROM labels WHERE account_id = ?"#,
        )
        .bind(aid)
        .fetch_all(self.reader())
        .await?;

        rows.into_iter().map(row_to_label).collect()
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

    pub async fn delete_label(&self, label_id: &LabelId) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM labels WHERE id = ?")
            .bind(label_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn replace_label(
        &self,
        old_label_id: &LabelId,
        new_label: &Label,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;

        let existing: Option<(i64, i64)> =
            sqlx::query_as("SELECT unread_count, total_count FROM labels WHERE id = ?")
                .bind(old_label_id.as_str())
                .fetch_optional(&mut *tx)
                .await?;

        let (unread_count, total_count) = existing.unwrap_or((0, 0));
        let kind = match new_label.kind {
            LabelKind::System => "system",
            LabelKind::Folder => "folder",
            LabelKind::User => "user",
        };

        sqlx::query(
            "INSERT INTO labels (id, account_id, name, kind, color, provider_id, unread_count, total_count, role)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                account_id = excluded.account_id,
                name = excluded.name,
                kind = excluded.kind,
                color = excluded.color,
                provider_id = excluded.provider_id,
                unread_count = excluded.unread_count,
                total_count = excluded.total_count,
                role = excluded.role",
        )
        .bind(new_label.id.as_str())
        .bind(new_label.account_id.as_str())
        .bind(&new_label.name)
        .bind(kind)
        .bind(&new_label.color)
        .bind(&new_label.provider_id)
        .bind(unread_count)
        .bind(total_count)
        .bind(new_label.role.map(Role::storage_name))
        .execute(&mut *tx)
        .await?;

        if old_label_id != &new_label.id {
            sqlx::query("UPDATE message_labels SET label_id = ? WHERE label_id = ?")
                .bind(new_label.id.as_str())
                .bind(old_label_id.as_str())
                .execute(&mut *tx)
                .await?;

            sqlx::query("DELETE FROM labels WHERE id = ?")
                .bind(old_label_id.as_str())
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn recalculate_label_counts(
        &self,
        account_id: &AccountId,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        // Rows whose counts already match are left alone; see `upsert_label`.
        sqlx::query!(
            "UPDATE labels SET total_count = counts.total, unread_count = counts.unread
            FROM (SELECT l.id,
                    (SELECT COUNT(*) FROM message_labels WHERE label_id = l.id) AS total,
                    (SELECT COUNT(*) FROM message_labels ml
                        JOIN messages m ON m.id = ml.message_id
                        WHERE ml.label_id = l.id AND (m.flags & 1) = 0) AS unread
                FROM labels l WHERE l.account_id = ?) AS counts
            WHERE labels.id = counts.id
              AND (labels.total_count IS NOT counts.total
                OR labels.unread_count IS NOT counts.unread)",
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
        let mut query =
            sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str())).bind(aid);
        for pid in provider_ids {
            query = query.bind(pid);
        }
        let rows = query.fetch_all(self.reader()).await?;
        rows.into_iter().map(|id| decode_id(&id)).collect()
    }

    pub async fn find_label_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<Label>, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query(
            r#"SELECT id, account_id, name, kind, color, provider_id, unread_count, total_count, role
               FROM labels WHERE account_id = ? AND provider_id = ?"#,
        )
        .bind(aid)
        .bind(provider_id)
        .fetch_optional(self.reader())
        .await?;

        row.map(row_to_label).transpose()
    }
}

fn row_to_label(row: sqlx::sqlite::SqliteRow) -> Result<Label, sqlx::Error> {
    Ok(Label {
        id: decode_id(&row.get::<String, _>("id"))?,
        account_id: decode_id(&row.get::<String, _>("account_id"))?,
        name: row.get::<String, _>("name"),
        kind: match row.get::<String, _>("kind").as_str() {
            "system" => LabelKind::System,
            "folder" => LabelKind::Folder,
            _ => LabelKind::User,
        },
        color: row.get::<Option<String>, _>("color"),
        provider_id: row.get::<String, _>("provider_id"),
        unread_count: row.get::<i64, _>("unread_count") as u32,
        total_count: row.get::<i64, _>("total_count") as u32,
        role: row
            .get::<Option<String>, _>("role")
            .as_deref()
            .and_then(Role::from_storage_name),
    })
}
