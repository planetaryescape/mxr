use mxr_core::{Account, AccountId, BackendRef};
use sqlx::Row;

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

        sqlx::query(
            "INSERT INTO accounts (id, name, email, sync_provider, send_provider, sync_config, send_config, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&account.name)
        .bind(&account.email)
        .bind(&sync_provider)
        .bind(&send_provider)
        .bind(&sync_config)
        .bind(&send_config)
        .bind(account.enabled)
        .bind(now)
        .bind(now)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn get_account(&self, id: &AccountId) -> Result<Option<Account>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM accounts WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(self.reader())
            .await?;

        match row {
            Some(row) => Ok(Some(row_to_account(&row))),
            None => Ok(None),
        }
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM accounts WHERE enabled = 1")
            .fetch_all(self.reader())
            .await?;

        Ok(rows.iter().map(row_to_account).collect())
    }
}

fn row_to_account(row: &sqlx::sqlite::SqliteRow) -> Account {
    let id_str: String = row.get("id");
    let sync_config: Option<String> = row.get("sync_config");
    let send_config: Option<String> = row.get("send_config");

    Account {
        id: AccountId::from_uuid(uuid::Uuid::parse_str(&id_str).unwrap()),
        name: row.get("name"),
        email: row.get("email"),
        sync_backend: sync_config.and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
        send_backend: send_config.and_then(|c| serde_json::from_str::<BackendRef>(&c).ok()),
        enabled: row.get::<bool, _>("enabled"),
    }
}
