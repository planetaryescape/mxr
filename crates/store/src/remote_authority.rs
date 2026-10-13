//! Transactional registration authority. A single writer transaction orders claim/revoke.
use mxr_core::{HostAuthority, PendingPairing, RegisteredClient, RemoteCapability, RemoteSecret};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, Transaction};

pub const PAIRING_TTL_SECONDS: i64 = 600;
pub const MAX_PENDING_PAIRINGS: i64 = 16;
pub const MAX_CLAIM_FAILURES: i64 = 5;
pub const MAIL_CREDENTIAL_TTL_SECONDS: i64 = 90 * 24 * 60 * 60;

fn denied() -> sqlx::Error {
    sqlx::Error::Protocol("Remote authority denied; inspect pairing or registration state".into())
}
fn valid_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn verifier(secret: &RemoteSecret) -> Result<String, sqlx::Error> {
    if !valid_hex(secret.expose()) {
        return Err(denied());
    }
    Ok(base16ct::lower::encode_string(&Sha256::digest(
        secret.expose().as_bytes(),
    )))
}
fn matches(expected: &str, presented: &str) -> bool {
    constant_time_eq::constant_time_eq(expected.as_bytes(), presented.as_bytes())
}
fn pairing(row: &sqlx::sqlite::SqliteRow) -> PendingPairing {
    PendingPairing {
        capabilities: vec![RemoteCapability::MailRead],
        request_id: row.get("request_id"),
        name: row.get("name"),
        challenge: row.get("challenge"),
        expires_at: row.get("expires_at"),
        approved: row.get("approved"),
        failed_attempts: row.get::<i64, _>("failed_attempts") as u32,
    }
}
fn client(row: &sqlx::sqlite::SqliteRow) -> RegisteredClient {
    RegisteredClient {
        capabilities: vec![RemoteCapability::MailRead],
        client_id: row.get("client_id"),
        name: row.get("name"),
        created_at: row.get("created_at"),
        expires_at: row.get("expires_at"),
        revoked_at: row.get("revoked_at"),
    }
}
async fn valid_owner(tx: &mut Transaction<'_, Sqlite>) -> Result<(), sqlx::Error> {
    let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM remote_host h JOIN archive_workspace w ON w.singleton=h.singleton AND w.workspace_id=h.workspace_id AND w.recovery_generation=h.recovery_generation)").fetch_one(&mut **tx).await?;
    let paused: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM restore_recovery WHERE pending=1)")
            .fetch_one(&mut **tx)
            .await?;
    if !valid || paused {
        return Err(denied());
    }
    Ok(())
}

async fn revoke_registration(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE remote_clients SET revoked_at=COALESCE(revoked_at,?),mail_verifier='',name='' WHERE client_id=?")
        .bind(now).bind(id).execute(&mut **tx).await?;
    Ok(())
}

