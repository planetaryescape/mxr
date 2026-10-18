//! Durable task links, shared-field baselines and uncertain-write receipts.

use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoMicrosoftExport {
    pub todo_id: String,
    pub state: String,
    pub task_id: Option<String>,
    pub operation_id: Option<String>,
    pub payload_json: String,
    pub baseline_json: Option<String>,
    pub last_error: Option<String>,
    pub uncertain_json: Option<String>,
}

fn export_row(row: sqlx::sqlite::SqliteRow) -> Result<TodoMicrosoftExport, sqlx::Error> {
    Ok(TodoMicrosoftExport {
        todo_id: row.try_get("todo_id")?,
        state: row.try_get("state")?,
        task_id: row.try_get("task_id")?,
        operation_id: row.try_get("operation_id")?,
        payload_json: row.try_get("payload_json")?,
        baseline_json: row.try_get("baseline_json")?,
        last_error: row.try_get("last_error")?,
        uncertain_json: row.try_get("uncertain_json")?,
    })
}

impl crate::Store {
    /// Apply only shared fields, and only while the snapshot we merged is
    /// still current. A concurrent user edit is never overwritten.
    pub async fn apply_linked_todo_fields(
        &self,
        expected: &crate::TodoRecord,
        desired: &crate::TodoRecord,
    ) -> Result<bool, sqlx::Error> {
        let stamp = |at: Option<chrono::DateTime<chrono::Utc>>| at.map(|at| at.timestamp());
        let mut tx = self.writer().begin().await?;
        let changed = sqlx::query("UPDATE todos SET title = ?, due_at = ?, due_words = ?, act_by_at = ?, surface_at = ?, relevant_until = ?, window_source = ?, scheduled_for = ?, state = ?, done_at = ?, dismissed_at = ?, expired_at = ?, expired_at_birth = ?, catchup = ?, field_sources = ?, user_edited = 1, updated_at = ? WHERE id = ? AND title = ? AND due_at IS ? AND scheduled_for IS ? AND state = ? AND kind = ? AND field_sources = ? AND due_words IS ? AND act_by_at IS ? AND surface_at IS ? AND relevant_until IS ? AND window_source IS ? AND done_at IS ? AND dismissed_at IS ? AND expired_at IS ? AND expired_at_birth = ? AND catchup IS ? AND commitment_id IS ?")
            .bind(&desired.title)
            .bind(stamp(desired.due_at))
            .bind(&desired.due_words)
            .bind(stamp(desired.act_by_at))
            .bind(stamp(desired.surface_at))
            .bind(stamp(desired.relevant_until))
            .bind(&desired.window_source)
            .bind(stamp(desired.scheduled_for))
            .bind(desired.state.as_str())
            .bind(stamp(desired.done_at))
            .bind(stamp(desired.dismissed_at))
            .bind(stamp(desired.expired_at))
            .bind(desired.expired_at_birth)
            .bind(desired.catchup.map(crate::TodoCatchup::as_str))
            .bind(&desired.field_sources)
            .bind(chrono::Utc::now().timestamp())
            .bind(&expected.id)
            .bind(&expected.title)
            .bind(stamp(expected.due_at))
            .bind(stamp(expected.scheduled_for))
            .bind(expected.state.as_str())
            .bind(&expected.kind)
            .bind(&expected.field_sources)
            .bind(&expected.due_words)
            .bind(stamp(expected.act_by_at))
            .bind(stamp(expected.surface_at))
            .bind(stamp(expected.relevant_until))
            .bind(&expected.window_source)
            .bind(stamp(expected.done_at))
            .bind(stamp(expected.dismissed_at))
            .bind(stamp(expected.expired_at))
            .bind(expected.expired_at_birth)
            .bind(expected.catchup.map(crate::TodoCatchup::as_str))
            .bind(&expected.commitment_id)
            .execute(&mut *tx)
            .await?
            .rows_affected() == 1;
        // The promise mirror must commit with the local task. Otherwise a
        // failed mirror can undo a Microsoft reopening on the next To do tick.
        if changed
            && expected.state != desired.state
            && matches!(
                desired.state,
                crate::TodoState::Open | crate::TodoState::Done
            )
        {
            if let Some(commitment_id) = &expected.commitment_id {
                let status = if desired.state == crate::TodoState::Done {
                    "resolved"
                } else {
                    "open"
                };
                sqlx::query("UPDATE contact_commitments SET status = ?, resolved_at = ? WHERE id = ? AND account_id = ?")
                    .bind(status)
                    .bind(stamp(desired.done_at))
                    .bind(commitment_id)
                    .bind(expected.account_id.as_str())
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(changed)
    }

    pub async fn todo_microsoft_export(
        &self,
        todo_id: &str,
    ) -> Result<Option<TodoMicrosoftExport>, sqlx::Error> {
        sqlx::query("SELECT * FROM todo_microsoft_exports WHERE todo_id = ?")
            .bind(todo_id)
            .fetch_optional(self.reader())
            .await?
            .map(export_row)
            .transpose()
    }

    /// Only one caller may launch the CLI, even across daemon requests.
    pub async fn claim_todo_microsoft_export(
        &self,
        expected: &crate::TodoRecord,
        payload_json: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let claimed = sqlx::query("INSERT INTO todo_microsoft_exports (todo_id, state, started_at, payload_json) SELECT id, 'sending', ?, ? FROM todos WHERE id = ? AND title = ? AND due_at IS ? AND scheduled_for IS ? AND state = ? AND reason = ? AND counterparty IS ? AND source_message_id IS ? AND action_url IS ? ON CONFLICT(todo_id) DO NOTHING")
            .bind(chrono::Utc::now().timestamp())
            .bind(payload_json)
            .bind(&expected.id)
            .bind(&expected.title)
            .bind(expected.due_at.map(|at| at.timestamp()))
            .bind(expected.scheduled_for.map(|at| at.timestamp()))
            .bind(expected.state.as_str())
            .bind(&expected.reason)
            .bind(&expected.counterparty)
            .bind(expected.source_message_id.as_ref().map(|id| id.as_str()))
            .bind(&expected.action_url)
            .execute(&mut *tx)
            .await?
            .rows_affected() == 1;
        if claimed {
            // Protect the linked task from ageing and redetection in the same
            // transaction as the receipt, before any external effect.
            sqlx::query("UPDATE todos SET user_edited = 1 WHERE id = ?")
                .bind(&expected.id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(claimed)
    }

    pub async fn accept_todo_microsoft_export(
        &self,
        todo_id: &str,
        task_id: &str,
        operation_id: &str,
        baseline_json: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE todo_microsoft_exports SET state = 'accepted', task_id = ?, operation_id = ?, baseline_json = ? WHERE todo_id = ?")
            .bind(task_id)
            .bind(operation_id)
            .bind(baseline_json)
            .bind(todo_id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn list_todo_microsoft_exports(
        &self,
        due_only: bool,
        accounts: Option<&[mxr_core::AccountId]>,
    ) -> Result<Vec<TodoMicrosoftExport>, sqlx::Error> {
        let rows = sqlx::query("SELECT exports.* FROM todo_microsoft_exports exports JOIN todos ON todos.id = exports.todo_id WHERE exports.state = 'accepted' AND (? IS NULL OR todos.account_id IN (SELECT value FROM json_each(?))) AND (? = 0 OR last_attempt_at IS NULL OR last_attempt_at <= ?) ORDER BY COALESCE(last_attempt_at, 0), todo_id LIMIT ?")
            .bind(accounts.map(sqlx::types::Json))
            .bind(accounts.map(sqlx::types::Json))
            .bind(due_only)
            .bind(chrono::Utc::now().timestamp() - 30)
            .bind(if due_only { 100_i64 } else { -1_i64 })
            .fetch_all(self.reader())
            .await?;
        rows.into_iter().map(export_row).collect()
    }

    pub async fn note_todo_microsoft_sync(
        &self,
        todo_id: &str,
        baseline_json: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE todo_microsoft_exports SET baseline_json = COALESCE(?, baseline_json), last_attempt_at = ?, last_synced_at = CASE WHEN ? IS NULL THEN ? ELSE last_synced_at END, last_error = ?, uncertain_json = CASE WHEN ? IS NOT NULL THEN NULL ELSE uncertain_json END WHERE todo_id = ?")
            .bind(baseline_json)
            .bind(chrono::Utc::now().timestamp())
            .bind(error)
            .bind(chrono::Utc::now().timestamp())
            .bind(error)
            .bind(baseline_json)
            .bind(todo_id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn reserve_todo_microsoft_sync(
        &self,
        todo_id: &str,
        desired_json: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE todo_microsoft_exports SET uncertain_json = ? WHERE todo_id = ?")
            .bind(desired_json)
            .bind(todo_id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Used only when the CLI definitively refused or was never started.
    pub async fn release_todo_microsoft_export(&self, todo_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM todo_microsoft_exports WHERE todo_id = ? AND state = 'sending'")
            .bind(todo_id)
            .execute(self.writer())
            .await?;
        Ok(())
    }
}
