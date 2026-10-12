use mxr_core::{DraftId, RecoveredDraftData, RecoveryStatusData, RetainedIntentData};
use sqlx::Row;

impl crate::Store {
    pub async fn restore_pending(&self) -> Result<bool, sqlx::Error> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT pending FROM restore_recovery WHERE singleton = 1",
        )
        .fetch_optional(self.reader())
        .await?
        .unwrap_or(false))
    }

    pub async fn recovery_status(&self) -> Result<RecoveryStatusData, sqlx::Error> {
        let (workspace_id, recovery_generation) = self.archive_workspace().await?;
        let stopped = sqlx::query_scalar::<_, bool>(
            "SELECT source_owner_stopped FROM restore_recovery WHERE singleton = 1",
        )
        .fetch_optional(self.reader())
        .await?
        .unwrap_or(false);
        let rows = sqlx::query("SELECT h.draft_id,h.account_id,COALESCE(d.revision,h.revision) AS revision,COALESCE(d.subject,h.subject) AS subject,h.prepared_draft_id FROM restored_draft_holds h LEFT JOIN drafts d ON d.id = h.draft_id ORDER BY h.draft_id").fetch_all(self.reader()).await?;
        let held_drafts = rows
            .into_iter()
            .map(|row| {
                Ok(RecoveredDraftData {
                    draft_id: crate::decode_id(&row.get::<String, _>("draft_id"))?,
                    account_id: crate::decode_id(&row.get::<String, _>("account_id"))?,
                    revision: row.get("revision"),
                    subject: row.get("subject"),
                    prepared_draft_id: row
                        .get::<Option<String>, _>("prepared_draft_id")
                        .map(|id| crate::decode_id(&id))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let retained_intentions = sqlx::query("SELECT original_id,kind,account_id,original_state,outcome_reference,payload_json FROM restored_intentions ORDER BY kind,original_id").fetch_all(self.reader()).await?.into_iter().map(|row| Ok(RetainedIntentData {
            archive_evidence: {let payload:serde_json::Value=crate::decode_json(&row.get::<String,_>("payload_json"))?;if payload["kind"]=="recoverable_archive" {payload.get("data_json").and_then(|v|v.as_str()).map(crate::decode_json).transpose()?}else{None}},
            original_id: row.get("original_id"), kind: row.get("kind"),
            account_id: row.get::<Option<String>,_>("account_id").map(|id| crate::decode_id(&id)).transpose()?,
            original_state: row.get("original_state"), outcome_reference: row.get("outcome_reference"),
        })).collect::<Result<Vec<_>,sqlx::Error>>()?;
        Ok(RecoveryStatusData {
            workspace_id,
            recovery_generation,
            restore_pending: self.restore_pending().await?,
            source_owner_stopped: stopped,
            retired_intentions: retained_intentions.len() as u64,
            retained_intentions,
            held_drafts,
        })
    }

    pub async fn is_restored_draft(&self, id: &DraftId) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM restored_draft_holds WHERE draft_id = ?)")
            .bind(id.as_str())
            .fetch_one(self.reader())
            .await
    }

    pub async fn activate_recovery(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE restore_recovery SET pending = 0, source_owner_stopped = 1 WHERE singleton = 1",
        )
        .execute(self.writer())
        .await?;
        Ok(())
    }
}