impl crate::Store {
    pub async fn remote_authority_status(&self) -> Result<HostAuthority, sqlx::Error> {
        let owner = self.read_archive_workspace().await?;
        let host_id: Option<String> = sqlx::query_scalar("SELECT h.host_id FROM remote_host h JOIN archive_workspace w ON w.singleton=h.singleton AND w.workspace_id=h.workspace_id AND w.recovery_generation=h.recovery_generation").fetch_optional(self.reader()).await?;
        Ok(HostAuthority {
            host_id,
            workspace_id: owner.as_ref().map(|o| o.0.clone()),
            recovery_generation: owner.map(|o| o.1),
            hosting_available: false,
        })
    }
    pub async fn remote_pairings(&self, now: i64) -> Result<Vec<PendingPairing>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM remote_pairings WHERE expires_at>? AND failed_attempts<? ORDER BY request_id").bind(now).bind(MAX_CLAIM_FAILURES).fetch_all(self.reader()).await?;
        Ok(rows.iter().map(pairing).collect())
    }
    pub async fn remote_clients(&self) -> Result<Vec<RegisteredClient>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM remote_clients ORDER BY client_id")
            .fetch_all(self.reader())
            .await?;
        Ok(rows.iter().map(client).collect())
    }
    pub async fn start_remote_pairing(
        &self,
        name: &str,
        capabilities: &[RemoteCapability],
        mail: &RemoteSecret,
        cleanup: &RemoteSecret,
        now: i64,
    ) -> Result<PendingPairing, sqlx::Error> {
        if capabilities != [RemoteCapability::MailRead]
            || name.trim().is_empty()
            || name.len() > 80
            || name.chars().any(char::is_control)
            || !valid_hex(mail.expose())
            || !valid_hex(cleanup.expose())
            || matches(mail.expose(), cleanup.expose())
        {
            return Err(denied());
        }
        // Identity initialization is a mutation and only happens when a pairing starts.
        self.archive_workspace().await?;
        let mut tx = self.writer().begin().await?;
        let (workspace_id, recovery_generation): (String, String) = sqlx::query_as(
            "SELECT workspace_id,recovery_generation FROM archive_workspace WHERE singleton=1",
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM remote_pairings WHERE expires_at<=? OR failed_attempts>=?")
            .bind(now)
            .bind(MAX_CLAIM_FAILURES)
            .execute(&mut *tx)
            .await?;
        let paused: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM restore_recovery WHERE pending=1)")
                .fetch_one(&mut *tx)
                .await?;
        if paused {
            return Err(denied());
        }
        let stale:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM remote_host WHERE workspace_id<>? OR recovery_generation<>?)").bind(&workspace_id).bind(&recovery_generation).fetch_one(&mut *tx).await?;
        if stale {
            sqlx::raw_sql(
                "DELETE FROM remote_clients; DELETE FROM remote_pairings; DELETE FROM remote_pairing_retirements; DELETE FROM remote_host;",
            )
            .execute(&mut *tx)
            .await?;
        }
        if let Some(row) = sqlx::query(
            "SELECT * FROM remote_pairings WHERE mail_verifier=? AND cleanup_verifier=? AND name=?",
        )
        .bind(mail.expose())
        .bind(cleanup.expose())
        .bind(name.trim())
        .fetch_optional(&mut *tx)
        .await?
        {
            let existing = pairing(&row);
            tx.commit().await?;
            return Ok(existing);
        }
        let retired: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM remote_pairing_retirements WHERE cleanup_verifier=?)",
        )
        .bind(cleanup.expose())
        .fetch_one(&mut *tx)
        .await?;
        if retired {
            return Err(denied());
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM remote_pairings")
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_PENDING_PAIRINGS {
            return Err(sqlx::Error::Protocol(
                "Too many pending pairings; wait for expiry".into(),
            ));
        }
        sqlx::query("INSERT INTO remote_host VALUES(1,?,?,?) ON CONFLICT(singleton) DO NOTHING")
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(workspace_id)
            .bind(recovery_generation)
            .execute(&mut *tx)
            .await?;
        let request_id = uuid::Uuid::now_v7().to_string();
        let challenge = base16ct::lower::encode_string(&Sha256::digest(
            format!(
                "mxr-pairing-v1:{request_id}:{}:{}",
                mail.expose(),
                cleanup.expose()
            )
            .as_bytes(),
        ));
        sqlx::query("INSERT INTO remote_pairings(request_id,name,challenge,mail_verifier,cleanup_verifier,expires_at) VALUES(?,?,?,?,?,?)").bind(&request_id).bind(name.trim()).bind(&challenge).bind(mail.expose()).bind(cleanup.expose()).bind(now+PAIRING_TTL_SECONDS).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(PendingPairing {
            capabilities: vec![RemoteCapability::MailRead],
            request_id,
            name: name.trim().into(),
            challenge,
            expires_at: now + PAIRING_TTL_SECONDS,
            approved: false,
            failed_attempts: 0,
        })
    }
    pub async fn approve_remote_pairing(
        &self,
        id: &str,
        dry_run: bool,
        now: i64,
    ) -> Result<PendingPairing, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row=sqlx::query("SELECT * FROM remote_pairings WHERE request_id=? AND expires_at>? AND failed_attempts<?").bind(id).bind(now).bind(MAX_CLAIM_FAILURES).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        let mut result = pairing(&row);
        if !dry_run {
            sqlx::query("UPDATE remote_pairings SET approved=1 WHERE request_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            result.approved = true;
        }
        tx.commit().await?;
        Ok(result)
    }
    pub async fn claim_remote_pairing(
        &self,
        id: &str,
        mail: &RemoteSecret,
        cleanup: &RemoteSecret,
        now: i64,
    ) -> Result<RegisteredClient, sqlx::Error> {
        // Malformed proofs consume the retry budget; empty cannot match a live verifier.
        let mail = verifier(mail).unwrap_or_default();
        let cleanup = verifier(cleanup).unwrap_or_default();
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        // Lost ACK retries are bound to both proofs and cannot resurrect revoked/expired authority.
        if let Some(row) = sqlx::query("SELECT * FROM remote_clients WHERE pairing_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
        {
            let result = client(&row);
            if result.revoked_at.is_some()
                || result.expires_at <= now
                || !matches(&row.get::<String, _>("mail_verifier"), &mail)
                || !matches(&row.get::<String, _>("cleanup_verifier"), &cleanup)
            {
                return Err(denied());
            }
            tx.commit().await?;
            return Ok(result);
        }
        let row=sqlx::query("SELECT * FROM remote_pairings WHERE request_id=? AND approved=1 AND expires_at>? AND failed_attempts<?").bind(id).bind(now).bind(MAX_CLAIM_FAILURES).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        if !matches(&row.get::<String, _>("mail_verifier"), &mail)
            || !matches(&row.get::<String, _>("cleanup_verifier"), &cleanup)
        {
            sqlx::query(
                "UPDATE remote_pairings SET failed_attempts=failed_attempts+1 WHERE request_id=?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Err(denied());
        }
        let result = RegisteredClient {
            capabilities: vec![RemoteCapability::MailRead],
            client_id: uuid::Uuid::now_v7().to_string(),
            name: row.get("name"),
            created_at: now,
            expires_at: now + MAIL_CREDENTIAL_TTL_SECONDS,
            revoked_at: None,
        };
        sqlx::query("INSERT INTO remote_clients(client_id,pairing_id,name,mail_verifier,cleanup_verifier,created_at,expires_at,revoked_at) VALUES(?,?,?,?,?,?,?,NULL)").bind(&result.client_id).bind(id).bind(&result.name).bind(mail).bind(cleanup).bind(now).bind(result.expires_at).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM remote_pairings WHERE request_id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn authenticate_remote_client(
        &self,
        id: &str,
        mail: &RemoteSecret,
        now: i64,
    ) -> Result<RegisteredClient, sqlx::Error> {
        let mail = verifier(mail)?;
        // Use the writer transaction: completed revocations cannot race an old reader snapshot.
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row=sqlx::query("SELECT * FROM remote_clients WHERE client_id=? AND revoked_at IS NULL AND expires_at>?").bind(id).bind(now).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        if !matches(&row.get::<String, _>("mail_verifier"), &mail) {
            return Err(denied());
        }
        let result = client(&row);
        tx.commit().await?;
        Ok(result)
    }
    pub async fn revoke_remote_client(
        &self,
        id: &str,
        dry_run: bool,
        now: i64,
    ) -> Result<RegisteredClient, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let row = sqlx::query("SELECT * FROM remote_clients WHERE client_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(denied)?;
        let mut result = client(&row);
        if !dry_run {
            revoke_registration(&mut tx, id, now).await?;
            result.revoked_at = Some(result.revoked_at.unwrap_or(now));
        }
        tx.commit().await?;
        Ok(result)
    }
    /// Retire only the pending/claimed registration bound to this installation's cleanup proof.
    /// This is idempotent even when the start/claim acknowledgement was lost.
    pub async fn abandon_remote_pairing(
        &self,
        cleanup: &RemoteSecret,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        let cleanup = verifier(cleanup)?;
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let known: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM remote_pairings WHERE cleanup_verifier=? UNION ALL SELECT 1 FROM remote_clients WHERE cleanup_verifier=? UNION ALL SELECT 1 FROM remote_pairing_retirements WHERE cleanup_verifier=?)").bind(&cleanup).bind(&cleanup).bind(&cleanup).fetch_one(&mut *tx).await?;
        // Unknown cleanup must not acknowledge a start that has not reached the host yet.
        if !known {
            return Err(denied());
        }
        sqlx::query("INSERT INTO remote_pairing_retirements(cleanup_verifier) VALUES(?) ON CONFLICT DO NOTHING").bind(&cleanup).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM remote_pairings WHERE cleanup_verifier=?")
            .bind(&cleanup)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE remote_clients SET revoked_at=COALESCE(revoked_at,?),mail_verifier='',name='' WHERE cleanup_verifier=?").bind(now).bind(cleanup).execute(&mut *tx).await?;
        tx.commit().await
    }
    pub async fn cleanup_remote_client(
        &self,
        id: &str,
        cleanup: &RemoteSecret,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        let cleanup = verifier(cleanup)?;
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row = sqlx::query("SELECT cleanup_verifier FROM remote_clients WHERE client_id=?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(denied)?;
        if !matches(&row.get::<String, _>("cleanup_verifier"), &cleanup) {
            return Err(denied());
        }
        // Keep only cleanup evidence after revocation for retries; no mail work is cancelled.
        revoke_registration(&mut tx, id, now).await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
