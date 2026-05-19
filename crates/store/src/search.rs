use crate::{decode_id, decode_json, decode_timestamp, encode_json, trace_lookup, trace_query};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

impl super::Store {
    pub async fn insert_saved_search(&self, search: &SavedSearch) -> Result<(), sqlx::Error> {
        let id = search.id.as_str();
        let account_id = search.account_id.as_ref().map(|id| id.as_str());
        let search_mode = encode_json(&search.search_mode)?;
        let sort = encode_json(&search.sort)?;
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
        let started_at = std::time::Instant::now();
        let rows = sqlx::query(
            r#"SELECT id, account_id, name, query, search_mode, sort_order, icon, position, created_at
               FROM saved_searches ORDER BY position ASC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("search.list_saved_searches", started_at, rows.len());

        rows.into_iter().map(row_to_saved_search).collect()
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
        let started_at = std::time::Instant::now();
        let row = sqlx::query(
            r#"SELECT id, account_id, name, query, search_mode, sort_order, icon, position, created_at
               FROM saved_searches WHERE name = ?"#,
        )
        .bind(name)
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("search.get_saved_search_by_name", started_at, row.is_some());

        row.map(row_to_saved_search).transpose()
    }

    pub async fn delete_saved_search_by_name(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM saved_searches WHERE name = ?", name)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Patch a saved search by its current name. Returns the updated record
    /// or None if no match exists.
    pub async fn update_saved_search_by_name(
        &self,
        name: &str,
        update: super::SavedSearchUpdate<'_>,
    ) -> Result<Option<SavedSearch>, sqlx::Error> {
        let Some(existing) = self.get_saved_search_by_name(name).await? else {
            return Ok(None);
        };
        let merged = SavedSearch {
            id: existing.id,
            account_id: existing.account_id,
            name: update
                .new_name
                .map(|s| s.to_string())
                .unwrap_or(existing.name),
            query: update
                .query
                .map(|s| s.to_string())
                .unwrap_or(existing.query),
            search_mode: update.search_mode.cloned().unwrap_or(existing.search_mode),
            sort: update.sort.cloned().unwrap_or(existing.sort),
            icon: update.icon.map(|s| s.to_string()).or(existing.icon),
            position: update.position.unwrap_or(existing.position),
            created_at: existing.created_at,
        };

        let id_str = merged.id.as_str();
        let search_mode_json = encode_json(&merged.search_mode)?;
        let sort_json = encode_json(&merged.sort)?;
        let position_i64 = merged.position as i64;

        sqlx::query(
            r#"UPDATE saved_searches
               SET name = ?, query = ?, search_mode = ?, sort_order = ?, icon = ?, position = ?
               WHERE id = ?"#,
        )
        .bind(&merged.name)
        .bind(&merged.query)
        .bind(search_mode_json)
        .bind(sort_json)
        .bind(&merged.icon)
        .bind(position_i64)
        .bind(id_str)
        .execute(self.writer())
        .await?;

        Ok(Some(merged))
    }
}

fn row_to_saved_search(row: sqlx::sqlite::SqliteRow) -> Result<SavedSearch, sqlx::Error> {
    Ok(SavedSearch {
        id: decode_id(&row.get::<String, _>("id"))?,
        account_id: row
            .get::<Option<String>, _>("account_id")
            .map(|value| decode_id(&value))
            .transpose()?,
        name: row.get::<String, _>("name"),
        query: row.get::<String, _>("query"),
        search_mode: decode_json(&row.get::<String, _>("search_mode"))?,
        sort: decode_json(&row.get::<String, _>("sort_order"))?,
        icon: row.get::<Option<String>, _>("icon"),
        position: row.get::<i64, _>("position") as i32,
        created_at: decode_timestamp(row.get::<i64, _>("created_at"))?,
    })
}
