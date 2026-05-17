use mxr_core::{AccountId, SyncCursor};
use sqlx::Row;
use std::collections::HashMap;

// The `accounts.sync_cursor` column is TEXT. With MSP Phase B, the cursor
// is opaque bytes owned by the provider adapter — by convention always
// UTF-8 JSON. We round-trip it as a String here without decoding any
// inner structure; the adapter parses (and migrates) on read.

impl super::Store {
    pub async fn get_sync_cursor(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncCursor>, sqlx::Error> {
        let aid = account_id.as_str();
        let row = sqlx::query!("SELECT sync_cursor FROM accounts WHERE id = ?", aid,)
            .fetch_optional(self.reader())
            .await?;

        Ok(row
            .and_then(|r| r.sync_cursor)
            .map(|s| SyncCursor::from_bytes(s.into_bytes())))
    }

    pub async fn set_sync_cursor(
        &self,
        account_id: &AccountId,
        cursor: &SyncCursor,
    ) -> Result<(), sqlx::Error> {
        let cursor_text = String::from_utf8(cursor.as_bytes().to_vec()).map_err(|err| {
            // Adapter contract: cursors are UTF-8 JSON. A non-UTF-8 cursor
            // signals an adapter bug; surface it as a decode error rather
            // than silently lossy-truncating.
            sqlx::Error::Decode(Box::new(err))
        })?;
        let aid = account_id.as_str();
        sqlx::query!(
            "UPDATE accounts SET sync_cursor = ? WHERE id = ?",
            cursor_text,
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
                let cursor_text: Option<String> = row.get("sync_cursor");
                let cursor = cursor_text
                    .map(|s| SyncCursor::from_bytes(s.into_bytes()))
                    .ok_or_else(|| sqlx::Error::Protocol("missing sync cursor".into()))?;
                Ok((account_id.parse().map_err(sqlx::Error::decode)?, cursor))
            })
            .collect()
    }
}
