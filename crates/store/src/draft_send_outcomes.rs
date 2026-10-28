use mxr_core::{
    AccountId, DraftId, DraftSendOutcome, MessageId, SendIdentity, SendOutcomeState, SendReceipt,
};
use sqlx::Row;

impl super::Store {
    pub async fn get_send_outcome(
        &self,
        operation_id: &str,
    ) -> Result<Option<(DraftSendOutcome, String)>, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM draft_send_outcomes WHERE operation_id = ?")
            .bind(operation_id)
            .fetch_optional(self.reader())
            .await?;
        let recovery = self.get_send_recovery_status(operation_id).await?;
        row.map(|row| {
            let state = match row.get::<String, _>("state").as_str() {
                "accepted" => SendOutcomeState::Accepted,
                "not_accepted" => SendOutcomeState::NotAccepted,
                _ if recovery
                    .as_ref()
                    .is_some_and(|(status, _)| status == "waiting") =>
                {
                    SendOutcomeState::Waiting
                }
                _ => SendOutcomeState::Unknown,
            };
            Ok((
                DraftSendOutcome {
                    identity: SendIdentity {
                        operation_id: row.get("operation_id"),
                        expected_revision: row.get("reviewed_revision"),
                    },
                    draft_id: crate::decode_id(&row.get::<String, _>("draft_id"))?,
                    account_id: crate::decode_id(&row.get::<String, _>("account_id"))?,
                    state,
                    local_message_id: row
                        .get::<Option<String>, _>("local_message_id")
                        .map(|s| crate::decode_id(&s))
                        .transpose()?,
                    provider_message_id: row.get("provider_message_id"),
                    rfc2822_message_id: row.get("rfc2822_message_id"),
                    recovery: recovery.map(|(_, recovery)| recovery),
                },
                row.get("command_hash"),
            ))
        })
        .transpose()
    }

    pub async fn get_draft_send_outcome(
        &self,
        draft_id: &DraftId,
    ) -> Result<Option<DraftSendOutcome>, sqlx::Error> {
        let operation_id: Option<String> = sqlx::query_scalar("SELECT operation_id FROM draft_send_outcomes WHERE draft_id = ? ORDER BY updated_at DESC, rowid DESC LIMIT 1").bind(draft_id.as_str()).fetch_optional(self.reader()).await?;
        match operation_id {
            Some(id) => Ok(self
                .get_send_outcome(&id)
                .await?
                .map(|(outcome, _)| outcome)),
            None => Ok(None),
        }
    }

    /// Claim and uncertainty become durable together before any provider call.
    pub async fn claim_draft_send(
        &self,
        identity: &SendIdentity,
        draft_id: &DraftId,
        account_id: &AccountId,
        stored_revision: i64,
        command_hash: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE drafts SET status = 'sending', status_updated_at = unixepoch(), last_heartbeat_at = unixepoch() WHERE id = ? AND revision = ? AND status = 'draft' AND NOT EXISTS (SELECT 1 FROM restored_draft_holds WHERE restored_draft_holds.draft_id = drafts.id) AND NOT EXISTS (SELECT 1 FROM draft_send_outcomes WHERE draft_id = ? AND state IN ('unknown','accepted'))")
            .bind(draft_id.as_str()).bind(stored_revision).bind(draft_id.as_str()).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO draft_send_outcomes (operation_id,draft_id,account_id,reviewed_revision,command_hash,state,rfc2822_message_id,updated_at) VALUES (?,?,?,?,?,'unknown',(SELECT message_id_header FROM drafts WHERE id=?),unixepoch())")
            .bind(&identity.operation_id).bind(draft_id.as_str()).bind(account_id.as_str()).bind(identity.expected_revision).bind(command_hash).bind(draft_id.as_str()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn accept_draft_send(
        &self,
        identity: &SendIdentity,
        receipt: &SendReceipt,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        sqlx::query("UPDATE draft_send_outcomes SET state = 'accepted', provider_message_id = ?, rfc2822_message_id = ?, updated_at = unixepoch() WHERE operation_id = ? AND state = 'unknown'")
            .bind(&receipt.provider_message_id).bind(&receipt.rfc2822_message_id).bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE drafts SET status = 'sent', status_updated_at = unixepoch() WHERE id = (SELECT draft_id FROM draft_send_outcomes WHERE operation_id = ? AND state = 'accepted')")
            .bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE approved_sends SET status='accepted',phase='\"accepted\"',retry_at=NULL,reason=NULL,accepted_at=COALESCE(accepted_at,?),updated_at=unixepoch() WHERE operation_id=?")
            .bind(receipt.sent_at.timestamp()).bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE scheduled_send_attempts SET outcome='sent' WHERE outcome='waiting' AND draft_id=(SELECT draft_id FROM draft_send_outcomes WHERE operation_id=? AND state='accepted')").bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE approved_send_attempts SET phase='\"accepted\"',outcome='accepted',recorded_at=unixepoch() WHERE operation_id=? AND attempt=(SELECT attempts FROM approved_sends WHERE operation_id=?)")
            .bind(&identity.operation_id).bind(&identity.operation_id).execute(&mut *tx).await?;
        tx.commit().await
    }

    pub async fn reject_draft_send(&self, identity: &SendIdentity) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        sqlx::query("UPDATE draft_send_outcomes SET state = 'not_accepted', updated_at = unixepoch() WHERE operation_id = ? AND state = 'unknown'").bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE drafts SET status = 'draft', status_updated_at = unixepoch() WHERE id = (SELECT draft_id FROM draft_send_outcomes WHERE operation_id = ? AND state = 'not_accepted')").bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE approved_sends SET status='failed',retry_at=NULL,updated_at=unixepoch() WHERE operation_id=? AND status!='accepted'").bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE scheduled_send_attempts SET outcome='failed' WHERE outcome='waiting' AND draft_id=(SELECT draft_id FROM draft_send_outcomes WHERE operation_id=? AND state='not_accepted')").bind(&identity.operation_id).execute(&mut *tx).await?;
        tx.commit().await
    }

    pub async fn link_send_local_message(
        &self,
        identity: &SendIdentity,
        local_id: &MessageId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE draft_send_outcomes SET local_message_id = ? WHERE operation_id = ? AND state = 'accepted'")
            .bind(local_id.as_str()).bind(&identity.operation_id).execute(self.writer()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::{Draft, DraftContent, DraftIntent, DraftStatus};

    #[tokio::test]
    async fn send_outcomes_restart_retains_claim_and_completed_receipt_without_draft() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("state.db");
        let account = crate::test_fixtures::test_account();
        let now = chrono::Utc::now();
        let draft = Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: account.id.clone(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "fixture".into(),
            content: DraftContent::markdown("synthetic"),
            managed_attachments: None,
            attachments: vec![],
            inline_assets: vec![],
            inline_calendar_reply: None,
            created_at: now,
            updated_at: now,
        };
        let identity = SendIdentity::for_draft(&draft);
        {
            let store = super::super::Store::new(&db).await.unwrap();
            store.insert_account(&account).await.unwrap();
            store.insert_draft(&draft).await.unwrap();
            assert!(store
                .claim_draft_send(&identity, &draft.id, &account.id, 1, "command")
                .await
                .unwrap());
        }
        {
            let store = super::super::Store::new(&db).await.unwrap();
            assert_eq!(
                store
                    .get_send_outcome(&identity.operation_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .0
                    .state,
                SendOutcomeState::Unknown
            );
            assert!(!store.reset_orphaned_draft(&draft.id).await.unwrap());
            assert_eq!(
                store.get_draft_status(&draft.id).await.unwrap(),
                Some(DraftStatus::Sending)
            );
            let other = SendIdentity {
                operation_id: "another-identity".into(),
                expected_revision: 1,
            };
            assert!(!store
                .claim_draft_send(&other, &draft.id, &account.id, 1, "command")
                .await
                .unwrap());
            store
                .accept_draft_send(
                    &identity,
                    &SendReceipt {
                        provider_message_id: Some("provider-accepted".into()),
                        rfc2822_message_id: "<accepted@fake.local>".into(),
                        sent_at: now,
                    },
                )
                .await
                .unwrap();
            store.delete_draft(&draft.id).await.unwrap();
        }
        let store = super::super::Store::new(&db).await.unwrap();
        assert!(store.get_draft(&draft.id).await.unwrap().is_none());
        let accepted = store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(accepted.state, SendOutcomeState::Accepted);
        assert!(accepted.local_message_id.is_none());
        let mut legacy = draft.clone();
        legacy.id = DraftId::new();
        store.insert_draft(&legacy).await.unwrap();
        store
            .update_draft_status(&legacy.id, DraftStatus::Sending)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/073_draft_send_outcomes.sql"))
            .execute(store.writer())
            .await
            .unwrap();
        let recovered = store
            .get_send_outcome(&format!("legacy-sending:{}", legacy.id))
            .await
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(recovered.state, SendOutcomeState::Unknown);
        assert_eq!(recovered.account_id, legacy.account_id);
        store.delete_draft(&legacy.id).await.unwrap();
        assert!(store
            .get_send_outcome(&recovered.identity.operation_id)
            .await
            .unwrap()
            .is_some());
    }
}
