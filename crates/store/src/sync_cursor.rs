use crate::{decode_json, encode_json};
use mxr_core::{AccountId, SyncCursor};
use sqlx::Row;
use std::collections::HashMap;

impl super::Store {
    pub async fn get_sync_cursor(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncCursor>, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query!("SELECT sync_cursor FROM accounts WHERE id = ?", aid,)
            .fetch_optional(self.reader())
            .await?;

        match row {
            Some(r) => r.sync_cursor.map(|cursor| decode_json(&cursor)).transpose(),
            None => Ok(None),
        }
    }

    pub async fn set_sync_cursor(
        &self,
        account_id: &AccountId,
        cursor: &SyncCursor,
    ) -> Result<(), sqlx::Error> {
        let cursor_json = encode_json(cursor)?;
        let aid = account_id.as_str();
        sqlx::query!(
            "UPDATE accounts SET sync_cursor = ? WHERE id = ?",
            cursor_json,
            aid,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_sync_cursors(&self) -> Result<HashMap<AccountId, SyncCursor>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, sync_cursor
            FROM accounts
            WHERE sync_cursor IS NOT NULL
            "#,
        )
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                let account_id: String = row.get("id");
                let cursor_json: Option<String> = row.get("sync_cursor");
                let cursor = cursor_json
                    .as_deref()
                    .ok_or_else(|| sqlx::Error::Protocol("missing sync cursor".into()))
                    .and_then(decode_json)?;
                Ok((account_id.parse().map_err(sqlx::Error::decode)?, cursor))
            })
            .collect()
    }
}
