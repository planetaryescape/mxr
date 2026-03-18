use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    pub async fn insert_saved_search(&self, search: &SavedSearch) -> Result<(), sqlx::Error> {
        let account_id = search.account_id.as_ref().map(|id| id.as_str());
        let sort = serde_json::to_string(&search.sort).unwrap();

        sqlx::query(
            "INSERT INTO saved_searches (id, account_id, name, query, sort_order, icon, position, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(search.id.as_str())
        .bind(&account_id)
        .bind(&search.name)
        .bind(&search.query)
        .bind(&sort)
        .bind(&search.icon)
        .bind(search.position)
        .bind(search.created_at.timestamp())
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedSearch>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM saved_searches ORDER BY position ASC")
            .fetch_all(self.reader())
            .await?;

        Ok(rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let id_str: String = row.get("id");
                let account_id: Option<String> = row.get("account_id");
                let sort_str: String = row.get("sort_order");
                let created_at_ts: i64 = row.get("created_at");

                SavedSearch {
                    id: SavedSearchId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
                    account_id: account_id
                        .map(|s| AccountId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
                    name: row.get("name"),
                    query: row.get("query"),
                    sort: serde_json::from_str(&sort_str).unwrap_or(SortOrder::DateDesc),
                    icon: row.get("icon"),
                    position: row.get("position"),
                    created_at: chrono::DateTime::from_timestamp(created_at_ts, 0)
                        .unwrap_or_default(),
                }
            })
            .collect())
    }

    pub async fn delete_saved_search(&self, id: &SavedSearchId) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM saved_searches WHERE id = ?")
            .bind(id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
