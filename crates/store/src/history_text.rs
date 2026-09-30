//! Fingerprints of sentences AI wrote from a user's history; see migration
//! 055. The daemon computes the fingerprints; this stores and finds them.
//! Nothing here holds text.

use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;

use crate::{decode_id, encode_json, trace_query};

impl super::Store {
    /// Remember `fingerprints` for `account_id` as of `now`, then drop that
    /// account's rows from before `expired_before` and all but its newest
    /// `cap`.
    pub async fn record_history_fingerprints(
        &self,
        account_id: &AccountId,
        fingerprints: &[i64],
        now: DateTime<Utc>,
        expired_before: DateTime<Utc>,
        cap: u32,
    ) -> Result<(), sqlx::Error> {
        if fingerprints.is_empty() {
            return Ok(());
        }
        let started_at = std::time::Instant::now();
        let mut tx = self.writer().begin().await?;
        sqlx::query(
            "INSERT INTO history_text_fingerprints (fingerprint, account_id, created_at)
             SELECT value, ?2, ?3 FROM json_each(?1) WHERE true
             ON CONFLICT(fingerprint, account_id) DO UPDATE SET created_at = excluded.created_at",
        )
        .bind(encode_json(&fingerprints)?)
        .bind(account_id.as_str())
        .bind(now.timestamp())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM history_text_fingerprints WHERE account_id = ?1 AND created_at < ?2",
        )
        .bind(account_id.as_str())
        .bind(expired_before.timestamp())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM history_text_fingerprints
             WHERE account_id = ?1 AND fingerprint IN (
                 SELECT fingerprint FROM history_text_fingerprints
                 WHERE account_id = ?1
                 ORDER BY created_at DESC
                 LIMIT -1 OFFSET ?2
             )",
        )
        .bind(account_id.as_str())
        .bind(i64::from(cap))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        trace_query("history_text.record", started_at, fingerprints.len());
        Ok(())
    }

    /// The accounts that recorded any of `fingerprints` since `since`:
    /// just `account_id` when given, else every one that did.
    pub async fn history_fingerprint_accounts(
        &self,
        account_id: Option<&AccountId>,
        fingerprints: &[i64],
        since: DateTime<Utc>,
    ) -> Result<Vec<AccountId>, sqlx::Error> {
        if fingerprints.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = std::time::Instant::now();
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT f.account_id FROM json_each(?1) wanted
             CROSS JOIN history_text_fingerprints f ON f.fingerprint = wanted.value
             WHERE (?2 IS NULL OR f.account_id = ?2) AND f.created_at >= ?3",
        )
        .bind(encode_json(&fingerprints)?)
        .bind(account_id.map(AccountId::as_str))
        .bind(since.timestamp())
        .fetch_all(self.reader())
        .await?;
        trace_query("history_text.lookup", started_at, rows.len());
        rows.iter().map(|id| decode_id(id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::test_account;
    use crate::Store;
    use chrono::{DateTime, Duration, Utc};
    use mxr_core::AccountId;

    async fn has(
        store: &Store,
        account: Option<&AccountId>,
        fps: &[i64],
        since: DateTime<Utc>,
    ) -> bool {
        !store
            .history_fingerprint_accounts(account, fps, since)
            .await
            .unwrap()
            .is_empty()
    }

    #[tokio::test]
    async fn fingerprints_are_found_per_account_expire_and_stay_under_the_cap() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let now = Utc::now();
        let week_ago = now - Duration::days(7);

        store
            .record_history_fingerprints(&account.id, &[1, 2, 3], now, week_ago, 10)
            .await
            .unwrap();
        // Recording again is an upsert, not a duplicate.
        store
            .record_history_fingerprints(&account.id, &[3], now, week_ago, 10)
            .await
            .unwrap();
        assert!(has(&store, Some(&account.id), &[9, 2], week_ago).await);
        assert!(has(&store, None, &[3], week_ago).await);
        assert!(!has(&store, Some(&AccountId::new()), &[2], week_ago).await);
        assert!(!has(&store, Some(&account.id), &[2], now + Duration::seconds(1)).await);

        // Past the cap, the oldest go.
        store
            .record_history_fingerprints(
                &account.id,
                &[4, 5],
                now + Duration::seconds(5),
                week_ago,
                2,
            )
            .await
            .unwrap();
        assert!(has(&store, Some(&account.id), &[4, 5], week_ago).await);
        assert!(!has(&store, Some(&account.id), &[1, 2, 3], week_ago).await);

        // Past the TTL, on the next insert, they go too.
        store
            .record_history_fingerprints(
                &account.id,
                &[6],
                now + Duration::days(8),
                now + Duration::days(1),
                10,
            )
            .await
            .unwrap();
        assert!(!has(&store, Some(&account.id), &[4, 5], week_ago).await);
    }
}
