use mxr_core::{Account, AccountId, BackendRef};

impl super::Store {
    pub async fn insert_account(&self, account: &Account) -> Result<(), sqlx::Error> {
        let id = account.id.as_str();
        let sync_provider = account
            .sync_backend
            .as_ref()
            .map(|b| serde_json::to_string(&b.provider_kind).unwrap());
        let send_provider = account
            .send_backend
            .as_ref()
            .map(|b| serde_json::to_string(&b.provider_kind).unwrap());
        let sync_config = account
            .sync_backend
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap());
        let send_config = account
            .send_backend
            .as_ref()
            .map(|b| serde_json::to_string(b).unwrap());
        let now = chrono::Utc::now().timestamp();

        sqlx::query!(
            "INSERT INTO accounts (id, name, email, sync_provider, send_provider, sync_config, send_config, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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

        Ok(())
    }

    pub async fn get_account(&self, id: &AccountId) -> Result<Option<Account>, sqlx::Error> {
        let id_str = id.as_str();
        let row = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", email as "email!", sync_config, send_config, enabled as "enabled!: bool" FROM accounts WHERE id = ?"#,
            id_str,
        )
        .fetch_optional(self.reader())
        .await?;

        Ok(row.map(|r| Account {
            id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
            name: r.name,
            email: r.email,
            sync_backend: r
                .sync_config
                .and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
            send_backend: r
                .send_config
                .and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
            enabled: r.enabled,
        }))
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", email as "email!", sync_config, send_config, enabled as "enabled!: bool" FROM accounts WHERE enabled = 1"#
        )
        .fetch_all(self.reader())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Account {
                id: AccountId::from_uuid(uuid::Uuid::parse_str(&r.id).unwrap()),
                name: r.name,
                email: r.email,
                sync_backend: r
                    .sync_config
                    .and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
                send_backend: r
                    .send_config
                    .and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
                enabled: r.enabled,
            })
            .collect())
    }
}
