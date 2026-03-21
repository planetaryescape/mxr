use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn insert_saved_search(&self, search: &SavedSearch) -> Result<(), sqlx::Error> {
        let id = search.id.as_str();
        let account_id = search.account_id.as_ref().map(|id| id.as_str());
        let search_mode = serde_json::to_string(&search.search_mode).unwrap();
        let sort = serde_json::to_string(&search.sort).unwrap();
        let created_at = search.created_at.timestamp();
        let position = search.position as i64;

        sqlx::query(
            "INSERT INTO saved_searches (id, account_id, name, query, search_mode, sort_order, icon, position, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(account_id)
        .bind(&search.name)
        .bind(&search.query)
        .bind(search_mode)
        .bind(sort)
        .bind(&search.icon)
        .bind(position)
        .bind(created_at)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedSearch>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, account_id, name, query, search_mode, sort_order, icon, position, created_at
               FROM saved_searches ORDER BY position ASC"#,
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows.into_iter().map(row_to_saved_search).collect())
    }

    pub async fn delete_saved_search(&self, id: &SavedSearchId) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        sqlx::query!("DELETE FROM saved_searches WHERE id = ?", id_str)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn get_saved_search_by_name(
        &self,
        name: &str,
    ) -> Result<Option<SavedSearch>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, account_id, name, query, search_mode, sort_order, icon, position, created_at
               FROM saved_searches WHERE name = ?"#,
        )
        .bind(name)
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(row_to_saved_search))
    }

    pub async fn delete_saved_search_by_name(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM saved_searches WHERE name = ?", name)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_saved_search(row: sqlx::sqlite::SqliteRow) -> SavedSearch {
    SavedSearch {
        id: SavedSearchId::from_uuid(uuid::Uuid::parse_str(&row.get::<String, _>("id")).unwrap()),
        account_id: row
            .get::<Option<String>, _>("account_id")
            .map(|s| AccountId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
        name: row.get::<String, _>("name"),
        query: row.get::<String, _>("query"),
        search_mode: serde_json::from_str(&row.get::<String, _>("search_mode")).unwrap_or_default(),
        sort: serde_json::from_str(&row.get::<String, _>("sort_order"))
            .unwrap_or(SortOrder::DateDesc),
        icon: row.get::<Option<String>, _>("icon"),
        position: row.get::<i64, _>("position") as i32,
        created_at: chrono::DateTime::from_timestamp(row.get::<i64, _>("created_at"), 0)
            .unwrap_or_default(),
    }
}
