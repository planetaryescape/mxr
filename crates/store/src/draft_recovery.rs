//! Crash-safe-drafts recovery primitives.
//!
//! Orphan detection identifies interrupted sends. Age and heartbeat absence
//! never prove provider non-acceptance, so recovery preserves Sending.

use chrono::{DateTime, Utc};
use mxr_core::id::DraftId;

impl super::Store {
    /// Touch the heartbeat on a draft. Called periodically by the live
    /// send pipeline so daemon-startup recovery can tell live in-flight
    /// sends apart from orphaned ones.
    pub async fn touch_draft_heartbeat(
        &self,
        draft_id: &DraftId,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let id = draft_id.as_str();
        let ts = now.timestamp();
        sqlx::query!(
            r#"UPDATE drafts SET last_heartbeat_at = ? WHERE id = ?"#,
            ts,
            id,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Read a draft's `last_heartbeat_at`. Returns `Ok(None)` if the draft
    /// is missing or has never been heartbeated.
    pub async fn get_draft_heartbeat(
        &self,
        draft_id: &DraftId,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let id = draft_id.as_str();
        let row = sqlx::query!(r#"SELECT last_heartbeat_at FROM drafts WHERE id = ?"#, id,)
            .fetch_optional(self.reader())
            .await?;
        Ok(row
            .and_then(|r| r.last_heartbeat_at)
            .and_then(|ts| DateTime::<Utc>::from_timestamp(ts, 0)))
    }

    /// Find drafts that look orphaned: `status = 'sending'` AND the
    /// most-recent activity (heartbeat if present, else
    /// `status_updated_at`) is older than `cutoff`.
    pub async fn list_orphaned_sending_drafts(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<DraftId>, sqlx::Error> {
        let cutoff_ts = cutoff.timestamp();
        let rows = sqlx::query!(
            r#"SELECT id as "id!"
               FROM drafts
               WHERE status = 'sending'
                 AND COALESCE(last_heartbeat_at, status_updated_at, updated_at) < ?
               ORDER BY status_updated_at ASC"#,
            cutoff_ts,
        )
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|r| crate::decode_id(&r.id)).collect()
    }

    /// Compatibility entry point: uncertain legacy and current sends stay blocked.
    pub async fn reset_orphaned_draft(&self, draft_id: &DraftId) -> Result<bool, sqlx::Error> {
        let _ = draft_id;
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use chrono::{TimeZone, Utc};
    use mxr_core::id::DraftId;
    use mxr_core::types::{Address, Draft, DraftContent, DraftIntent, DraftStatus};

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    async fn seed_draft(store: &Store) -> DraftId {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let draft = Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: account.id,
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "you@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test".into(),
            content: DraftContent::markdown("Body"),
            managed_attachments: None,
            attachments: vec![],
            inline_assets: vec![],
            inline_calendar_reply: None,
            created_at: anchor(),
            updated_at: anchor(),
        };
        store.insert_draft(&draft).await.unwrap();
        draft.id
    }

    #[tokio::test]
    async fn list_orphaned_includes_stale_sending_drafts() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        let _ = store
            .cas_draft_status(&id, DraftStatus::Draft, DraftStatus::Sending)
            .await
            .unwrap();
        let orphans = store
            .list_orphaned_sending_drafts(Utc.with_ymd_and_hms(2099, 1, 1, 0, 0, 0).unwrap())
            .await
            .unwrap();
        assert_eq!(orphans, vec![id]);
    }

    #[tokio::test]
    async fn reset_orphaned_preserves_unknown_acceptance() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        let _ = store
            .cas_draft_status(&id, DraftStatus::Draft, DraftStatus::Sending)
            .await
            .unwrap();

        let reset = store.reset_orphaned_draft(&id).await.unwrap();
        assert!(!reset, "uncertain acceptance cannot be reset");

        assert_eq!(
            store.get_draft_status(&id).await.unwrap(),
            Some(DraftStatus::Sending)
        );
    }
}
