//! When each mode was last opened, for "N expired since you last looked".

use crate::decode_optional_timestamp;
use chrono::{DateTime, Utc};

impl super::Store {
    pub async fn mode_last_viewed(&self, mode: &str) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let at: Option<i64> =
            sqlx::query_scalar("SELECT last_viewed_at FROM mode_views WHERE mode = ?")
                .bind(mode)
                .fetch_optional(self.reader())
                .await?;
        decode_optional_timestamp(at)
    }

    pub async fn set_mode_viewed(&self, mode: &str, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO mode_views (mode, last_viewed_at) VALUES (?1, ?2)
             ON CONFLICT(mode) DO UPDATE SET last_viewed_at = excluded.last_viewed_at",
        )
        .bind(mode)
        .bind(at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }
}
