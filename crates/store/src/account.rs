use crate::{decode_id, decode_json, encode_json, trace_lookup, trace_query};
use mxr_core::{derive_account_tag, Account, AccountAddress, AccountColor, AccountId, BackendRef};

/// Why `remove_account_address` refuses the primary. A dry run returns the
/// same error, so the two paths cannot disagree.
pub const PRIMARY_REMOVAL_REFUSED: &str =
    "cannot remove the primary address; set another address as primary first";

/// An account's stored colour slot, with what the daemon needs to order
/// accounts when it resolves tags (blueprint 33).
#[derive(Debug, Clone)]
pub struct AccountIdentitySlot {
    pub account_id: AccountId,
    pub name: String,
    pub created_at: i64,
    pub color: Option<AccountColor>,
    pub tag: Option<String>,
}

/// `id, name, created_at, identity_color, identity_tag` from `accounts`.
type IdentitySlotRow = (String, String, i64, Option<String>, Option<String>);

impl super::Store {
    pub async fn insert_account(&self, account: &Account) -> Result<(), sqlx::Error> {
        self.insert_account_reserving(account, &[]).await
    }

    /// Inserts an account and gives it a colour and a tag. The tag is derived
    /// from the name, avoiding every stored tag and `reserved_tags`, the tags
    /// configured on accounts. Once stored, the tag is never derived again.
    pub async fn insert_account_reserving(
        &self,
        account: &Account,
        reserved_tags: &[String],
    ) -> Result<(), sqlx::Error> {
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

        // The colour and tag are picked and written in one transaction, so two
        // new accounts cannot take the same ones. ON CONFLICT leaves them alone,
        // so re-inserting an account (a rename, say) never moves them.
        let mut tx = self.writer().begin().await?;
        let others: Vec<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT identity_color, identity_tag FROM accounts WHERE id <> ?1")
                .bind(&id)
                .fetch_all(&mut *tx)
                .await?;
        let taken_colors: Vec<AccountColor> = others
            .iter()
            .filter_map(|(color, _)| color.as_deref()?.parse().ok())
            .collect();
        let mut taken_tags: std::collections::HashSet<String> =
            others.iter().filter_map(|(_, tag)| tag.clone()).collect();
        taken_tags.extend(reserved_tags.iter().cloned());
        let color = mxr_core::next_account_color(&taken_colors, others.len());
        let tag = derive_account_tag(&account.name, &taken_tags, &id)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;

        // Unchecked on purpose: a checked query! here would need a fresh .sqlx entry.
        sqlx::query(
            "INSERT INTO accounts (id, name, email, sync_provider, send_provider, sync_config, send_config, enabled, created_at, updated_at, identity_color, identity_tag)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                email = excluded.email,
                sync_provider = excluded.sync_provider,
                send_provider = excluded.send_provider,
                sync_config = excluded.sync_config,
                send_config = excluded.send_config,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&account.name)
        .bind(&account.email)
        .bind(sync_provider)
        .bind(send_provider)
        .bind(sync_config)
        .bind(send_config)
        .bind(account.enabled)
        .bind(now)
        .bind(now)
        .bind(color.as_str())
        .bind(&tag)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

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

    /// Removes an alias. A primary row is never deleted: the DELETE itself
    /// excludes it, so a concurrent promotion cannot race the check. Each
    /// owned-address reader treats the primary row as the account's own
    /// identity, so a missing row drops the account's own mail out of direction
    /// and reply defaults. Removing an address that is not registered is a
    /// no-op.
    pub async fn remove_account_address(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<(), sqlx::Error> {
        let aid = account_id.as_str();
        let removed = sqlx::query(
            "DELETE FROM account_addresses WHERE account_id = ? AND email = ? AND is_primary = 0",
        )
        .bind(aid.as_str())
        .bind(email)
        .execute(self.writer())
        .await?;
        if removed.rows_affected() == 0 {
            let is_primary = sqlx::query_scalar::<_, i64>(
                "SELECT 1 FROM account_addresses WHERE account_id = ? AND email = ? AND is_primary = 1",
            )
            .bind(aid.as_str())
            .bind(email)
            .fetch_optional(self.writer())
            .await?;
            if is_primary.is_some() {
                return Err(sqlx::Error::Protocol(PRIMARY_REMOVAL_REFUSED.to_string()));
            }
        }
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

    pub async fn list_all_accounts(&self) -> Result<Vec<Account>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows =
            sqlx::query("SELECT id, name, email, sync_config, send_config, enabled FROM accounts")
                .fetch_all(self.reader())
                .await?;
        trace_query("account.list_all_accounts", started_at, rows.len());

        rows.into_iter()
            .map(|row| {
                use sqlx::Row;

                let id: String = row.try_get("id")?;
                let name: String = row.try_get("name")?;
                let email: String = row.try_get("email")?;
                let sync_config: Option<String> = row.try_get("sync_config")?;
                let send_config: Option<String> = row.try_get("send_config")?;
                let enabled: bool = row.try_get("enabled")?;
                Ok(Account {
                    id: decode_id(&id)?,
                    name,
                    email,
                    sync_backend: sync_config
                        .map(|value| decode_json::<BackendRef>(&value))
                        .transpose()?,
                    send_backend: send_config
                        .map(|value| decode_json::<BackendRef>(&value))
                        .transpose()?,
                    enabled,
                })
            })
            .collect()
    }

    /// Every account's stored colour and tag, oldest first. A NULL value can
    /// only come from a row written before migration 094 ran.
    pub async fn list_account_identity_slots(
        &self,
    ) -> Result<Vec<AccountIdentitySlot>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows: Vec<IdentitySlotRow> = sqlx::query_as(
            "SELECT id, name, created_at, identity_color, identity_tag FROM accounts ORDER BY created_at, id",
        )
        .fetch_all(self.reader())
        .await?;
        trace_query(
            "account.list_account_identity_slots",
            started_at,
            rows.len(),
        );

        rows.into_iter()
            .map(|(id, name, created_at, color, tag)| {
                Ok(AccountIdentitySlot {
                    account_id: decode_id(&id)?,
                    name,
                    created_at,
                    color: color.and_then(|slot| slot.parse().ok()),
                    tag,
                })
            })
            .collect()
    }

    /// Fills in a colour and a tag for every account that lacks one, in
    /// creation order. Each takes the first colour no account holds and a tag
    /// derived against the tags already held. Runs on every open, so restored
    /// bundles from before migration 094 are covered too.
    pub(crate) async fn backfill_account_identity(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id, name, identity_color, identity_tag FROM accounts ORDER BY created_at, id",
        )
        .fetch_all(&mut *tx)
        .await?;
        let mut colors: Vec<AccountColor> = rows
            .iter()
            .filter_map(|(_, _, color, _)| color.as_deref()?.parse().ok())
            .collect();
        let mut tags: std::collections::HashSet<String> = rows
            .iter()
            .filter_map(|(_, _, _, tag)| tag.clone())
            .collect();
        for (index, (id, name, color, tag)) in rows.iter().enumerate() {
            if color.is_none() {
                let assigned = mxr_core::next_account_color(&colors, index);
                colors.push(assigned);
                sqlx::query("UPDATE accounts SET identity_color = ?1 WHERE id = ?2")
                    .bind(assigned.as_str())
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            if tag.is_none() {
                let assigned = derive_account_tag(name, &tags, id)
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
                sqlx::query("UPDATE accounts SET identity_tag = ?1 WHERE id = ?2")
                    .bind(&assigned)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                tags.insert(assigned);
            }
        }
        tx.commit().await?;
        Ok(())
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

    /// Deletes the account and, by cascade, all of its mail. The mail is
    /// recorded in `pending_message_forgets` in the same transaction, for
    /// the daemon to clear what lives outside SQLite.
    pub async fn delete_account(&self, id: &AccountId) -> Result<u64, sqlx::Error> {
        let id_str = id.as_str();
        let mut tx = self.writer().begin().await?;
        sqlx::query(
            "INSERT OR IGNORE INTO pending_message_forgets (message_id, account_id, deleted_at)
             SELECT id, account_id, unixepoch() FROM messages WHERE account_id = ?1",
        )
        .bind(&id_str)
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query("DELETE FROM accounts WHERE id = ?1")
            .bind(&id_str)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::Store;
    use mxr_core::AccountColor;

    fn account(name: &str) -> Account {
        Account {
            id: AccountId::new(),
            name: name.to_string(),
            email: format!("{}@example.com", name.to_lowercase()),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        }
    }

    async fn colour_of(store: &Store, id: &AccountId) -> Option<AccountColor> {
        store
            .list_account_identity_slots()
            .await
            .unwrap()
            .into_iter()
            .find(|slot| &slot.account_id == id)
            .unwrap()
            .color
    }

    #[tokio::test]
    async fn new_accounts_take_the_first_free_slot() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        let wedding = account("Wedding");
        store.insert_account(&work).await.unwrap();
        store.insert_account(&wedding).await.unwrap();
        assert_eq!(colour_of(&store, &work.id).await, Some(AccountColor::Sky));
        assert_eq!(
            colour_of(&store, &wedding.id).await,
            Some(AccountColor::Amber)
        );
    }

    #[tokio::test]
    async fn re_inserting_an_account_keeps_its_slot() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        store.insert_account(&work).await.unwrap();
        store.insert_account(&account("Wedding")).await.unwrap();
        store.insert_account(&work).await.unwrap();
        assert_eq!(colour_of(&store, &work.id).await, Some(AccountColor::Sky));
    }

    #[tokio::test]
    async fn a_removed_accounts_slot_goes_to_the_next_new_account() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        let wedding = account("Wedding");
        let travel = account("Travel");
        store.insert_account(&work).await.unwrap();
        store.insert_account(&wedding).await.unwrap();
        store.insert_account(&travel).await.unwrap();
        store.delete_account(&wedding.id).await.unwrap();
        let school = account("School");
        store.insert_account(&school).await.unwrap();
        assert_eq!(colour_of(&store, &work.id).await, Some(AccountColor::Sky));
        assert_eq!(
            colour_of(&store, &travel.id).await,
            Some(AccountColor::Green)
        );
        assert_eq!(
            colour_of(&store, &school.id).await,
            Some(AccountColor::Amber)
        );
    }

