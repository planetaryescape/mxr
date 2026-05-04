use crate::{decode_id, decode_json, encode_json, trace_lookup, trace_query};
use mxr_core::{Account, AccountAddress, AccountId, BackendRef};

impl super::Store {
    pub async fn insert_account(&self, account: &Account) -> Result<(), sqlx::Error> {
        let id = account.id.as_str();
        let sync_provider = account
            .sync_backend
            .as_ref()
            .map(|backend| encode_json(&backend.provider_kind))
            .transpose()?;
        let send_provider = account
            .send_backend
            .as_ref()
            .map(|backend| encode_json(&backend.provider_kind))
            .transpose()?;
        let sync_config = account.sync_backend.as_ref().map(encode_json).transpose()?;
        let send_config = account.send_backend.as_ref().map(encode_json).transpose()?;
        let now = chrono::Utc::now().timestamp();

        sqlx::query!(
            "INSERT INTO accounts (id, name, email, sync_provider, send_provider, sync_config, send_config, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                email = excluded.email,
                sync_provider = excluded.sync_provider,
                send_provider = excluded.send_provider,
                sync_config = excluded.sync_config,
                send_config = excluded.send_config,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
            id,
            account.name,
            account.email,
            sync_provider,
            send_provider,
            sync_config,
            send_config,
            account.enabled,
            now,
            now,
        )
        .execute(self.writer())
        .await?;

        // Seed the canonical account email as the primary address. Idempotent
        // via INSERT OR IGNORE — re-inserting an existing account does not
        // duplicate or demote.
        if !account.email.is_empty() {
            sqlx::query!(
                "INSERT OR IGNORE INTO account_addresses (account_id, email, is_primary)
                 VALUES (?, ?, 1)",
                id,
                account.email,
            )
            .execute(self.writer())
            .await?;
        }

        Ok(())
    }

    /// Add an alias address. Pass `is_primary=true` to also demote the
    /// existing primary atomically; the partial unique index enforces "at most
    /// one primary per account" at the storage layer.
    pub async fn add_account_address(
        &self,
        account_id: &AccountId,
        email: &str,
        is_primary: bool,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        if is_primary {
            // Demote any existing primary in the same transaction-like step
            // before inserting the new one.
            sqlx::query!(
                "UPDATE account_addresses SET is_primary = 0 WHERE account_id = ?",
                aid,
            )
            .execute(self.writer())
            .await?;
        }
        let primary_int = if is_primary { 1i64 } else { 0 };
        sqlx::query!(
            "INSERT INTO account_addresses (account_id, email, is_primary)
             VALUES (?, ?, ?)
             ON CONFLICT(account_id, email) DO UPDATE SET is_primary = excluded.is_primary",
            aid,
            email,
            primary_int,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_account_addresses(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<AccountAddress>, sqlx::Error> {
        let aid = account_id.as_str();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT
                account_id  as "account_id!",
                email       as "email!",
                is_primary  as "is_primary!: bool"
               FROM account_addresses
               WHERE account_id = ?
               ORDER BY is_primary DESC, email ASC"#,
            aid,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("account.list_account_addresses", started_at, rows.len());
        rows.into_iter()
            .map(|r| {
                Ok(AccountAddress {
                    account_id: decode_id(&r.account_id)?,
                    email: r.email,
                    is_primary: r.is_primary,
                })
            })
            .collect()
    }

    /// Promote an existing address to primary, demoting any prior primary.
    /// Errors if the address does not exist on this account.
    pub async fn set_primary_address(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        sqlx::query!(
            "UPDATE account_addresses SET is_primary = 0 WHERE account_id = ?",
            aid,
        )
        .execute(self.writer())
        .await?;
        let result = sqlx::query!(
            "UPDATE account_addresses SET is_primary = 1
             WHERE account_id = ? AND email = ?",
            aid,
            email,
        )
        .execute(self.writer())
        .await?;
        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    pub async fn remove_account_address(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        sqlx::query!(
            "DELETE FROM account_addresses WHERE account_id = ? AND email = ?",
            aid,
            email,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Snapshot of every owned address across every account. Used by
    /// `AppState::refresh_account_addresses` to populate the in-memory cache
    /// the sync engine consults during direction inference.
    pub async fn list_all_account_addresses(&self) -> Result<Vec<AccountAddress>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT
                account_id  as "account_id!",
                email       as "email!",
                is_primary  as "is_primary!: bool"
               FROM account_addresses"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("account.list_all_account_addresses", started_at, rows.len());
        rows.into_iter()
            .map(|r| {
                Ok(AccountAddress {
                    account_id: decode_id(&r.account_id)?,
                    email: r.email,
                    is_primary: r.is_primary,
                })
            })
            .collect()
    }

    pub async fn get_account(&self, id: &AccountId) -> Result<Option<Account>, sqlx::Error> {
        let id_str = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", email as "email!", sync_config, send_config, enabled as "enabled!: bool" FROM accounts WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("account.get_account", started_at, row.is_some());

        row.map(|row| {
            Ok(Account {
                id: decode_id(&row.id)?,
                name: row.name,
                email: row.email,
                sync_backend: row
                    .sync_config
                    .map(|value| decode_json::<BackendRef>(&value))
                    .transpose()?,
                send_backend: row
                    .send_config
                    .map(|value| decode_json::<BackendRef>(&value))
                    .transpose()?,
                enabled: row.enabled,
            })
        })
        .transpose()
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", email as "email!", sync_config, send_config, enabled as "enabled!: bool" FROM accounts WHERE enabled = 1"#
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("account.list_accounts", started_at, rows.len());

        rows.into_iter()
            .map(|row| {
                Ok(Account {
                    id: decode_id(&row.id)?,
                    name: row.name,
                    email: row.email,
                    sync_backend: row
                        .sync_config
                        .map(|value| decode_json::<BackendRef>(&value))
                        .transpose()?,
                    send_backend: row
                        .send_config
                        .map(|value| decode_json::<BackendRef>(&value))
                        .transpose()?,
                    enabled: row.enabled,
                })
            })
            .collect()
    }

    pub async fn set_account_enabled(
        &self,
        id: &AccountId,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        let id_str = id.as_str();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("UPDATE accounts SET enabled = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(enabled)
            .bind(now)
            .bind(id_str)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn delete_account(&self, id: &AccountId) -> Result<u64, sqlx::Error> {
        let id_str = id.as_str();
        let result = sqlx::query("DELETE FROM accounts WHERE id = ?1")
            .bind(id_str)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }
}
