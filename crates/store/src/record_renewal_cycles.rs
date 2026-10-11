//! Durable identity for the annual cycle represented by a merged charge.

use mxr_core::id::AccountId;
use std::collections::BTreeSet;

impl super::Store {
    /// Resolve all aliases together under the writer, so concurrent ticks
    /// agree even when a duplicate invoice adds an earlier-sorting id.
    /// Preview takes the same path and rolls its aliases back. A committed
    /// alias without a task is safe: replay uses its key to create the task.
    pub async fn record_renewal_cycle_key(
        &self,
        account: &AccountId,
        record_ids: &[String],
        commit: bool,
    ) -> Result<String, sqlx::Error> {
        let Some(first) = record_ids.iter().min() else {
            return Err(sqlx::Error::Protocol(
                "renewal charge has no records".into(),
            ));
        };
        let mut tx = self.writer().begin().await?;
        let mut keys = BTreeSet::new();
        for id in record_ids {
            if let Some(key) = sqlx::query_scalar::<_, String>(
                "SELECT dedup_key FROM record_renewal_cycles WHERE account_id = ? AND record_id = ?",
            )
            .bind(account.as_str())
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            {
                keys.insert(key);
            }
        }
        if keys.len() > 1 {
            return Err(sqlx::Error::Protocol(
                "renewal charge joins multiple existing cycles; keeping their tasks unchanged"
                    .into(),
            ));
        }
        let key = keys
            .into_iter()
            .next()
            .unwrap_or_else(|| format!("record-renewal|{first}"));
        for id in record_ids {
            sqlx::query(
                "INSERT INTO record_renewal_cycles (account_id, record_id, dedup_key)
                 VALUES (?, ?, ?) ON CONFLICT(account_id, record_id) DO NOTHING",
            )
            .bind(account.as_str())
            .bind(id)
            .bind(&key)
            .execute(&mut *tx)
            .await?;
        }
        if commit {
            tx.commit().await?;
        } else {
            tx.rollback().await?;
        }
        Ok(key)
    }
}
