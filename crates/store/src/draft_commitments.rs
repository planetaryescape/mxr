//! Slice 2.1 of docs/ai-email/02-follow-up-work.md.
//!
//! Per-draft commitment candidates extracted before send.
//! Promoted to `contact_commitments` after the draft sends successfully.

use crate::contact_commitments::CommitmentDirection;
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, DraftId};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftCommitmentCandidate {
    pub id: String,
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub email: String,
    pub direction: CommitmentDirection,
    pub who_owes: String,
    pub what: String,
    pub by_when: Option<DateTime<Utc>>,
    pub extracted_at: DateTime<Utc>,
}

impl super::Store {
    /// Insert a draft-scoped commitment candidate. Idempotent on
    /// `(draft_id, email, direction, what)` — re-extraction with the
    /// same content is a no-op.
    pub async fn upsert_draft_commitment_candidate(
        &self,
        candidate: &DraftCommitmentCandidate,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO draft_commitment_candidates
               (id, draft_id, account_id, email, direction, who_owes, what, by_when, extracted_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(draft_id, email, direction, what) DO NOTHING"#,
        )
        .bind(&candidate.id)
        .bind(candidate.draft_id.as_str())
        .bind(candidate.account_id.as_str())
        .bind(&candidate.email)
        .bind(candidate.direction.as_str())
        .bind(&candidate.who_owes)
        .bind(&candidate.what)
        .bind(candidate.by_when.map(|v| v.timestamp()))
        .bind(candidate.extracted_at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_draft_commitment_candidates(
        &self,
        draft_id: &DraftId,
    ) -> Result<Vec<DraftCommitmentCandidate>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, draft_id, account_id, email, direction, who_owes, what,
                      by_when, extracted_at
               FROM draft_commitment_candidates
               WHERE draft_id = ?
               ORDER BY extracted_at ASC, id ASC"#,
        )
        .bind(draft_id.as_str())
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                let dir = match row.get::<String, _>("direction").as_str() {
                    "theirs" => CommitmentDirection::Theirs,
                    _ => CommitmentDirection::Yours,
                };
                Ok(DraftCommitmentCandidate {
                    id: row.try_get("id")?,
                    draft_id: crate::decode_id(row.try_get::<&str, _>("draft_id")?)?,
                    account_id: crate::decode_id(row.try_get::<&str, _>("account_id")?)?,
                    email: row.try_get("email")?,
                    direction: dir,
                    who_owes: row.try_get("who_owes")?,
                    what: row.try_get("what")?,
                    by_when: crate::decode_optional_timestamp(row.try_get("by_when")?)?,
                    extracted_at: crate::decode_timestamp(row.try_get("extracted_at")?)?,
                })
            })
            .collect()
    }

    pub async fn delete_draft_commitment_candidates(
        &self,
        draft_id: &DraftId,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("DELETE FROM draft_commitment_candidates WHERE draft_id = ?")
            .bind(draft_id.as_str())
            .execute(self.writer())
            .await?;
        Ok(res.rows_affected())
    }
}

pub fn new_candidate_id() -> String {
    Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use mxr_core::id::{AccountId, DraftId};

    async fn fixture() -> (Store, AccountId, DraftId) {
        let store = Store::in_memory().await.unwrap();
        let account = mxr_core::Account {
            id: AccountId::new(),
            name: "T".into(),
            email: "me@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.unwrap();
        (store, account.id, DraftId::new())
    }

    fn candidate(
        account_id: &AccountId,
        draft_id: &DraftId,
        what: &str,
    ) -> DraftCommitmentCandidate {
        DraftCommitmentCandidate {
            id: new_candidate_id(),
            draft_id: draft_id.clone(),
            account_id: account_id.clone(),
            email: "alice@example.com".into(),
            direction: CommitmentDirection::Yours,
            who_owes: "me@example.com".into(),
            what: what.into(),
            by_when: None,
            extracted_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_list_returns_inserted_candidate() {
        let (store, account_id, draft_id) = fixture().await;
        let c = candidate(&account_id, &draft_id, "send the deck Friday");
        store.upsert_draft_commitment_candidate(&c).await.unwrap();
        let rows = store
            .list_draft_commitment_candidates(&draft_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].what, "send the deck Friday");
    }

    #[tokio::test]
    async fn upsert_is_idempotent_on_unique_key() {
        let (store, account_id, draft_id) = fixture().await;
        let c = candidate(&account_id, &draft_id, "send deck");
        store.upsert_draft_commitment_candidate(&c).await.unwrap();
        // Re-insert with a fresh id but same key fields.
        let mut c2 = c.clone();
        c2.id = new_candidate_id();
        store.upsert_draft_commitment_candidate(&c2).await.unwrap();
        let rows = store
            .list_draft_commitment_candidates(&draft_id)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "duplicate key must not duplicate row");
    }

    #[tokio::test]
    async fn delete_clears_candidates_for_draft_only() {
        let (store, account_id, draft_id) = fixture().await;
        let other_draft = DraftId::new();
        store
            .upsert_draft_commitment_candidate(&candidate(&account_id, &draft_id, "a"))
            .await
            .unwrap();
        store
            .upsert_draft_commitment_candidate(&candidate(&account_id, &other_draft, "b"))
            .await
            .unwrap();
        let removed = store
            .delete_draft_commitment_candidates(&draft_id)
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert!(store
            .list_draft_commitment_candidates(&draft_id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .list_draft_commitment_candidates(&other_draft)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
