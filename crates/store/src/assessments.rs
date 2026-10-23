//! Stored classifier answers. Reads never materialize or repair them.
use crate::{decode_json, encode_json};
use mxr_core::{assessment::MailAssessment, AccountId, MessageId, ThreadId};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::HashMap;

impl super::Store {
    pub async fn assessment_thread_revisions(
        &self,
        account: &AccountId,
        threads: &[ThreadId],
    ) -> Result<HashMap<ThreadId, i64>, sqlx::Error> {
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT w.value, COALESCE(i.effective_revision, -1) FROM json_each(?2) w LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = w.value")
            .bind(account.as_str()).bind(encode_json(&threads)?).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|(id, revision)| Ok((crate::decode_id(&id)?, revision)))
            .collect()
    }

    /// Commit one coherent batch or none of it. A rejected CAS is pending,
    /// whereas a SQL failure remains an error. Never return computed rows.
    pub async fn save_assessment_batch(
        &self,
        account: &AccountId,
        revisions: &HashMap<ThreadId, i64>,
        answers: &[MailAssessment],
    ) -> Result<bool, sqlx::Error> {
        let expected = encode_json(revisions)?;
        // A settled read must not queue behind the writer. One SELECT checks
        // both CAS and pending acknowledgements against one SQLite snapshot.
        if answers.is_empty() {
            let (changed, dirty): (bool, bool) = sqlx::query_as("SELECT COALESCE(max(COALESCE(i.effective_revision, -1) != expected.value), 0), COALESCE(max(i.dirty != 0 OR i.revision != i.effective_revision), 0) FROM json_each(?2) expected LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = expected.key")
                .bind(account.as_str()).bind(&expected).fetch_one(self.reader()).await?;
            if changed || !dirty {
                return Ok(!changed);
            }
        }
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let changed: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM json_each(?2) expected LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = expected.key WHERE COALESCE(i.effective_revision, -1) != expected.value)")
            .bind(account.as_str()).bind(&expected).fetch_one(&mut *tx).await?;
        if changed {
            tx.rollback().await?;
            return Ok(false);
        }
        for answer in answers {
            sqlx::query("INSERT INTO mail_assessments (message_id, account_id, policy_version, input_fingerprint, input_revision, assessment_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(message_id, policy_version) DO UPDATE SET input_fingerprint = excluded.input_fingerprint, input_revision = excluded.input_revision, assessment_json = excluded.assessment_json
                WHERE mail_assessments.input_fingerprint != excluded.input_fingerprint")
                .bind(answer.message_id.as_str()).bind(account.as_str()).bind(&answer.policy_version)
                .bind(&answer.input_fingerprint).bind(revisions.values().max().copied().unwrap_or(0))
                .bind(encode_json(answer)?).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE assessment_thread_inputs SET dirty = 0, revision = (SELECT value FROM json_each(?2) WHERE key = thread_id) WHERE account_id = ?1 AND thread_id IN (SELECT key FROM json_each(?2) WHERE assessment_thread_inputs.dirty != 0 OR assessment_thread_inputs.revision != value)")
            .bind(account.as_str()).bind(&expected).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn dirty_assessment_threads(
        &self,
        account: &AccountId,
        after: Option<&str>,
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT thread_id FROM assessment_effective_thread_inputs WHERE account_id = ?1 AND (dirty = 1 OR effective_revision > revision) AND thread_id > COALESCE(?2, '') ORDER BY thread_id LIMIT 100")
            .bind(account.as_str()).bind(after).fetch_all(self.reader()).await?;
        ids.iter().map(|id| crate::decode_id(id)).collect()
    }

    /// Startup and config changes must also cover historical/no-arrivals mail.
    pub async fn invalidate_all_assessments(&self) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE assessment_inputs SET revision = revision + 1 WHERE id = 1")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO assessment_thread_inputs (account_id, thread_id, revision, dirty) SELECT DISTINCT account_id, thread_id, (SELECT revision FROM assessment_inputs WHERE id = 1), 1 FROM messages WHERE 1 ON CONFLICT(account_id, thread_id) DO UPDATE SET revision = excluded.revision, dirty = 1")
            .execute(&mut *tx).await?;
        tx.commit().await
    }
    pub async fn assessment_report_rows(
        &self,
        accounts: &[AccountId],
        policy: &str,
        limit: u32,
    ) -> Result<Vec<Option<MailAssessment>>, sqlx::Error> {
        let rows = sqlx::query("SELECT a.assessment_json FROM messages m LEFT JOIN mail_assessments a ON a.message_id = m.id AND a.account_id = m.account_id AND a.policy_version = ?2
            WHERE m.account_id IN (SELECT value FROM json_each(?1)) AND m.direction != 'outbound' ORDER BY m.date DESC, m.id LIMIT ?3")
            .bind(encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())?).bind(policy).bind(limit.min(10001))
            .fetch_all(self.reader()).await?;
        rows.iter()
            .map(|row| {
                row.try_get::<Option<String>, _>("assessment_json")?
                    .map(|json| decode_json(&json))
                    .transpose()
            })
            .collect()
    }

    pub async fn assessment_input_revision(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT revision FROM assessment_inputs WHERE id = 1")
            .fetch_one(self.reader())
            .await
    }

    pub async fn assessments(
        &self,
        account: &AccountId,
        policy: &str,
    ) -> Result<HashMap<MessageId, MailAssessment>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar("SELECT assessment_json FROM mail_assessments WHERE account_id = ? AND policy_version = ? ORDER BY message_id")
            .bind(account.as_str()).bind(policy).fetch_all(self.reader()).await?;
        rows.iter()
            .map(|json| {
                let value: MailAssessment = decode_json(json)?;
                Ok((value.message_id.clone(), value))
            })
            .collect()
    }

    pub async fn assessments_for_messages(
        &self,
        account: &AccountId,
        policy: &str,
        ids: &[MessageId],
    ) -> Result<HashMap<MessageId, MailAssessment>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar("SELECT assessment_json FROM mail_assessments WHERE account_id = ?1 AND policy_version = ?2 AND message_id IN (SELECT value FROM json_each(?3)) ORDER BY message_id")
            .bind(account.as_str()).bind(policy).bind(encode_json(&ids.iter().map(MessageId::as_str).collect::<Vec<_>>())?).fetch_all(self.reader()).await?;
        rows.iter()
            .map(|json| {
                let value: MailAssessment = decode_json(json)?;
                Ok((value.message_id.clone(), value))
            })
            .collect()
    }

    /// Reject a result if any observed input changed while it was being read.
    /// SQLite serializes this predicate with the input mutation's trigger.
    pub async fn save_assessment(
        &self,
        assessment: &MailAssessment,
        revision: i64,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("INSERT INTO mail_assessments (message_id, account_id, policy_version, input_fingerprint, input_revision, assessment_json)
            SELECT ?1, ?2, ?3, ?4, ?5, ?6 WHERE (SELECT revision FROM assessment_inputs WHERE id = 1) = ?5
            AND EXISTS (SELECT 1 FROM messages WHERE id = ?1 AND account_id = ?2)
            ON CONFLICT(message_id, policy_version) DO UPDATE SET
                input_fingerprint = excluded.input_fingerprint, input_revision = excluded.input_revision, assessment_json = excluded.assessment_json
            WHERE mail_assessments.input_fingerprint != excluded.input_fingerprint")
            .bind(assessment.message_id.as_str()).bind(assessment.account_id.as_str()).bind(&assessment.policy_version)
            .bind(&assessment.input_fingerprint).bind(revision).bind(encode_json(assessment)?)
            .execute(self.writer()).await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn assessment_threads_page(
        &self,
        account: &AccountId,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT DISTINCT thread_id FROM messages WHERE account_id = ? AND (?2 IS NULL OR thread_id > ?2) ORDER BY thread_id LIMIT ?3")
            .bind(account.as_str()).bind(after).bind(limit.min(100)).fetch_all(self.reader()).await?;
        ids.iter().map(|id| crate::decode_id(id)).collect()
    }

    pub async fn assessment_versions(
        &self,
        accounts: &[AccountId],
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT DISTINCT policy_version FROM mail_assessments WHERE account_id IN (SELECT value FROM json_each(?)) ORDER BY policy_version")
            .bind(encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())?).fetch_all(self.reader()).await
    }
}

pub fn assessment_fingerprint(input: &str) -> String {
    base16ct::lower::encode_string(&Sha256::digest(input.as_bytes()))
}

#[cfg(test)]
mod tests;
