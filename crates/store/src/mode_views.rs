//! When each mode was last opened, for "N expired since you last looked",
//! and which modes' first-encounter cards are retired.

use crate::{decode_optional_timestamp, decode_timestamp};
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

    /// When each mode's first-encounter card was retired, by mode.
    pub async fn mode_guides_seen(
        &self,
    ) -> Result<std::collections::HashMap<String, DateTime<Utc>>, sqlx::Error> {
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT mode, seen_at FROM mode_guide_seen")
            .fetch_all(self.reader())
            .await?;
        rows.into_iter()
            .map(|(mode, at)| Ok((mode, decode_timestamp(at)?)))
            .collect()
    }

    /// Retire a mode's card, keeping the first time; `None` brings it back.
    pub async fn set_mode_guide_seen(
        &self,
        mode: &str,
        at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        match at {
            Some(at) => sqlx::query(
                "INSERT INTO mode_guide_seen (mode, seen_at) VALUES (?1, ?2)
                 ON CONFLICT(mode) DO NOTHING",
            )
            .bind(mode)
            .bind(at.timestamp())
            .execute(self.writer())
            .await
            .map(|_| ()),
            None => sqlx::query("DELETE FROM mode_guide_seen WHERE mode = ?")
                .bind(mode)
                .execute(self.writer())
                .await
                .map(|_| ()),
        }
    }
}
