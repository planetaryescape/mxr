//! Persistent bounded replay; readers never acknowledge projection work.
use crate::{decode_json, Store};
use mxr_core::{
    asks::{AskAssessment, ASKS_POLICY_VERSION},
    AccountId,
};

#[derive(Debug, Clone)]
pub struct AsksProjection {
    pub answer: AskAssessment,
    pub revision: i64,
}

impl Store {
    pub async fn pending_asks_projections(
        &self,
        account: &AccountId,
    ) -> Result<Vec<AsksProjection>, sqlx::Error> {
        let context = crate::asks_context_token().map_err(sqlx::Error::Io)?;
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT a.assessment_json, o.input_revision FROM asks_projection_outbox o JOIN mail_assessments a ON a.message_id = o.message_id AND a.account_id = o.account_id AND a.policy_version = ?2 AND a.input_revision = o.input_revision AND a.input_fingerprint = o.input_fingerprint JOIN messages m ON m.id = o.message_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id AND i.effective_revision = o.input_revision WHERE o.account_id = ?1 AND o.context_token = ?3 AND json_extract(a.assessment_json, '$.pending_body') = 0 ORDER BY o.message_id LIMIT 100")
            .bind(account.as_str()).bind(ASKS_POLICY_VERSION).bind(context).fetch_all(self.reader()).await?;
        rows.iter()
            .map(|(json, revision)| {
                Ok(AsksProjection {
                    answer: decode_json(json)?,
                    revision: *revision,
                })
            })
            .collect()
    }

    pub async fn acknowledge_asks_projection(
        &self,
        ticket: &AsksProjection,
    ) -> Result<bool, sqlx::Error> {
        let answer = &ticket.answer;
        let context = crate::asks_context_token().map_err(sqlx::Error::Io)?;
        let result = sqlx::query("DELETE FROM asks_projection_outbox WHERE message_id = ?1 AND account_id = ?2 AND input_fingerprint = ?3 AND context_token = ?4 AND context_token = ?5 AND input_revision = ?7 AND EXISTS (SELECT 1 FROM mail_assessments a JOIN messages m ON m.id = a.message_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE a.message_id = ?1 AND a.account_id = ?2 AND a.policy_version = ?6 AND a.input_fingerprint = ?3 AND a.input_revision = i.effective_revision AND a.input_revision = asks_projection_outbox.input_revision)")
            .bind(answer.message_id.as_str()).bind(answer.account_id.as_str()).bind(&answer.input_fingerprint).bind(&answer.context_token).bind(context).bind(ASKS_POLICY_VERSION).bind(ticket.revision).execute(self.writer()).await?;
        Ok(result.rows_affected() > 0)
    }
}
