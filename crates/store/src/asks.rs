//! Asks use the assessment storage envelope with their own policy and shape.
//! Home rows stay inbound-only. No migration or replacement of old rows is needed.
use crate::{decode_json, encode_json};
use mxr_core::asks::{AskAssessment, ASKS_POLICY_VERSION};
use mxr_core::{AccountId, MessageId, ThreadId};
use std::collections::HashMap;

// Keep the requested IDs outermost: an account index scan makes per-message projection quadratic.
const ASKS_FOR_MESSAGES: &str = "SELECT a.assessment_json FROM (SELECT DISTINCT value FROM json_each(?3)) requested CROSS JOIN mail_assessments a ON a.message_id = requested.value JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id
            WHERE a.account_id = ?1 AND a.policy_version = ?2 AND a.input_revision = i.effective_revision ORDER BY a.message_id";

// Compute a thread's effective generation once, not once per message/facet.
// Read asks directly from the compact projection: a CASE over the authoritative
// table still visits its large source pages before choosing the summary.
const FACETS_FOR_MESSAGES: &str = "WITH wanted AS MATERIALIZED (
            SELECT m.id, m.thread_id FROM (SELECT DISTINCT value FROM json_each(?4)) requested
            CROSS JOIN messages m ON m.id = requested.value WHERE m.account_id = ?1
        ), revisions AS MATERIALIZED (
            SELECT i.thread_id, i.effective_revision FROM (SELECT DISTINCT thread_id FROM wanted) threads
            CROSS JOIN assessment_effective_thread_inputs i ON i.account_id = ?1 AND i.thread_id = threads.thread_id
        )
        SELECT a.policy_version, a.assessment_json
        FROM wanted CROSS JOIN mail_assessments a ON a.message_id = wanted.id AND a.policy_version = ?2 AND a.account_id = ?1
        JOIN revisions i ON i.thread_id = wanted.thread_id AND i.effective_revision = a.input_revision
        UNION ALL SELECT s.policy_version, s.summary_json
        FROM wanted CROSS JOIN ask_view_summaries s ON s.message_id = wanted.id AND s.policy_version = ?3 AND s.account_id = ?1
        JOIN revisions i ON i.thread_id = wanted.thread_id AND i.effective_revision = s.input_revision";

/// Mail views need answers, not the authored/quoted source zones and audit
/// evidence retained by a full asks assessment. Keep those on detail reads.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AskSummary {
    pub message_id: MessageId,
    pub input_fingerprint: String,
    pub owner_authored: bool,
    pub context_token: String,
    pub answers: Vec<mxr_core::asks::AskAnswer>,
}

impl From<AskAssessment> for AskSummary {
    fn from(answer: AskAssessment) -> Self {
        Self {
            message_id: answer.message_id,
            input_fingerprint: answer.input_fingerprint,
            owner_authored: answer.facts.owner_authored,
            context_token: answer.context_token,
            answers: answer.units.into_iter().map(|unit| unit.answer).collect(),
        }
    }
}

type CachedBodies = HashMap<MessageId, (Option<String>, Option<String>, String)>;

impl super::Store {
    /// Full cached source parts for bounded asks batches. Desk snippets are
    /// intentionally truncated and cannot be the asks evidence boundary.
    pub async fn asks_source_bodies(
        &self,
        account: &AccountId,
        ids: &[MessageId],
    ) -> Result<CachedBodies, sqlx::Error> {
        let rows: Vec<(String, Option<String>, Option<String>, String)> = sqlx::query_as("SELECT b.message_id, b.text_plain, b.text_html, b.metadata_json FROM bodies b JOIN messages m ON m.id = b.message_id WHERE m.account_id = ?1 AND m.id IN (SELECT value FROM json_each(?2))")
            .bind(account.as_str()).bind(encode_json(&ids)?).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|(id, plain, html, metadata)| {
                Ok((crate::decode_id(&id)?, (plain, html, metadata)))
            })
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
        write_asks(&mut tx, account, &expected, answers).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// A bounded, read-only uncertainty projection. Placement uncertainty is
    /// a separate facet and never enters this selection.
    pub async fn uncertain_asks(
        &self,
        account: Option<&AccountId>,
    ) -> Result<Vec<(ThreadId, String, AskAssessment)>, sqlx::Error> {
        let context = asks_context_token().map_err(sqlx::Error::Io)?;
        let rows: Vec<(String, String, String)> = sqlx::query_as("SELECT m.thread_id, substr(m.subject, 1, 240), a.assessment_json FROM mail_assessments a JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE (?1 IS NULL OR a.account_id = ?1) AND a.policy_version = ?2 AND a.input_revision = i.effective_revision AND json_extract(a.assessment_json, '$.context_token') = ?3 AND json_extract(a.assessment_json, '$.pending_body') = 0 AND (m.flags & ?4) = 0 AND EXISTS (SELECT 1 FROM json_each(a.assessment_json, '$.units') u WHERE json_extract(u.value, '$.answer') = 'not_sure') ORDER BY m.date DESC, m.id LIMIT 100")
            .bind(account.map(AccountId::as_str)).bind(ASKS_POLICY_VERSION).bind(context).bind(i64::from((mxr_core::MessageFlags::SPAM | mxr_core::MessageFlags::TRASH).bits())).fetch_all(self.reader()).await?;
        rows.into_iter()
            .map(|(thread, subject, json)| {
                Ok((crate::decode_id(&thread)?, subject, decode_json(&json)?))
            })
            .collect()
    }

