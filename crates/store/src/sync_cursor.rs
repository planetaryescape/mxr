use mxr_core::{AccountId, SyncCursor};

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
            Some(r) => Ok(r.sync_cursor.and_then(|c| serde_json::from_str(&c).ok())),
            None => Ok(None),
        }
    }

    pub async fn set_sync_cursor(
        &self,
        account_id: &AccountId,
        cursor: &SyncCursor,
    ) -> Result<(), sqlx::Error> {
        let cursor_json = serde_json::to_string(cursor).unwrap();
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
}
