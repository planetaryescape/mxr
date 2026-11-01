//! When each mode was last opened, for "N expired since you last looked",
//! and which hints were dismissed.

use crate::{decode_optional_timestamp, decode_timestamp};
use chrono::{DateTime, Utc};

impl super::Store {
    /// Look up only the exact revisions in a response, not the account's history.
    pub async fn mode_rows_seen_in(
        &self,
        account: &mxr_core::AccountId,
        mode: &str,
        revisions: &[(String, String)],
    ) -> Result<std::collections::HashSet<(String, String)>, sqlx::Error> {
        let rows = sqlx::query_as(
            "SELECT DISTINCT seen.row_id, seen.revision FROM json_each(?) requested
             JOIN mode_row_seen seen ON seen.account_id = ? AND seen.mode = ?
             AND seen.row_id = json_extract(requested.value, '$[0]')
             AND seen.revision = json_extract(requested.value, '$[1]')",
        )
        .bind(
            serde_json::to_string(revisions)
                .map_err(|error| sqlx::Error::Encode(Box::new(error)))?,
        )
        .bind(account.as_str())
        .bind(mode)
        .fetch_all(self.reader())
        .await?;
        Ok(rows.into_iter().collect())
    }

    /// Exact revisions opened in a mode. Reading a list never writes these.
    pub async fn mode_rows_seen(
        &self,
        account: &mxr_core::AccountId,
        mode: &str,
    ) -> Result<std::collections::HashSet<(String, String)>, sqlx::Error> {
        let rows = sqlx::query_as(
            "SELECT row_id, revision FROM mode_row_seen WHERE account_id = ? AND mode = ?",
        )
        .bind(account.as_str())
        .bind(mode)
        .fetch_all(self.reader())
        .await?;
        Ok(rows.into_iter().collect())
    }

    pub async fn mark_mode_row_seen(
        &self,
        account: &mxr_core::AccountId,
        mode: &str,
        row_id: &str,
        revision: &str,
        at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO mode_row_seen (account_id, mode, row_id, revision, seen_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING")
            .bind(account.as_str()).bind(mode).bind(row_id).bind(revision).bind(at.timestamp())
            .execute(self.writer()).await?;
        Ok(())
    }

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

    /// When each dismissed hint was dismissed, by hint id.
    pub async fn hints_seen(
        &self,
    ) -> Result<std::collections::HashMap<String, DateTime<Utc>>, sqlx::Error> {
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT hint_id, seen_at FROM hint_seen")
            .fetch_all(self.reader())
            .await?;
        rows.into_iter()
            .map(|(id, at)| Ok((id, decode_timestamp(at)?)))
            .collect()
    }

    /// Dismiss a hint, keeping the first time; `None` brings it back. True
    /// when this call marked it seen (it was not seen before).
    pub async fn set_hint_seen(
        &self,
        hint_id: &str,
        at: Option<DateTime<Utc>>,
    ) -> Result<bool, sqlx::Error> {
        match at {
            Some(at) => sqlx::query(
                "INSERT INTO hint_seen (hint_id, seen_at) VALUES (?1, ?2)
                 ON CONFLICT(hint_id) DO NOTHING",
            )
            .bind(hint_id)
            .bind(at.timestamp())
            .execute(self.writer())
            .await
            .map(|done| done.rows_affected() > 0),
            None => sqlx::query("DELETE FROM hint_seen WHERE hint_id = ?")
                .bind(hint_id)
                .execute(self.writer())
                .await
                .map(|_| false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn row_seen_survives_reopen_and_is_exact_scoped_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("seen.db");
        let store = crate::Store::new(&path).await.unwrap();
        let account = crate::test_fixtures::test_account();
        store.insert_account(&account).await.unwrap();
        for revision in ["new", "old", "new"] {
            store
                .mark_mode_row_seen(&account.id, "todo", "same-id", revision, Utc::now())
                .await
                .unwrap();
        }
        store.reader().close().await;
        store.writer().close().await;
        drop(store);
        let reopened = crate::Store::new(&path).await.unwrap();
        let rows = reopened.mode_rows_seen(&account.id, "todo").await.unwrap();
        assert_eq!(
            rows.len(),
            2,
            "duplicates are idempotent; old acknowledgements cannot replace new ones"
        );
        assert!(rows.contains(&("same-id".into(), "new".into())));
        let requested = [
            ("same-id".into(), "new".into()),
            ("same-id".into(), "unseen".into()),
        ];
        let current = reopened
            .mode_rows_seen_in(&account.id, "todo", &requested)
            .await
            .unwrap();
        assert_eq!(
            current,
            std::collections::HashSet::from([("same-id".into(), "new".into())])
        );
        assert!(reopened
            .mode_rows_seen_in(&account.id, "updates", &requested)
            .await
            .unwrap()
            .is_empty());
        assert!(reopened
            .mode_rows_seen_in(&mxr_core::AccountId::new(), "todo", &requested)
            .await
            .unwrap()
            .is_empty());
        assert!(!rows.contains(&("same-id".into(), "not-opened".into())));
        assert!(reopened
            .mode_rows_seen(&account.id, "updates")
            .await
            .unwrap()
            .is_empty());
        assert!(reopened
            .mode_rows_seen(&mxr_core::AccountId::new(), "todo")
            .await
            .unwrap()
            .is_empty());
        reopened.delete_account(&account.id).await.unwrap();
        assert!(reopened
            .mode_rows_seen(&account.id, "todo")
            .await
            .unwrap()
            .is_empty());
    }
}
