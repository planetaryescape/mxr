//! Bounded upgrade of automatic tasks without replacing user-owned history.
use crate::{Store, TodoRecord};
use mxr_core::AccountId;

impl Store {
    /// Manual/history rows stand on the user's decision. Automatic open rows
    /// stand only on the exact current stored answer that produced them.
    pub async fn fresh_asks_todos(
        &self,
        mut rows: Vec<TodoRecord>,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let mut accounts = std::collections::HashMap::<AccountId, Vec<mxr_core::MessageId>>::new();
        for row in &rows {
            if row.has_stored_asks() && row.state == crate::TodoState::Open && !row.user_touched() {
                if let Some(id) = &row.source_message_id {
                    accounts
                        .entry(row.account_id.clone())
                        .or_default()
                        .push(id.clone());
                }
            }
        }
        let mut current = std::collections::HashMap::new();
        for (account, ids) in accounts {
            current.extend(self.asks_for_messages(&account, &ids).await?);
        }
        rows.retain(|row| {
            if row.state != crate::TodoState::Open || row.user_touched() {
                return true;
            }
            if row.rules_version < 2 && matches!(row.origin.as_str(), "rule" | "schema" | "model") {
                return false;
            }
            if !row.has_stored_asks() {
                return true;
            }
            let expected = serde_json::from_str::<serde_json::Value>(&row.field_sources)
                .ok()
                .and_then(|v| {
                    v.get("asks_fingerprint")
                        .and_then(|p| p.get("evidence"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                });
            row.source_message_id
                .as_ref()
                .and_then(|id| current.get(id))
                .is_some_and(|a| !a.pending_body && Some(&a.input_fingerprint) == expected.as_ref())
        });
        Ok(rows)
    }

    pub async fn withdraw_asks_todos(
        &self,
        answer: &mxr_core::asks::AskAssessment,
        quotes: &[String],
    ) -> Result<(), sqlx::Error> {
        if answer.pending_body {
            return Ok(());
        }
        sqlx::query("UPDATE todos SET state = 'expired' WHERE account_id = ?1 AND source_message_id = ?2 AND json_extract(field_sources, '$.asks_fingerprint.evidence') IS NOT NULL AND kind != 'rsvp' AND state = 'open' AND user_edited = 0 AND scheduled_for IS NULL AND reason NOT IN (SELECT value FROM json_each(?3)) AND EXISTS (SELECT 1 FROM mail_assessments a JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE a.message_id = ?2 AND a.account_id = ?1 AND a.policy_version = ?4 AND a.input_fingerprint = ?5 AND a.input_revision = i.effective_revision)")
            .bind(answer.account_id.as_str()).bind(answer.message_id.as_str()).bind(crate::encode_json(&quotes)?).bind(mxr_core::asks::ASKS_POLICY_VERSION).bind(&answer.input_fingerprint).execute(self.writer()).await?;
        Ok(())
    }

    pub async fn legacy_asks_todos(
        &self,
        account: &AccountId,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!("SELECT {} FROM todos WHERE account_id = ? AND rules_version < 2 AND origin IN ('rule', 'schema', 'model') ORDER BY id LIMIT 100", crate::todos::COLUMNS);
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account.as_str())
            .fetch_all(self.reader())
            .await?;
        rows.iter().map(crate::todos::row_to_todo).collect()
    }

    pub async fn upgrade_asks_todo(
        &self,
        before: &TodoRecord,
        replacement: Option<&TodoRecord>,
        source: Option<&mxr_core::asks::AskAssessment>,
        ambiguous_keys: &[String],
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        if let Some(source) = source {
            if source.account_id != before.account_id
                || before.source_message_id.as_ref() != Some(&source.message_id)
            {
                return Err(sqlx::Error::Protocol(
                    "legacy to-do source does not match assessment".into(),
                ));
            }
            let current: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mail_assessments a JOIN messages m ON m.id = a.message_id AND m.account_id = a.account_id JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE a.message_id = ? AND a.account_id = ? AND a.policy_version = ? AND a.input_fingerprint = ? AND a.input_revision = i.effective_revision AND json_extract(a.assessment_json, '$.context_token') = ?)")
                .bind(source.message_id.as_str()).bind(before.account_id.as_str()).bind(mxr_core::asks::ASKS_POLICY_VERSION).bind(&source.input_fingerprint).bind(crate::asks_context_token().map_err(sqlx::Error::Io)?).fetch_one(&mut *tx).await?;
            if !current {
                tx.rollback().await?;
                return Ok(false);
            }
        } else if before.source_message_id.is_some() {
            tx.rollback().await?;
            return Ok(false);
        }
        let sql = format!("SELECT {} FROM todos WHERE id = ?", crate::todos::COLUMNS);
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&before.id)
            .fetch_one(&mut *tx)
            .await?;
        if crate::todos::row_to_todo(&row)? != *before {
            tx.rollback().await?;
            return Ok(false);
        }
        if let Some(source) = source {
            sqlx::query("INSERT INTO asks_projection_outbox(message_id, account_id, input_revision, input_fingerprint, context_token) SELECT message_id, account_id, input_revision, input_fingerprint, ?3 FROM mail_assessments WHERE message_id = ?1 AND policy_version = ?2 ON CONFLICT(message_id) DO UPDATE SET input_revision = excluded.input_revision, input_fingerprint = excluded.input_fingerprint, context_token = excluded.context_token")
                .bind(source.message_id.as_str()).bind(mxr_core::asks::ASKS_POLICY_VERSION).bind(&source.context_token).execute(&mut *tx).await?;
        }
        if !ambiguous_keys.is_empty()
            && (before.user_touched()
                || matches!(
                    before.state,
                    crate::TodoState::Done | crate::TodoState::Dismissed
                ))
        {
            for key in ambiguous_keys {
                sqlx::query("INSERT INTO asks_legacy_links(todo_id, dedup_key) VALUES(?, ?) ON CONFLICT DO NOTHING")
                    .bind(&before.id).bind(key).execute(&mut *tx).await?;
            }
        }
        if let Some(replacement) = replacement {
            // Never merge two histories: the caller must finish this upgrade
            // before inserting new automatic rows for the account.
            let sql = format!(
                "SELECT {} FROM todos WHERE account_id = ? AND dedup_key = ? AND id != ?",
                crate::todos::COLUMNS
            );
            if let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(before.account_id.as_str())
                .bind(&replacement.dedup_key)
                .bind(&before.id)
                .fetch_optional(&mut *tx)
                .await?
            {
                let other = crate::todos::row_to_todo(&row)?;
                let protected = |r: &TodoRecord| {
                    r.user_touched()
                        || matches!(
                            r.state,
                            crate::TodoState::Done | crate::TodoState::Dismissed
                        )
                };
                if protected(before) && !protected(&other) {
                    sqlx::query("UPDATE todos SET dedup_key = ?, state = 'expired', rules_version = 2 WHERE id = ?")
                        .bind(format!("asks-superseded|{}", other.id)).bind(&other.id).execute(&mut *tx).await?;
                } else {
                    sqlx::query("UPDATE todos SET rules_version = 2, state = CASE WHEN state = 'open' AND user_edited = 0 AND scheduled_for IS NULL THEN 'expired' ELSE state END WHERE id = ?")
                        .bind(&before.id).execute(&mut *tx).await?;
                    tx.commit().await?;
                    return Ok(true);
                }
            }
            let mut fields: serde_json::Map<String, serde_json::Value> =
                crate::decode_json(&before.field_sources)?;
            let source_fields: serde_json::Map<String, serde_json::Value> =
                crate::decode_json(&replacement.field_sources)?;
            for key in ["asks_fingerprint", "asks_provenance", "quote", "requester"] {
                if let Some(value) = source_fields.get(key) {
                    fields.insert(key.into(), value.clone());
                }
            }
            sqlx::query("UPDATE todos SET dedup_key = ?, rules_version = 2, reason = ?, field_sources = ? WHERE id = ?")
                .bind(&replacement.dedup_key).bind(&replacement.reason).bind(crate::encode_json(&fields)?).bind(&before.id).execute(&mut *tx).await?;
        } else {
            // Unsupported old verdicts stop surfacing, but manual edits and
            // all completed/dismissed history retain their exact state.
            sqlx::query("UPDATE todos SET rules_version = 2, state = CASE WHEN state = 'open' AND user_edited = 0 AND scheduled_for IS NULL THEN 'expired' ELSE state END WHERE id = ?")
                .bind(&before.id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(true)
    }
}
