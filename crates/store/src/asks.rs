//! Asks use the assessment storage envelope with their own policy and shape.
//! Home rows stay inbound-only. No migration or replacement of old rows is needed.
use crate::{decode_json, encode_json};
use mxr_core::asks::{AskAssessment, ASKS_POLICY_VERSION};
use mxr_core::{AccountId, MessageId, ThreadId};
use std::collections::HashMap;

type CachedBodies = HashMap<MessageId, (Option<String>, Option<String>)>;

impl super::Store {
    /// Full cached source parts for bounded asks batches. Desk snippets are
    /// intentionally truncated and cannot be the asks evidence boundary.
    pub async fn asks_source_bodies(
        &self,
        account: &AccountId,
        ids: &[MessageId],
    ) -> Result<CachedBodies, sqlx::Error> {
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as("SELECT b.message_id, b.text_plain, b.text_html FROM bodies b JOIN messages m ON m.id = b.message_id WHERE m.account_id = ?1 AND m.id IN (SELECT value FROM json_each(?2))")
            .bind(account.as_str()).bind(encode_json(&ids)?).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|(id, plain, html)| Ok((crate::decode_id(&id)?, (plain, html))))
            .collect()
    }

    /// Persist before projecting. The home batch acknowledges the dirty
    /// generations only after both facets have committed successfully.
    pub async fn save_asks_batch(
        &self,
        account: &AccountId,
        revisions: &HashMap<ThreadId, i64>,
        answers: &[AskAssessment],
    ) -> Result<bool, sqlx::Error> {
        if answers
            .iter()
            .any(|a| &a.account_id != account || a.policy_version != ASKS_POLICY_VERSION)
        {
            return Err(sqlx::Error::Protocol(
                "asks batch account or policy mismatch".into(),
            ));
        }
        let expected = encode_json(revisions)?;
        if answers.is_empty() {
            return Ok(!sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM json_each(?2) expected LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = expected.key WHERE COALESCE(i.effective_revision, -1) != expected.value)")
                .bind(account.as_str()).bind(&expected).fetch_one(self.reader()).await?);
        }
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let changed: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM json_each(?2) expected LEFT JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = expected.key WHERE COALESCE(i.effective_revision, -1) != expected.value)")
            .bind(account.as_str()).bind(&expected).fetch_one(&mut *tx).await?;
        if changed {
            tx.rollback().await?;
            return Ok(false);
        }
        for answer in answers {
            // Each row carries its own thread token, rather than the
            // maximum of a page whose other threads may be changing.
            let valid: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM messages m JOIN json_each(?3) expected ON expected.key = m.thread_id WHERE m.id = ?1 AND m.account_id = ?2)")
                .bind(answer.message_id.as_str()).bind(account.as_str()).bind(&expected).fetch_one(&mut *tx).await?;
            if !valid {
                tx.rollback().await?;
                return Err(sqlx::Error::Protocol(
                    "asks message is outside the observed threads".into(),
                ));
            }
            sqlx::query("INSERT INTO mail_assessments (message_id, account_id, policy_version, input_fingerprint, input_revision, assessment_json)
                SELECT m.id, m.account_id, ?3, ?4, expected.value, ?5 FROM messages m JOIN json_each(?6) expected ON expected.key = m.thread_id WHERE m.id = ?1 AND m.account_id = ?2
                ON CONFLICT(message_id, policy_version) DO UPDATE SET input_fingerprint = excluded.input_fingerprint, input_revision = excluded.input_revision, assessment_json = excluded.assessment_json
                WHERE mail_assessments.input_fingerprint != excluded.input_fingerprint OR mail_assessments.input_revision != excluded.input_revision")
                .bind(answer.message_id.as_str()).bind(account.as_str()).bind(ASKS_POLICY_VERSION)
                .bind(&answer.input_fingerprint).bind(encode_json(answer)?).bind(&expected)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Missing or stale asks are pending. Reading never judges or repairs.
    pub async fn asks_for_messages(
        &self,
        account: &AccountId,
        ids: &[MessageId],
    ) -> Result<HashMap<MessageId, AskAssessment>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar("SELECT a.assessment_json FROM mail_assessments a JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id
            WHERE a.account_id = ?1 AND a.policy_version = ?2 AND a.input_revision = i.effective_revision AND a.message_id IN (SELECT value FROM json_each(?3)) ORDER BY a.message_id")
            .bind(account.as_str()).bind(ASKS_POLICY_VERSION).bind(encode_json(&ids)?).fetch_all(self.reader()).await?;
        rows.iter()
            .map(|json| {
                let answer: AskAssessment = decode_json(json)?;
                Ok((answer.message_id.clone(), answer))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