    #[tokio::test]
    async fn the_ninth_account_repeats_the_slots_in_order() {
        let store = Store::in_memory().await.unwrap();
        let mut ids = Vec::new();
        for n in 0..9 {
            let account = account(&format!("Account {n}"));
            store.insert_account(&account).await.unwrap();
            ids.push(account.id);
        }
        assert_eq!(colour_of(&store, &ids[8]).await, Some(AccountColor::Sky));
    }

    #[tokio::test]
    async fn backfill_gives_unslotted_accounts_colours_in_creation_order() {
        let store = Store::in_memory().await.unwrap();
        let older = account("Older");
        let newer = account("Newer");
        store.insert_account(&older).await.unwrap();
        store.insert_account(&newer).await.unwrap();
        // A row restored from a bundle written before migration 094.
        sqlx::query("UPDATE accounts SET identity_color = NULL, created_at = CASE WHEN id = ?1 THEN 20 ELSE 10 END")
            .bind(older.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        store.backfill_account_identity().await.unwrap();
        assert_eq!(colour_of(&store, &newer.id).await, Some(AccountColor::Sky));
        assert_eq!(
            colour_of(&store, &older.id).await,
            Some(AccountColor::Amber)
        );
    }

    #[tokio::test]
    async fn backfill_leaves_stored_slots_alone() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        store.insert_account(&work).await.unwrap();
        sqlx::query("UPDATE accounts SET identity_color = 'slate'")
            .execute(store.writer())
            .await
            .unwrap();
        store.backfill_account_identity().await.unwrap();
        assert_eq!(colour_of(&store, &work.id).await, Some(AccountColor::Slate));
    }

