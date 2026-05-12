//! Local outgoing email signatures and scoped defaults.

use crate::{decode_id, decode_timestamp, trace_lookup, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::{AccountId, SignatureId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureKind {
    New,
    Reply,
}

impl SignatureKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Reply => "reply",
        }
    }

    fn from_db_str(value: &str) -> Result<Self, sqlx::Error> {
        match value {
            "new" => Ok(Self::New),
            "reply" => Ok(Self::Reply),
            other => Err(sqlx::Error::Protocol(format!(
                "invalid signature kind: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureScope {
    Global,
    Account(AccountId),
    Address {
        account_id: AccountId,
        email: String,
    },
}

impl SignatureScope {
    pub fn address(account_id: AccountId, email: String) -> Self {
        Self::Address {
            account_id,
            email: normalize_email(&email),
        }
    }

    fn scope_key(&self) -> String {
        match self {
            Self::Global => "global".to_string(),
            Self::Account(account_id) => format!("account:{account_id}"),
            Self::Address { account_id, email } => {
                format!("address:{account_id}:{}", normalize_email(email))
            }
        }
    }

    fn account_id_str(&self) -> Option<String> {
        match self {
            Self::Global => None,
            Self::Account(account_id) | Self::Address { account_id, .. } => {
                Some(account_id.as_str())
            }
        }
    }

    fn from_email(&self) -> Option<String> {
        match self {
            Self::Global | Self::Account(_) => None,
            Self::Address { email, .. } => Some(normalize_email(email)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub id: SignatureId,
    pub name: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureDefault {
    pub scope: SignatureScope,
    pub kind: SignatureKind,
    pub signature: Signature,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl super::Store {
    pub async fn upsert_signature(&self, signature: &Signature) -> Result<(), sqlx::Error> {
        let id = signature.id.as_str();
        let created_at = signature.created_at.timestamp();
        let updated_at = signature.updated_at.timestamp();
        sqlx::query!(
            r#"INSERT INTO signatures (id, name, body, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   body = excluded.body,
                   updated_at = excluded.updated_at"#,
            id,
            signature.name,
            signature.body,
            created_at,
            updated_at,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn get_signature(&self, id: &SignatureId) -> Result<Option<Signature>, sqlx::Error> {
        let id = id.as_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", body as "body!",
                      created_at as "created_at!", updated_at as "updated_at!"
               FROM signatures WHERE id = ?"#,
            id,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("signatures.get", started_at, row.is_some());
        row.map(|row| row_to_signature(row.id, row.name, row.body, row.created_at, row.updated_at))
            .transpose()
    }

    pub async fn get_signature_by_name(
        &self,
        name: &str,
    ) -> Result<Option<Signature>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", body as "body!",
                      created_at as "created_at!", updated_at as "updated_at!"
               FROM signatures WHERE name = ?"#,
            name,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("signatures.get_by_name", started_at, row.is_some());
        row.map(|row| row_to_signature(row.id, row.name, row.body, row.created_at, row.updated_at))
            .transpose()
    }

    pub async fn list_signatures(&self) -> Result<Vec<Signature>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT id as "id!", name as "name!", body as "body!",
                      created_at as "created_at!", updated_at as "updated_at!"
               FROM signatures ORDER BY name ASC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("signatures.list", started_at, rows.len());
        rows.into_iter()
            .map(|row| row_to_signature(row.id, row.name, row.body, row.created_at, row.updated_at))
            .collect()
    }

    pub async fn delete_signature_by_name(&self, name: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM signatures WHERE name = ?", name)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_signature_default(
        &self,
        scope: &SignatureScope,
        kind: SignatureKind,
        signature_id: &SignatureId,
    ) -> Result<(), sqlx::Error> {
        let scope_key = scope.scope_key();
        let kind = kind.as_db_str();
        let signature_id = signature_id.as_str();
        let account_id = scope.account_id_str();
        let from_email = scope.from_email();
        let now = Utc::now().timestamp();
        sqlx::query!(
            r#"INSERT INTO signature_defaults
                   (scope_key, kind, signature_id, account_id, from_email, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(scope_key, kind) DO UPDATE SET
                   signature_id = excluded.signature_id,
                   account_id = excluded.account_id,
                   from_email = excluded.from_email,
                   updated_at = excluded.updated_at"#,
            scope_key,
            kind,
            signature_id,
            account_id,
            from_email,
            now,
            now,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn clear_signature_default(
        &self,
        scope: &SignatureScope,
        kind: SignatureKind,
    ) -> Result<bool, sqlx::Error> {
        let scope_key = scope.scope_key();
        let kind = kind.as_db_str();
        let result = sqlx::query!(
            "DELETE FROM signature_defaults WHERE scope_key = ? AND kind = ?",
            scope_key,
            kind,
        )
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_signature_defaults(&self) -> Result<Vec<SignatureDefault>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT d.scope_key as "scope_key!", d.kind as "kind!",
                      d.account_id, d.from_email,
                      d.created_at as "default_created_at!",
                      d.updated_at as "default_updated_at!",
                      s.id as "signature_id!", s.name as "signature_name!",
                      s.body as "signature_body!",
                      s.created_at as "signature_created_at!",
                      s.updated_at as "signature_updated_at!"
               FROM signature_defaults d
               JOIN signatures s ON s.id = d.signature_id
               ORDER BY d.scope_key ASC, d.kind ASC"#,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("signatures.defaults.list", started_at, rows.len());
        rows.into_iter()
            .map(|row| {
                let scope = decode_scope(row.scope_key, row.account_id, row.from_email)?;
                let signature = row_to_signature(
                    row.signature_id,
                    row.signature_name,
                    row.signature_body,
                    row.signature_created_at,
                    row.signature_updated_at,
                )?;
                Ok(SignatureDefault {
                    scope,
                    kind: SignatureKind::from_db_str(&row.kind)?,
                    signature,
                    created_at: decode_timestamp(row.default_created_at)?,
                    updated_at: decode_timestamp(row.default_updated_at)?,
                })
            })
            .collect()
    }

    pub async fn resolve_signature(
        &self,
        account_id: Option<&AccountId>,
        from_email: Option<&str>,
        kind: SignatureKind,
    ) -> Result<Option<Signature>, sqlx::Error> {
        let keys = resolution_scope_keys(account_id, from_email);
        let first = keys.first().cloned().unwrap_or_default();
        let second = keys.get(1).cloned().unwrap_or_default();
        let third = keys.get(2).cloned().unwrap_or_default();
        let kind = kind.as_db_str();
        let started_at = std::time::Instant::now();
        let row = sqlx::query!(
            r#"SELECT s.id as "id!", s.name as "name!", s.body as "body!",
                      s.created_at as "created_at!", s.updated_at as "updated_at!"
               FROM signature_defaults d
               JOIN signatures s ON s.id = d.signature_id
               WHERE d.kind = ? AND d.scope_key IN (?, ?, ?)
               ORDER BY CASE d.scope_key
                   WHEN ? THEN 0
                   WHEN ? THEN 1
                   WHEN ? THEN 2
                   ELSE 3
               END
               LIMIT 1"#,
            kind,
            first,
            second,
            third,
            first,
            second,
            third,
        )
        .fetch_optional(self.reader())
        .await?;
        trace_lookup("signatures.resolve", started_at, row.is_some());
        row.map(|row| row_to_signature(row.id, row.name, row.body, row.created_at, row.updated_at))
            .transpose()
    }
}

fn row_to_signature(
    id: String,
    name: String,
    body: String,
    created_at: i64,
    updated_at: i64,
) -> Result<Signature, sqlx::Error> {
    Ok(Signature {
        id: decode_id(&id)?,
        name,
        body,
        created_at: decode_timestamp(created_at)?,
        updated_at: decode_timestamp(updated_at)?,
    })
}

fn decode_scope(
    scope_key: String,
    account_id: Option<String>,
    from_email: Option<String>,
) -> Result<SignatureScope, sqlx::Error> {
    if scope_key == "global" {
        return Ok(SignatureScope::Global);
    }
    let Some(account_id) = account_id else {
        return Err(sqlx::Error::Protocol(format!(
            "signature scope {scope_key} is missing account_id"
        )));
    };
    let account_id = decode_id(&account_id)?;
    Ok(match from_email {
        Some(email) => SignatureScope::address(account_id, email),
        None => SignatureScope::Account(account_id),
    })
}

fn resolution_scope_keys(account_id: Option<&AccountId>, from_email: Option<&str>) -> Vec<String> {
    let mut keys = Vec::with_capacity(3);
    if let Some(account_id) = account_id {
        if let Some(email) = from_email.map(str::trim).filter(|email| !email.is_empty()) {
            keys.push(SignatureScope::address(account_id.clone(), email.to_string()).scope_key());
        }
        keys.push(SignatureScope::Account(account_id.clone()).scope_key());
    }
    keys.push(SignatureScope::Global.scope_key());
    keys
}

fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use chrono::{TimeZone, Utc};
    use mxr_core::Account;

    fn anchor() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 12, 9, 0, 0).unwrap()
    }

    fn signature(name: &str, body: &str) -> Signature {
        Signature {
            id: SignatureId::new(),
            name: name.into(),
            body: body.into(),
            created_at: anchor(),
            updated_at: anchor(),
        }
    }

    async fn insert_account(store: &Store, email: &str) -> AccountId {
        let id = AccountId::new();
        store
            .insert_account(&Account {
                id: id.clone(),
                name: email.into(),
                email: email.into(),
                sync_backend: None,
                send_backend: None,
                enabled: true,
            })
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn upsert_get_list_and_delete_round_trip() {
        let store = Store::in_memory().await.unwrap();
        let sig = signature("Work", "-- \nAlice");
        store.upsert_signature(&sig).await.unwrap();

        assert_eq!(
            store.get_signature(&sig.id).await.unwrap(),
            Some(sig.clone())
        );
        assert_eq!(store.list_signatures().await.unwrap(), vec![sig.clone()]);
        assert!(store.delete_signature_by_name("Work").await.unwrap());
        assert!(store.get_signature(&sig.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn resolve_prefers_address_then_account_then_global() {
        let store = Store::in_memory().await.unwrap();
        let account_id = insert_account(&store, "me@example.com").await;
        let global = signature("Global", "global");
        let account = signature("Account", "account");
        let address = signature("Alias", "alias");
        store.upsert_signature(&global).await.unwrap();
        store.upsert_signature(&account).await.unwrap();
        store.upsert_signature(&address).await.unwrap();

        store
            .set_signature_default(&SignatureScope::Global, SignatureKind::New, &global.id)
            .await
            .unwrap();
        store
            .set_signature_default(
                &SignatureScope::Account(account_id.clone()),
                SignatureKind::New,
                &account.id,
            )
            .await
            .unwrap();
        store
            .set_signature_default(
                &SignatureScope::address(account_id.clone(), "ALIAS@example.com".into()),
                SignatureKind::New,
                &address.id,
            )
            .await
            .unwrap();

        let resolved = store
            .resolve_signature(
                Some(&account_id),
                Some("alias@example.com"),
                SignatureKind::New,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.name, "Alias");

        let resolved = store
            .resolve_signature(
                Some(&account_id),
                Some("other@example.com"),
                SignatureKind::New,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.name, "Account");

        let resolved = store
            .resolve_signature(None, None, SignatureKind::New)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.name, "Global");
    }

    #[tokio::test]
    async fn reply_defaults_do_not_fall_back_to_new_defaults() {
        let store = Store::in_memory().await.unwrap();
        let sig = signature("New", "new only");
        store.upsert_signature(&sig).await.unwrap();
        store
            .set_signature_default(&SignatureScope::Global, SignatureKind::New, &sig.id)
            .await
            .unwrap();

        assert!(store
            .resolve_signature(None, None, SignatureKind::Reply)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn account_default_does_not_require_runtime_account_row() {
        let store = Store::in_memory().await.unwrap();
        let account_id = AccountId::new();
        let sig = signature("Config Only", "body");
        store.upsert_signature(&sig).await.unwrap();

        store
            .set_signature_default(
                &SignatureScope::Account(account_id.clone()),
                SignatureKind::New,
                &sig.id,
            )
            .await
            .unwrap();

        let resolved = store
            .resolve_signature(Some(&account_id), None, SignatureKind::New)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.name, "Config Only");
    }

    #[tokio::test]
    async fn deleting_signature_cascades_defaults() {
        let store = Store::in_memory().await.unwrap();
        let sig = signature("Work", "body");
        store.upsert_signature(&sig).await.unwrap();
        store
            .set_signature_default(&SignatureScope::Global, SignatureKind::New, &sig.id)
            .await
            .unwrap();

        store.delete_signature_by_name("Work").await.unwrap();

        assert!(store.list_signature_defaults().await.unwrap().is_empty());
    }
}
