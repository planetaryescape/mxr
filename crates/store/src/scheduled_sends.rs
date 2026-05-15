//! Send-later: drafts with a future `send_at`.
//!
//! Lives entirely on the existing `drafts` table — no separate row.
//! "Scheduled" is the orthogonal combination `status = 'draft' AND
//! send_at IS NOT NULL`. The flusher loop scans by partial index on
//! that combination.

use crate::{decode_id, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::DraftId;

impl super::Store {
    /// Schedule a draft to be sent at `send_at`. Idempotent — re-calling
    /// with a different `send_at` updates the schedule. The draft's
    /// `status` is left untouched (must be 'draft' for the flusher to
    /// pick it up).
    pub async fn schedule_send(
        &self,
        draft_id: &DraftId,
        send_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let id = draft_id.as_str();
        let send_at_ts = send_at.timestamp();
        sqlx::query!(
            r#"UPDATE drafts SET send_at = ? WHERE id = ?"#,
            send_at_ts,
            id,
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Clear the schedule on a draft, leaving the draft itself intact.
    pub async fn cancel_scheduled_send(&self, draft_id: &DraftId) -> Result<(), sqlx::Error> {
        let id = draft_id.as_str();
        sqlx::query!(r#"UPDATE drafts SET send_at = NULL WHERE id = ?"#, id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Read the scheduled send time for a draft, if any.
    pub async fn get_scheduled_send(
        &self,
        draft_id: &DraftId,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let id = draft_id.as_str();
        let row = sqlx::query!(r#"SELECT send_at FROM drafts WHERE id = ?"#, id,)
            .fetch_optional(self.reader())
            .await?;
        match row.and_then(|r| r.send_at) {
            None => Ok(None),
            Some(ts) => Ok(Some(decode_timestamp(ts)?)),
        }
    }

    /// Drafts due to fire by `now`: scheduled (`send_at` non-null,
    /// `status = 'draft'`) with `send_at <= now`. Ordered by `send_at`
    /// so oldest-due fires first.
    pub async fn get_due_scheduled_drafts(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<DraftId>, sqlx::Error> {
        let now_ts = now.timestamp();
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT id as "id!"
               FROM drafts
               WHERE status = 'draft'
                 AND send_at IS NOT NULL
                 AND send_at <= ?
               ORDER BY send_at ASC"#,
            now_ts,
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("scheduled_sends.get_due", started_at, rows.len());
        rows.into_iter().map(|r| decode_id(&r.id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::*;
    use super::super::Store;
    use chrono::{Duration, TimeZone, Utc};
    use mxr_core::id::DraftId;
    use mxr_core::types::{DraftStatus, *};

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 14, 0, 0).unwrap()
    }

    async fn seed_draft(store: &Store) -> DraftId {
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let draft = Draft {
            id: DraftId::new(),
            account_id: account.id,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "you@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test".into(),
            body_markdown: "Body".into(),
            attachments: vec![],
            created_at: anchor(),
            updated_at: anchor(),
        };
        store.insert_draft(&draft).await.unwrap();
        draft.id
    }

    #[tokio::test]
    async fn schedule_send_persists_and_round_trips() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        let send_at = anchor() + Duration::hours(1);

        store.schedule_send(&id, send_at).await.unwrap();

        assert_eq!(store.get_scheduled_send(&id).await.unwrap(), Some(send_at));
    }

    #[tokio::test]
    async fn scheduled_send_survives_store_reopen() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("mxr.db");
        let send_at = anchor() + Duration::hours(1);

        let id = {
            let store = Store::new(&db_path).await.unwrap();
            let id = seed_draft(&store).await;
            store.schedule_send(&id, send_at).await.unwrap();
            id
        };

        let reopened = Store::new(&db_path).await.unwrap();
        assert_eq!(
            reopened.get_scheduled_send(&id).await.unwrap(),
            Some(send_at),
            "scheduled send time must survive daemon/store restart"
        );
    }

    #[tokio::test]
    async fn cancel_scheduled_send_clears_send_at() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        store
            .schedule_send(&id, anchor() + Duration::hours(1))
            .await
            .unwrap();
        store.cancel_scheduled_send(&id).await.unwrap();

        assert_eq!(store.get_scheduled_send(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn get_due_excludes_future_scheduled_drafts() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        store
            .schedule_send(&id, anchor() + Duration::hours(1))
            .await
            .unwrap();

        let due = store.get_due_scheduled_drafts(anchor()).await.unwrap();
        assert!(due.is_empty());
    }

    #[tokio::test]
    async fn get_due_includes_past_scheduled_drafts() {
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        store
            .schedule_send(&id, anchor() - Duration::minutes(5))
            .await
            .unwrap();

        let due = store.get_due_scheduled_drafts(anchor()).await.unwrap();
        assert_eq!(due, vec![id]);
    }

    #[tokio::test]
    async fn get_due_excludes_already_sending_drafts() {
        // Once a flusher CAS-promotes a draft to `sending`, the row
        // must drop out of the due-list so it can't be picked up
        // twice (concurrent flushers, or a restart mid-send).
        let store = Store::in_memory().await.unwrap();
        let id = seed_draft(&store).await;
        store
            .schedule_send(&id, anchor() - Duration::hours(1))
            .await
            .unwrap();
        let _ = store
            .cas_draft_status(&id, DraftStatus::Draft, DraftStatus::Sending)
            .await
            .unwrap();

        let due = store.get_due_scheduled_drafts(anchor()).await.unwrap();
        assert!(
            due.is_empty(),
            "drafts in 'sending' status are not re-flushed"
        );
    }

    #[tokio::test]
    async fn get_due_orders_by_send_at_ascending() {
        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let make = |i: u32| {
            let draft = Draft {
                id: DraftId::new(),
                account_id: account.id.clone(),
                reply_headers: None,
                intent: DraftIntent::New,
                to: vec![Address {
                    name: None,
                    email: format!("a{i}@example.com"),
                }],
                cc: vec![],
                bcc: vec![],
                subject: format!("S{i}").into(),
                body_markdown: "Body".into(),
                attachments: vec![],
                created_at: anchor(),
                updated_at: anchor(),
            };
            draft
        };
        let a = make(0);
        let b = make(1);
        let c = make(2);
        for d in [&a, &b, &c] {
            store.insert_draft(d).await.unwrap();
        }
        store
            .schedule_send(&a.id, anchor() - Duration::hours(2))
            .await
            .unwrap();
        store
            .schedule_send(&b.id, anchor() - Duration::hours(5))
            .await
            .unwrap();
        store
            .schedule_send(&c.id, anchor() - Duration::hours(1))
            .await
            .unwrap();

        let due = store.get_due_scheduled_drafts(anchor()).await.unwrap();
        assert_eq!(
            due,
            vec![b.id.clone(), a.id.clone(), c.id.clone()],
            "oldest send_at first"
        );
    }
}