    async fn tag_of(store: &Store, id: &AccountId) -> Option<String> {
        store
            .list_account_identity_slots()
            .await
            .unwrap()
            .into_iter()
            .find(|slot| &slot.account_id == id)
            .unwrap()
            .tag
    }

    #[tokio::test]
    async fn backfill_takes_the_first_free_colour_beside_a_stored_one() {
        // The older account has no colour and the newer one holds sky. Absolute
        // rank would give the older one sky too; first-free gives it amber.
        let store = Store::in_memory().await.unwrap();
        let older = account("Older");
        let newer = account("Newer");
        store.insert_account(&older).await.unwrap();
        store.insert_account(&newer).await.unwrap();
        sqlx::query("UPDATE accounts SET identity_color = NULL, identity_tag = NULL, created_at = CASE WHEN id = ?1 THEN 10 ELSE 20 END")
            .bind(older.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        sqlx::query("UPDATE accounts SET identity_color = 'sky' WHERE id = ?1")
            .bind(newer.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        store.backfill_account_identity().await.unwrap();
        assert_eq!(
            colour_of(&store, &older.id).await,
            Some(AccountColor::Amber)
        );
        assert_eq!(colour_of(&store, &newer.id).await, Some(AccountColor::Sky));
    }

    #[tokio::test]
    async fn backfill_derives_tags_in_creation_order() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        let wedding = account("Wedding");
        store.insert_account(&work).await.unwrap();
        store.insert_account(&wedding).await.unwrap();
        sqlx::query("UPDATE accounts SET identity_tag = NULL, created_at = CASE WHEN id = ?1 THEN 10 ELSE 20 END")
            .bind(work.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        store.backfill_account_identity().await.unwrap();
        assert_eq!(tag_of(&store, &work.id).await.as_deref(), Some("W"));
        assert_eq!(tag_of(&store, &wedding.id).await.as_deref(), Some("WE"));
    }

    #[tokio::test]
    async fn stored_tags_survive_a_rename_a_removal_and_a_new_account() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        let wedding = account("Wedding");
        let travel = account("Travel");
        store.insert_account(&work).await.unwrap();
        store.insert_account(&wedding).await.unwrap();
        store.insert_account(&travel).await.unwrap();
        assert_eq!(tag_of(&store, &work.id).await.as_deref(), Some("W"));
        assert_eq!(tag_of(&store, &wedding.id).await.as_deref(), Some("WE"));

        // Renaming Work to Office used to re-derive W from the new name, which
        // is the change that moved other accounts. Now the stored tag stays.
        let renamed = Account {
            name: "Office".to_string(),
            ..work.clone()
        };
        store.insert_account(&renamed).await.unwrap();
        store.delete_account(&wedding.id).await.unwrap();
        let fourth = account("Wedding");
        store.insert_account(&fourth).await.unwrap();

        assert_eq!(tag_of(&store, &work.id).await.as_deref(), Some("W"));
        assert_eq!(tag_of(&store, &travel.id).await.as_deref(), Some("T"));
        // The removed account's WE is free again, so the new one takes it.
        assert_eq!(tag_of(&store, &fourth.id).await.as_deref(), Some("WE"));
    }

    #[tokio::test]
    async fn a_new_account_avoids_the_tags_reserved_for_configured_accounts() {
        let store = Store::in_memory().await.unwrap();
        let work = account("Work");
        store
            .insert_account_reserving(&work, &["W".to_string()])
            .await
            .unwrap();
        assert_eq!(tag_of(&store, &work.id).await.as_deref(), Some("WO"));
    }
}
