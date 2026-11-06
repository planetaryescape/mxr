//! Browser credentials are children of a registration, never copies of its native proof.
use super::{client, denied, matches, valid_owner, verifier};
use mxr_core::{RegisteredClient, RemoteSecret};
use sqlx::Row;

const CODE_TTL: i64 = 120;
/// A paired browser stays signed in for thirty days. The parent registration
/// still caps it at ninety days, and revocation ends it immediately.
const SESSION_TTL: i64 = 30 * 24 * 60 * 60;

fn secret() -> Result<RemoteSecret, sqlx::Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| denied())?;
    Ok(RemoteSecret::new(base16ct::lower::encode_string(&bytes)))
}

impl crate::Store {
    pub async fn issue_browser_code(
        &self,
        parent_id: &str,
        mail: &RemoteSecret,
        now: i64,
    ) -> Result<(RemoteSecret, i64), sqlx::Error> {
        // Child sessions cannot delegate further authority.
        if parent_id.starts_with("browser:") {
            return Err(denied());
        }
        let proof = verifier(mail)?;
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row = sqlx::query("SELECT * FROM remote_clients WHERE client_id=? AND revoked_at IS NULL AND expires_at>?")
            .bind(parent_id).bind(now).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        if !matches(&row.get::<String, _>("mail_verifier"), &proof) {
            return Err(denied());
        }
        sqlx::query("DELETE FROM remote_browser_codes WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        // One outstanding launch per registration bounds storage and retires abandoned codes.
        sqlx::query("DELETE FROM remote_browser_codes WHERE parent_client_id=?")
            .bind(parent_id)
            .execute(&mut *tx)
            .await?;
        let code = secret()?;
        let expires = (now + CODE_TTL).min(row.get("expires_at"));
        sqlx::query("INSERT INTO remote_browser_codes(code_verifier,parent_client_id,expires_at) VALUES(?,?,?)")
            .bind(verifier(&code)?).bind(parent_id).bind(expires).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok((code, expires))
    }

    pub async fn claim_browser_code(
        &self,
        code: &RemoteSecret,
        now: i64,
    ) -> Result<(String, RemoteSecret, i64), sqlx::Error> {
        self.claim_browser_code_replacing(code, None, now).await
    }

    pub async fn claim_browser_code_replacing(
        &self,
        code: &RemoteSecret,
        previous: Option<&(String, RemoteSecret)>,
        now: i64,
    ) -> Result<(String, RemoteSecret, i64), sqlx::Error> {
        let proof = verifier(code)?;
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row = sqlx::query("SELECT c.parent_client_id,p.expires_at FROM remote_browser_codes c JOIN remote_clients p ON p.client_id=c.parent_client_id WHERE c.code_verifier=? AND c.expires_at>? AND p.revoked_at IS NULL AND p.expires_at>?")
            .bind(&proof).bind(now).bind(now).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        let parent: String = row.get("parent_client_id");
        let expires = (now + SESSION_TTL).min(row.get("expires_at"));
        sqlx::query("DELETE FROM remote_browser_codes WHERE code_verifier=?")
            .bind(proof)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM remote_browser_sessions WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        if let Some((id, mail)) = previous {
            if let (Some(id), Ok(proof)) = (id.strip_prefix("browser:"), verifier(mail)) {
                sqlx::query("DELETE FROM remote_browser_sessions WHERE session_id=? AND parent_client_id=? AND mail_verifier=?")
                    .bind(id).bind(&parent).bind(proof).execute(&mut *tx).await?;
            }
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM remote_browser_sessions WHERE parent_client_id=?",
        )
        .bind(&parent)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 16 {
            return Err(denied());
        }
        let id = uuid::Uuid::now_v7().to_string();
        let mail = secret()?;
        sqlx::query("INSERT INTO remote_browser_sessions(session_id,mail_verifier,parent_client_id,expires_at) VALUES(?,?,?,?)")
            .bind(&id).bind(verifier(&mail)?).bind(parent).bind(expires).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok((format!("browser:{id}"), mail, expires))
    }

    pub(super) async fn authenticate_browser_session(
        &self,
        session_id: &str,
        mail: &RemoteSecret,
        now: i64,
    ) -> Result<RegisteredClient, sqlx::Error> {
        let proof = verifier(mail)?;
        let mut tx = self.writer().begin().await?;
        valid_owner(&mut tx).await?;
        let row = sqlx::query("SELECT p.*,s.mail_verifier AS session_proof FROM remote_browser_sessions s JOIN remote_clients p ON p.client_id=s.parent_client_id WHERE s.session_id=? AND s.expires_at>? AND p.revoked_at IS NULL AND p.expires_at>?")
            .bind(session_id).bind(now).bind(now).fetch_optional(&mut *tx).await?.ok_or_else(denied)?;
        if !matches(&row.get::<String, _>("session_proof"), &proof) {
            return Err(denied());
        }
        let result = client(&row);
        tx.commit().await?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::RemoteCapability;
    use sha2::{Digest, Sha256};

    async fn parent(store: &crate::Store, marker: char) -> (String, RemoteSecret) {
        let mail = RemoteSecret::new(marker.to_string().repeat(64));
        let cleanup = RemoteSecret::new(if marker == 'a' { "b" } else { "d" }.repeat(64));
        let hash = |s: &RemoteSecret| {
            RemoteSecret::new(base16ct::lower::encode_string(&Sha256::digest(
                s.expose().as_bytes(),
            )))
        };
        let p = store
            .start_remote_pairing(
                "browser fixture",
                &[RemoteCapability::MailRead, RemoteCapability::MailTriage],
                &hash(&mail),
                &hash(&cleanup),
                100,
            )
            .await
            .unwrap();
        store
            .approve_remote_pairing(&p.request_id, false, 101)
            .await
            .unwrap();
        let client = store
            .claim_remote_pairing(&p.request_id, &mail, &cleanup, 102)
            .await
            .unwrap();
        (client.client_id, mail)
    }

    #[tokio::test]
    async fn browser_codes_are_single_use_bounded_children_and_revocation_checks_parent() {
        let store = crate::Store::in_memory().await.unwrap();
        let (first, mail) = parent(&store, 'a').await;
        let (other, other_mail) = parent(&store, 'c').await;
        let (code, _) = store.issue_browser_code(&first, &mail, 103).await.unwrap();
        let (id, secret, expires) = store.claim_browser_code(&code, 104).await.unwrap();
        assert!(store.claim_browser_code(&code, 104).await.is_err());
        let child = store
            .authenticate_remote_client(&id, &secret, 105)
            .await
            .unwrap();
        assert_eq!(child.client_id, first);
        assert_eq!(
            child.capabilities,
            [RemoteCapability::MailRead, RemoteCapability::MailTriage]
        );
        assert!(store.issue_browser_code(&id, &secret, 105).await.is_err());
        assert!(store
            .authenticate_remote_client(&id, &mail, 105)
            .await
            .is_err());
        assert!(store
            .authenticate_remote_client(&id, &secret, expires)
            .await
            .is_err());
        let (other_code, _) = store
            .issue_browser_code(&other, &other_mail, 105)
            .await
            .unwrap();
        store
            .revoke_remote_client(&first, false, 106)
            .await
            .unwrap();
        assert!(store
            .authenticate_remote_client(&id, &secret, 107)
            .await
            .is_err());
        assert!(store.claim_browser_code(&other_code, 107).await.is_ok());
    }

    #[tokio::test]
    async fn replacing_or_expiring_a_launch_code_cannot_issue_a_session() {
        let store = crate::Store::in_memory().await.unwrap();
        let (id, secret) = parent(&store, 'a').await;
        let (old, _) = store.issue_browser_code(&id, &secret, 103).await.unwrap();
        let (current, expiry) = store.issue_browser_code(&id, &secret, 104).await.unwrap();
        assert!(store.claim_browser_code(&old, 105).await.is_err());
        assert!(store.claim_browser_code(&current, expiry).await.is_err());
        let (code, _) = store.issue_browser_code(&id, &secret, 105).await.unwrap();
        store.revoke_remote_client(&id, false, 106).await.unwrap();
        assert!(store.claim_browser_code(&code, 107).await.is_err());
    }
}
