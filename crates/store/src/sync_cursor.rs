use crate::mxr_core::{AccountId, SyncCursor};
use crate::mxr_store::{decode_json, encode_json};

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
}
