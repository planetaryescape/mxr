//! Durable approval is local mail data, never activity/context JSON. The draft
//! hash, owner and frozen file hashes are checked again before every retry.
use chrono::{DateTime, Utc};
use mxr_core::{Address, Draft, SendIdentity, SendRecoveryStatus, SendSubmissionPhase};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovedSendFile {
    pub path: PathBuf,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendApproval {
    pub draft: Draft,
    pub from: Address,
    pub workspace_id: String,
    pub recovery_generation: String,
    pub stored_revision: i64,
    pub command_hash: String,
    pub files: Vec<ApprovedSendFile>,
    pub legacy_evidence_sha256: Option<String>,
}

impl super::Store {
    pub async fn approved_send_accepted_at(
        &self,
        operation_id: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let timestamp: Option<i64> =
            sqlx::query_scalar("SELECT accepted_at FROM approved_sends WHERE operation_id=?")
                .bind(operation_id)
                .fetch_optional(self.reader())
                .await?
                .flatten();
        timestamp.map(crate::decode_timestamp).transpose()
    }
    pub async fn send_outcome_recorded_at(
        &self,
        operation_id: &str,
    ) -> Result<DateTime<Utc>, sqlx::Error> {
        let timestamp: i64 =
            sqlx::query_scalar("SELECT updated_at FROM draft_send_outcomes WHERE operation_id=?")
                .bind(operation_id)
                .fetch_one(self.reader())
                .await?;
        crate::decode_timestamp(timestamp)
    }

    pub async fn recover_legacy_send(
        &self,
        identity: &SendIdentity,
        approval: &SendApproval,
        message_id: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE drafts SET status='draft',status_updated_at=unixepoch() WHERE id=? AND account_id=? AND revision=? AND status='sending' AND NOT EXISTS (SELECT 1 FROM restored_draft_holds WHERE draft_id=drafts.id) AND EXISTS (SELECT 1 FROM archive_workspace WHERE workspace_id=? AND recovery_generation=?) AND EXISTS (SELECT 1 FROM draft_send_outcomes WHERE operation_id=? AND draft_id=drafts.id AND reviewed_revision=? AND command_hash=? AND state='unknown') AND NOT EXISTS (SELECT 1 FROM approved_sends WHERE operation_id=?)")
            .bind(approval.draft.id.as_str()).bind(approval.draft.account_id.as_str()).bind(approval.stored_revision).bind(&approval.workspace_id).bind(&approval.recovery_generation).bind(&identity.operation_id).bind(identity.expected_revision).bind(&approval.command_hash).bind(&identity.operation_id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        let changed = sqlx::query("UPDATE draft_send_outcomes SET rfc2822_message_id=? WHERE operation_id=? AND (rfc2822_message_id IS NULL OR rfc2822_message_id=?)").bind(message_id).bind(&identity.operation_id).bind(message_id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO approved_sends (operation_id,approval_json,status,phase,attempts,retry_at,reason,created_at,updated_at) VALUES (?,?,'waiting','\"cooldown\"',1,?,'legacy_cooldown_proven',unixepoch(),unixepoch())")
            .bind(&identity.operation_id).bind(crate::encode_json(approval)?).bind(retry_at.timestamp()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO approved_send_attempts (operation_id,attempt,phase,outcome,recorded_at) SELECT operation_id,1,'\"cooldown\"','legacy_not_submitted',updated_at FROM draft_send_outcomes WHERE operation_id=?")
            .bind(&identity.operation_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
    pub async fn claim_approved_send(
        &self,
        identity: &SendIdentity,
        approval: &SendApproval,
        message_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE drafts SET status='sending', status_updated_at=unixepoch(), last_heartbeat_at=unixepoch() WHERE id=? AND account_id=? AND revision=? AND status='draft' AND NOT EXISTS (SELECT 1 FROM restored_draft_holds WHERE draft_id=drafts.id) AND NOT EXISTS (SELECT 1 FROM draft_send_outcomes WHERE draft_id=drafts.id AND state IN ('unknown','accepted')) AND EXISTS (SELECT 1 FROM archive_workspace WHERE workspace_id=? AND recovery_generation=?)")
            .bind(approval.draft.id.as_str()).bind(approval.draft.account_id.as_str()).bind(approval.stored_revision).bind(&approval.workspace_id).bind(&approval.recovery_generation).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO draft_send_outcomes (operation_id,draft_id,account_id,reviewed_revision,command_hash,state,rfc2822_message_id,updated_at) VALUES (?,?,?,?,?,'unknown',?,unixepoch())")
            .bind(&identity.operation_id).bind(approval.draft.id.as_str()).bind(approval.draft.account_id.as_str()).bind(identity.expected_revision).bind(&approval.command_hash).bind(message_id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO approved_sends (operation_id,approval_json,status,phase,created_at,updated_at) VALUES (?,?,'submitting','\"dispatch\"',unixepoch(),unixepoch())")
            .bind(&identity.operation_id).bind(crate::encode_json(approval)?).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO approved_send_attempts (operation_id,attempt,phase,outcome,recorded_at) VALUES (?,1,'\"dispatch\"','submitting',unixepoch())").bind(&identity.operation_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn get_send_approval(
        &self,
        operation_id: &str,
    ) -> Result<Option<SendApproval>, sqlx::Error> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT approval_json FROM approved_sends WHERE operation_id=?")
                .bind(operation_id)
                .fetch_optional(self.reader())
                .await?;
        json.map(|value| crate::decode_json(&value)).transpose()
    }

    pub async fn get_send_recovery_status(
        &self,
        operation_id: &str,
    ) -> Result<Option<(String, SendRecoveryStatus)>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT status,phase,attempts,retry_at,reason FROM approved_sends WHERE operation_id=?",
        )
        .bind(operation_id)
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            Ok((
                row.get("status"),
                SendRecoveryStatus {
                    phase: crate::decode_json(&row.get::<String, _>("phase"))?,
                    attempts: row.get::<i64, _>("attempts") as u32,
                    retry_at: row
                        .get::<Option<i64>, _>("retry_at")
                        .map(crate::decode_timestamp)
                        .transpose()?,
                    reason: row.get("reason"),
                },
            ))
        })
        .transpose()
    }

    pub async fn record_send_recovery(
        &self,
        identity: &SendIdentity,
        status: &str,
        phase: SendSubmissionPhase,
        retry_at: Option<DateTime<Utc>>,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let phase = crate::encode_json(&phase)?;
        sqlx::query("UPDATE approved_sends SET status=?,phase=?,retry_at=?,reason=?,updated_at=unixepoch() WHERE operation_id=? AND status IN ('submitting','unknown','waiting')")
            .bind(status).bind(&phase).bind(retry_at.map(|at| at.timestamp())).bind(reason).bind(&identity.operation_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE approved_send_attempts SET phase=?,outcome=?,recorded_at=unixepoch() WHERE operation_id=? AND attempt=(SELECT attempts FROM approved_sends WHERE operation_id=?)")
            .bind(&phase).bind(status).bind(&identity.operation_id).bind(&identity.operation_id).execute(&mut *tx).await?;
        if status == "waiting" {
            sqlx::query("UPDATE drafts SET status='draft',status_updated_at=unixepoch() WHERE id=(SELECT draft_id FROM draft_send_outcomes WHERE operation_id=? AND state='unknown')")
                .bind(&identity.operation_id).execute(&mut *tx).await?;
        }
        tx.commit().await
    }

    pub async fn claim_waiting_send(
        &self,
        identity: &SendIdentity,
        approval: &SendApproval,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE approved_sends SET status='submitting',phase='\"dispatch\"',attempts=attempts+1,retry_at=NULL,updated_at=unixepoch() WHERE operation_id=? AND status='waiting' AND retry_at<=? AND EXISTS (SELECT 1 FROM archive_workspace WHERE workspace_id=? AND recovery_generation=?)")
            .bind(&identity.operation_id).bind(now.timestamp()).bind(&approval.workspace_id).bind(&approval.recovery_generation).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        let changed = sqlx::query("UPDATE drafts SET status='sending',status_updated_at=unixepoch(),last_heartbeat_at=unixepoch() WHERE id=? AND account_id=? AND revision=? AND status='draft' AND NOT EXISTS (SELECT 1 FROM restored_draft_holds WHERE draft_id=drafts.id)")
            .bind(approval.draft.id.as_str()).bind(approval.draft.account_id.as_str()).bind(approval.stored_revision).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(false);
        }
        sqlx::query("INSERT INTO approved_send_attempts (operation_id,attempt,phase,outcome,recorded_at) SELECT operation_id,attempts,phase,'submitting',unixepoch() FROM approved_sends WHERE operation_id=?").bind(&identity.operation_id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn due_send_recovery(&self, now: DateTime<Utc>) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT a.operation_id FROM approved_sends a JOIN draft_send_outcomes o USING(operation_id) WHERE (a.status='waiting' AND a.retry_at<=?) OR (a.status IN ('submitting','unknown') AND COALESCE(a.retry_at,a.created_at+60)<=?) OR (o.state='accepted' AND o.local_message_id IS NULL AND COALESCE(a.retry_at,0)<=?) ORDER BY a.updated_at LIMIT 50")
            .bind(now.timestamp()).bind(now.timestamp()).bind(now.timestamp()).fetch_all(self.reader()).await
    }

    pub async fn postpone_send_recovery(
        &self,
        operation_id: &str,
        retry_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE approved_sends SET retry_at=?,reason='recovery_needs_attention',updated_at=unixepoch() WHERE operation_id=? AND status!='failed'")
            .bind(retry_at.timestamp()).bind(operation_id).execute(self.writer()).await?;
        Ok(())
    }

    /// Only waiting records have positive proof of non-submission. A changed
    /// draft cancels that approval; genuinely unknown submissions stay held.
    pub async fn cancel_waiting_send(
        &self,
        identity: &SendIdentity,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE approved_sends SET status='failed',reason=?,retry_at=NULL,updated_at=unixepoch() WHERE operation_id=? AND status='waiting'").bind(reason).bind(&identity.operation_id).execute(&mut *tx).await?.rows_affected();
        if changed == 1 {
            sqlx::query("UPDATE draft_send_outcomes SET state='not_accepted',updated_at=unixepoch() WHERE operation_id=? AND state='unknown'").bind(&identity.operation_id).execute(&mut *tx).await?;
            sqlx::query("UPDATE scheduled_send_attempts SET outcome='failed' WHERE outcome='waiting' AND draft_id=(SELECT draft_id FROM draft_send_outcomes WHERE operation_id=?)").bind(&identity.operation_id).execute(&mut *tx).await?;
        }
        tx.commit().await
    }
}
