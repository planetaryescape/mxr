//! Persistence for the mutation dedup log.
//!
//! Idempotent-retry safety: before calling `provider.apply_mutation`,
//! the daemon checks whether the `(mutation_id, provider_message_id)`
//! pair has already been recorded. On success it writes a row. A
//! replay of the same mutation_id (e.g. after a daemon restart) finds
//! the existing row and skips the provider call.
//!
//! TTL is 24h; expired rows are pruned alongside the undo log.

use mxr_core::AccountId;
#[cfg(test)]
use sqlx::Row;

/// 24h dedup window, matching the modern REST convention (Stripe et al).
pub const DEDUP_WINDOW_SECS: i64 = 24 * 60 * 60;

/// The stored key. The table's primary key is (mutation_id, key) and the key
/// is opaque to everything else, so the account rides inside it.
fn scoped_key(account_id: &AccountId, provider_message_id: &str) -> String {
    format!("{account_id}:{provider_message_id}")
}

impl super::Store {
    /// Returns true iff this (mutation_id, account, provider_message_id)
    /// triple has already been recorded as applied. The account is part of
    /// the key because provider ids are only unique within an account: two
    /// IMAP accounts can both hold `INBOX:42`.
    pub async fn was_mutation_applied(
        &self,
        mutation_id: &str,
        account_id: &AccountId,
        provider_message_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let provider_message_id = scoped_key(account_id, provider_message_id);
        let row = sqlx::query(
            "SELECT 1 AS hit FROM mutation_dedup_log
             WHERE mutation_id = ? AND provider_message_id = ?
             LIMIT 1",
        )
        .bind(mutation_id)
        .bind(&provider_message_id)
        .fetch_optional(self.reader())
        .await?;
        Ok(row.is_some())
    }

    /// Record a successful mutation apply. Idempotent — a second call
    /// with the same composite key is a no-op via INSERT OR IGNORE.
    pub async fn record_mutation_applied(
        &self,
        mutation_id: &str,
        provider_message_id: &str,
        account_id: &AccountId,
        applied_at: i64,
    ) -> Result<(), sqlx::Error> {
        let expires_at = applied_at.saturating_add(DEDUP_WINDOW_SECS);
        let provider_message_id = scoped_key(account_id, provider_message_id);
        sqlx::query(
            "INSERT OR IGNORE INTO mutation_dedup_log
             (mutation_id, provider_message_id, account_id, applied_at, expires_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(mutation_id)
        .bind(&provider_message_id)
        .bind(account_id.as_str())
        .bind(applied_at)
        .bind(expires_at)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Drop every dedup row whose `expires_at` is at or before `now`.
    /// Mirrors `prune_expired_undo_entries`; called from the daemon
    /// maintenance loop on the same cadence.
    pub async fn prune_expired_mutation_dedup(&self, now: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM mutation_dedup_log WHERE expires_at <= ?")
            .bind(now)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }

    #[cfg(test)]
    pub(crate) async fn count_mutation_dedup_rows(&self) -> Result<i64, sqlx::Error> {
        let row = sqlx::query("SELECT COUNT(*) AS n FROM mutation_dedup_log")
            .fetch_one(self.reader())
            .await?;
        let n: i64 = row.try_get("n")?;
        Ok(n)
    }
}
