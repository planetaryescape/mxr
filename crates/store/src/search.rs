use mxr_core::id::*;
use mxr_core::types::*;

impl super::Store {
    pub async fn insert_saved_search(&self, search: &SavedSearch) -> Result<(), sqlx::Error> {
        let id = search.id.as_str();
        let account_id = search.account_id.as_ref().map(|id| id.as_str());
        let sort = serde_json::to_string(&search.sort).unwrap();
        let created_at = search.created_at.timestamp();
        let position = search.position as i64; // i32 -> i64 for SQLite binding

        sqlx::query!(
            "INSERT INTO saved_searches (id, account_id, name, query, sort_order, icon, position, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            id,
            account_id,
            search.name,
            search.query,
            sort,
            search.icon,
            position,
            created_at,
        )
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedSearch>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT id as "id!", account_id, name as "name!", query as "query!",
                      sort_order as "sort_order!", icon, position as "position!",
                      created_at as "created_at!"
               FROM saved_searches ORDER BY position ASC"#
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| SavedSearch {
                id: SavedSearchId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
                account_id: r
                    .account_id
                    .map(|s| AccountId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
                name: r.name,
                query: r.query,
                sort: serde_json::from_str(&r.sort_order).unwrap_or(SortOrder::DateDesc),
                icon: r.icon,
                position: r.position as i32,
                created_at: chrono::DateTime::from_timestamp(r.created_at, 0).unwrap_or_default(),
            })
            .collect())
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
        let row = sqlx::query!(
            r#"SELECT id as "id!", account_id, name as "name!", query as "query!",
                      sort_order as "sort_order!", icon, position as "position!",
                      created_at as "created_at!"
               FROM saved_searches WHERE name = ?"#,
            name,
        )
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(|r| SavedSearch {
            id: SavedSearchId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
            account_id: r
                .account_id
                .map(|s| AccountId::from_uuid(uuid::Uuid::parse_str(&s).unwrap())),
            name: r.name,
            query: r.query,
            sort: serde_json::from_str(&r.sort_order).unwrap_or(SortOrder::DateDesc),
            icon: r.icon,
            position: r.position as i32,
            created_at: chrono::DateTime::from_timestamp(r.created_at, 0).unwrap_or_default(),
        }))
    }

    pub async fn delete_saved_search_by_name(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM saved_searches WHERE name = ?", name)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