    /// Both facets are read from one SQLite snapshot. Inbound homes without a
    /// current asks facet stay pending rather than mixing generations.
    pub async fn assessment_facets_for_messages(
        &self,
        account: &AccountId,
        ids: &[MessageId],
    ) -> Result<
        (
            HashMap<MessageId, mxr_core::assessment::MailAssessment>,
            HashMap<MessageId, AskSummary>,
        ),
        sqlx::Error,
    > {
        let context = asks_context_token().map_err(sqlx::Error::Io)?;
        let rows: Vec<(String, String)> = sqlx::query_as(FACETS_FOR_MESSAGES)
            .bind(account.as_str())
            .bind(mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
            .bind(ASKS_POLICY_VERSION)
            .bind(encode_json(&ids)?)
            .fetch_all(self.reader())
            .await?;
        let mut homes = HashMap::new();
        let mut asks = HashMap::new();
        for (policy, json) in rows {
            if policy == ASKS_POLICY_VERSION {
                let answer: AskSummary = decode_json(&json)?;
                if answer.context_token == context {
                    asks.insert(answer.message_id.clone(), answer);
                }
            } else {
                let answer: mxr_core::assessment::MailAssessment = decode_json(&json)?;
                homes.insert(answer.message_id.clone(), answer);
            }
        }
        homes.retain(|id, _| asks.get(id).is_some_and(|a| !a.owner_authored));
        Ok((homes, asks))
    }

    /// Missing or stale asks are pending. Reading never judges or repairs.
    pub async fn asks_for_messages(
        &self,
        account: &AccountId,
        ids: &[MessageId],
    ) -> Result<HashMap<MessageId, AskAssessment>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(ASKS_FOR_MESSAGES)
            .bind(account.as_str())
            .bind(ASKS_POLICY_VERSION)
            .bind(encode_json(&ids)?)
            .fetch_all(self.reader())
            .await?;
        let context = asks_context_token().map_err(sqlx::Error::Io)?;
        rows.iter()
            .map(|json| {
                let answer: AskAssessment = decode_json(json)?;
                Ok((answer.message_id.clone(), answer))
            })
            .collect::<Result<HashMap<_, _>, _>>()
            .map(|mut answers| {
                answers.retain(|_, a| a.context_token == context);
                answers
            })
    }
}

#[cfg(test)]
mod lookup_tests;
#[cfg(test)]
mod tests;

/// Date interpretation depends on the zone's rules, including future DST,
/// rather than only its offset at the message date. Hash the actual local
/// tzfile and any process override; failure leaves asks explicitly pending.
pub fn asks_context_token() -> Result<String, std::io::Error> {
    Ok(crate::AsksTimezone::resolve()?.token().to_string())
}

pub(crate) async fn write_asks(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account: &AccountId,
    expected: &str,
    answers: &[AskAssessment],
) -> Result<(), sqlx::Error> {
    if answers
        .iter()
        .any(|a| &a.account_id != account || a.policy_version != ASKS_POLICY_VERSION)
    {
        return Err(sqlx::Error::Protocol(
            "asks batch account or policy mismatch".into(),
        ));
    }
    for answer in answers {
        // Each row carries its own thread token, rather than the
        // maximum of a page whose other threads may be changing.
        let valid: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM messages m JOIN json_each(?3) expected ON expected.key = m.thread_id WHERE m.id = ?1 AND m.account_id = ?2)")
                .bind(answer.message_id.as_str()).bind(account.as_str()).bind(expected).fetch_one(&mut **tx).await?;
        if !valid {
            return Err(sqlx::Error::Protocol(
                "asks message is outside the observed threads".into(),
            ));
        }
        let written = sqlx::query("INSERT INTO mail_assessments (message_id, account_id, policy_version, input_fingerprint, input_revision, assessment_json)
                SELECT m.id, m.account_id, ?3, ?4, expected.value, ?5 FROM messages m JOIN json_each(?6) expected ON expected.key = m.thread_id WHERE m.id = ?1 AND m.account_id = ?2
                ON CONFLICT(message_id, policy_version) DO UPDATE SET input_fingerprint = excluded.input_fingerprint, input_revision = excluded.input_revision, assessment_json = excluded.assessment_json
                WHERE mail_assessments.input_fingerprint != excluded.input_fingerprint OR mail_assessments.input_revision != excluded.input_revision")
                .bind(answer.message_id.as_str()).bind(account.as_str()).bind(ASKS_POLICY_VERSION)
                .bind(&answer.input_fingerprint).bind(encode_json(answer)?).bind(expected)
                .execute(&mut **tx).await?;
        if written.rows_affected() > 0 {
            sqlx::query("INSERT INTO asks_projection_outbox(message_id, account_id, input_revision, input_fingerprint, context_token) SELECT message_id, account_id, input_revision, input_fingerprint, ?3 FROM mail_assessments WHERE message_id = ?1 AND policy_version = ?2 ON CONFLICT(message_id) DO UPDATE SET input_revision = excluded.input_revision, input_fingerprint = excluded.input_fingerprint, context_token = excluded.context_token")
                .bind(answer.message_id.as_str()).bind(ASKS_POLICY_VERSION).bind(&answer.context_token).execute(&mut **tx).await?;
        }
        crate::asks_commitments::write(tx, answer).await?;
    }
    Ok(())
}
