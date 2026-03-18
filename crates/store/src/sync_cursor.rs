use mxr_core::{AccountId, SyncCursor};
use sqlx::Row;

impl super::Store {
    pub async fn get_sync_cursor(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncCursor>, sqlx::Error> {
        let row = sqlx::query("SELECT sync_cursor FROM accounts WHERE id = ?")
            .bind(account_id.as_str())
            .fetch_optional(self.reader())
            .await?;

        match row {
            Some(r) => {
                let cursor_json: Option<String> = r.get("sync_cursor");
                Ok(cursor_json.and_then(|c| serde_json::from_str(&c).ok()))
            }
            None => Ok(None),
        }
    }

    pub async fn set_sync_cursor(
        &self,
        account_id: &AccountId,
        cursor: &SyncCursor,
    ) -> Result<(), sqlx::Error> {
        let cursor_json = serde_json::to_string(cursor).unwrap();
        sqlx::query("UPDATE accounts SET sync_cursor = ? WHERE id = ?")
            .bind(&cursor_json)
            .bind(account_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
