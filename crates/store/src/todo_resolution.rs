//! Local evidence and revision history for later-mail to-do offers.

use crate::{decode_id, Store, TodoRecord, TodoState};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoResolutionAction {
    Complete,
    ReviseDate,
}

impl TodoResolutionAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::ReviseDate => "revise_date",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoResolutionStatus {
    Pending,
    Accepted,
    Dismissed,
    Undone,
    Superseded,
}

impl TodoResolutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Dismissed => "dismissed",
            Self::Undone => "undone",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoResolutionSuggestion {
    pub id: String,
    pub todo_id: String,
    pub account_id: AccountId,
    pub source_message_id: MessageId,
    pub source_thread_id: ThreadId,
    pub source_date: DateTime<Utc>,
    pub source_quote: String,
    pub quote_location: String,
    pub source_method: String,
    pub action: TodoResolutionAction,
    pub proposed_due_at: Option<DateTime<Utc>>,
    pub proposed_due_words: Option<String>,
    pub task_updated_at: DateTime<Utc>,
    pub task_fingerprint: String,
    pub status: TodoResolutionStatus,
    pub snapshot_json: Option<String>,
    pub applied_updated_at: Option<DateTime<Utc>>,
    pub applied_fingerprint: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTodoResolutionSuggestion {
    pub id: String,
    pub todo_id: String,
    pub account_id: AccountId,
    pub source_message_id: MessageId,
    pub source_thread_id: ThreadId,
    pub source_date: DateTime<Utc>,
    pub source_quote: String,
    pub quote_location: String,
    pub source_method: String,
    pub action: TodoResolutionAction,
    pub proposed_due_at: Option<DateTime<Utc>>,
    pub proposed_due_words: Option<String>,
    pub task_updated_at: DateTime<Utc>,
    /// Captured before matching/model work; checked again in the insert transaction.
    pub task_fingerprint: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TodoResolutionDates {
    pub due_at: DateTime<Utc>,
    pub due_words: String,
    pub act_by_at: Option<DateTime<Utc>>,
    pub surface_at: Option<DateTime<Utc>>,
    pub relevant_until: Option<DateTime<Utc>>,
    pub field_sources: String,
}

fn fingerprint(row: &sqlx::sqlite::SqliteRow) -> Result<String, sqlx::Error> {
    let identity = (
        row.try_get::<String, _>("id")?,
        row.try_get::<String, _>("account_id")?,
        row.try_get::<Option<String>, _>("thread_id")?,
        row.try_get::<Option<String>, _>("source_message_id")?,
        row.try_get::<i64, _>("updated_at")?,
        row.try_get::<String, _>("kind")?,
        row.try_get::<String, _>("verb")?,
        row.try_get::<String, _>("title")?,
        row.try_get::<Option<String>, _>("counterparty")?,
        row.try_get::<Option<i64>, _>("amount_minor")?,
        row.try_get::<Option<String>, _>("currency")?,
    );
    let dates = (
        row.try_get::<Option<i64>, _>("due_at")?,
        row.try_get::<Option<String>, _>("due_words")?,
        row.try_get::<Option<i64>, _>("act_by_at")?,
        row.try_get::<Option<i64>, _>("surface_at")?,
        row.try_get::<Option<i64>, _>("scheduled_for")?,
        row.try_get::<Option<i64>, _>("relevant_until")?,
        row.try_get::<String, _>("field_sources")?,
        row.try_get::<String, _>("state")?,
        row.try_get::<i64, _>("user_edited")?,
        row.try_get::<String, _>("origin")?,
    );
    Ok(fingerprint_values(identity, dates))
}

fn fingerprint_values<I, D>(identity: I, dates: D) -> String
where
    I: Serialize,
    D: Serialize,
{
    let bytes = serde_json::to_vec(&(identity, dates)).expect("fingerprint values serialize");
    base16ct::lower::encode_string(&Sha256::digest(bytes))
}

/// Bind a suggestion to the exact task snapshot used by the matcher/model.
pub fn todo_resolution_fingerprint(task: &TodoRecord) -> String {
    let identity = (
        task.id.clone(),
        task.account_id.as_str(),
        task.thread_id.as_ref().map(ThreadId::as_str),
        task.source_message_id.as_ref().map(MessageId::as_str),
        task.updated_at.timestamp(),
        task.kind.clone(),
        task.verb.clone(),
        task.title.clone(),
        task.counterparty.clone(),
        task.amount_minor,
        task.currency.clone(),
    );
    let dates = (
        task.due_at.map(|date| date.timestamp()),
        task.due_words.clone(),
        task.act_by_at.map(|date| date.timestamp()),
        task.surface_at.map(|date| date.timestamp()),
        task.scheduled_for.map(|date| date.timestamp()),
        task.relevant_until.map(|date| date.timestamp()),
        task.field_sources.clone(),
        task.state.as_str().to_string(),
        i64::from(task.user_edited),
        task.origin.clone(),
    );
    fingerprint_values(identity, dates)
}

fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TodoResolutionSuggestion, sqlx::Error> {
    let action: String = row.try_get("action")?;
    let status: String = row.try_get("status")?;
    Ok(TodoResolutionSuggestion {
        id: row.try_get("id")?,
        todo_id: row.try_get("todo_id")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        source_message_id: decode_id(&row.try_get::<String, _>("source_message_id")?)?,
        source_thread_id: decode_id(&row.try_get::<String, _>("source_thread_id")?)?,
        source_date: crate::decode_timestamp(row.try_get("source_date")?)?,
        source_quote: row.try_get("source_quote")?,
        quote_location: row.try_get("quote_location")?,
        source_method: row.try_get("source_method")?,
        action: match action.as_str() {
            "complete" => TodoResolutionAction::Complete,
            "revise_date" => TodoResolutionAction::ReviseDate,
            _ => {
                return Err(sqlx::Error::Protocol(
                    "invalid todo resolution action".into(),
                ))
            }
        },
        proposed_due_at: crate::decode_optional_timestamp(row.try_get("proposed_due_at")?)?,
        proposed_due_words: row.try_get("proposed_due_words")?,
        task_updated_at: crate::decode_timestamp(row.try_get("task_updated_at")?)?,
        task_fingerprint: row.try_get("task_fingerprint")?,
        status: match status.as_str() {
            "pending" => TodoResolutionStatus::Pending,
            "accepted" => TodoResolutionStatus::Accepted,
            "dismissed" => TodoResolutionStatus::Dismissed,
            "undone" => TodoResolutionStatus::Undone,
            "superseded" => TodoResolutionStatus::Superseded,
            _ => {
                return Err(sqlx::Error::Protocol(
                    "invalid todo resolution status".into(),
                ))
            }
        },
        snapshot_json: row.try_get("snapshot_json")?,
        applied_updated_at: crate::decode_optional_timestamp(row.try_get("applied_updated_at")?)?,
        applied_fingerprint: row.try_get("applied_fingerprint")?,
        created_at: crate::decode_timestamp(row.try_get("created_at")?)?,
    })
}

impl Store {
    pub async fn validate_todo_resolution_task(
        &self,
        suggestion: &TodoResolutionSuggestion,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            "SELECT t.*, s.task_fingerprint FROM todos t
             JOIN todo_resolution_suggestions s ON s.todo_id = t.id
             WHERE t.id = ?1 AND t.account_id = ?2 AND s.id = ?3 AND t.state = 'open'",
        )
        .bind(&suggestion.todo_id)
        .bind(suggestion.account_id.as_str())
        .bind(&suggestion.id)
        .fetch_optional(self.reader())
        .await?;
        let Some(row) = row else { return Ok(false) };
        Ok(fingerprint(&row)? == suggestion.task_fingerprint)
    }

    /// A later, explicit contradiction supersedes any pending offer and
    /// advances the replay watermark even though it creates no action offer.
    pub async fn record_todo_resolution_contradiction(
        &self,
        todo_id: &str,
        account_id: &AccountId,
        source_message_id: &MessageId,
        source_date: DateTime<Utc>,
        task_fingerprint: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let task =
            sqlx::query("SELECT * FROM todos WHERE id = ?1 AND account_id = ?2 AND state = 'open'")
                .bind(todo_id)
                .bind(account_id.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        let source_exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM messages WHERE id = ?1 AND account_id = ?2 AND date = ?3",
        )
        .bind(source_message_id.as_str())
        .bind(account_id.as_str())
        .bind(source_date.timestamp())
        .fetch_optional(&mut *tx)
        .await?;
        if task.as_ref().map(fingerprint).transpose()?.as_deref() != Some(task_fingerprint)
            || source_exists.is_none()
        {
            tx.rollback().await?;
            return Ok(false);
        }
        let watermark: Option<(i64, String)> = sqlx::query_as(
            "SELECT source_date, source_message_id FROM todo_resolution_evidence
             WHERE todo_id = ? ORDER BY source_date DESC, source_message_id DESC LIMIT 1",
        )
        .bind(todo_id)
        .fetch_optional(&mut *tx)
        .await?;
        let incoming = (source_date.timestamp(), source_message_id.as_str());
        if watermark.is_some_and(|current| incoming <= current) {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query("UPDATE todo_resolution_suggestions SET status = 'superseded' WHERE todo_id = ? AND status = 'pending'")
            .bind(todo_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO todo_resolution_evidence
             (todo_id, account_id, source_message_id, source_date, outcome)
             VALUES (?1, ?2, ?3, ?4, 'contradiction')",
        )
        .bind(todo_id)
        .bind(account_id.as_str())
        .bind(source_message_id.as_str())
        .bind(source_date.timestamp())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Retain only related, newer evidence as the pending suggestion.
    /// Ordering by `(source_date, message_id)` makes replay deterministic.
    pub async fn offer_todo_resolution(
        &self,
        suggestion: &NewTodoResolutionSuggestion,
    ) -> Result<bool, sqlx::Error> {
        let quote = suggestion.source_quote.trim();
        if quote.is_empty() || quote.chars().count() > 500 {
            return Ok(false);
        }
        let mut tx = self.writer().begin().await?;
        let current = sqlx::query(
            "SELECT t.*, m.date AS resolution_source_date,
                    m.thread_id AS resolution_source_thread_id FROM todos t
             JOIN messages m ON m.id = ?2 AND m.account_id = t.account_id
             WHERE t.id = ?1 AND t.account_id = ?3 AND t.state = 'open'",
        )
        .bind(&suggestion.todo_id)
        .bind(suggestion.source_message_id.as_str())
        .bind(suggestion.account_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(current) = current else {
            tx.rollback().await?;
            return Ok(false);
        };
        if current.try_get::<i64, _>("resolution_source_date")?
            != suggestion.source_date.timestamp()
            || current.try_get::<String, _>("resolution_source_thread_id")?
                != suggestion.source_thread_id.as_str()
        {
            tx.rollback().await?;
            return Ok(false);
        }
        let stored_updated_at = current.try_get::<i64, _>("updated_at")?;
        if stored_updated_at != suggestion.task_updated_at.timestamp() {
            tx.rollback().await?;
            return Ok(false);
        }
        let current_fingerprint = fingerprint(&current)?;
        if current_fingerprint != suggestion.task_fingerprint {
            tx.rollback().await?;
            return Ok(false);
        }
        let watermark: Option<(i64, String)> = sqlx::query_as(
            "SELECT source_date, source_message_id FROM todo_resolution_evidence
             WHERE todo_id = ? ORDER BY source_date DESC, source_message_id DESC LIMIT 1",
        )
        .bind(&suggestion.todo_id)
        .fetch_optional(&mut *tx)
        .await?;
        let incoming = (
            suggestion.source_date.timestamp(),
            suggestion.source_message_id.as_str(),
        );
        if watermark
            .as_ref()
            .is_some_and(|current| &incoming <= current)
        {
            tx.rollback().await?;
            return Ok(false);
        }
        if watermark.is_some() {
            sqlx::query("UPDATE todo_resolution_suggestions SET status = 'superseded' WHERE todo_id = ? AND status = 'pending'")
                .bind(&suggestion.todo_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            "INSERT OR IGNORE INTO todo_resolution_evidence
             (todo_id, account_id, source_message_id, source_date, outcome)
             VALUES (?1, ?2, ?3, ?4, 'offer')",
        )
        .bind(&suggestion.todo_id)
        .bind(suggestion.account_id.as_str())
        .bind(suggestion.source_message_id.as_str())
        .bind(suggestion.source_date.timestamp())
        .execute(&mut *tx)
        .await?;
        let result = sqlx::query(
            "INSERT OR IGNORE INTO todo_resolution_suggestions
             (id, todo_id, account_id, source_message_id, source_date, source_quote,
              source_thread_id, quote_location, source_method, action, proposed_due_at, proposed_due_words,
              task_updated_at, task_fingerprint, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'pending', ?15)",
        )
        .bind(&suggestion.id)
        .bind(&suggestion.todo_id)
        .bind(suggestion.account_id.as_str())
        .bind(suggestion.source_message_id.as_str())
        .bind(suggestion.source_date.timestamp())
        .bind(quote)
        .bind(suggestion.source_thread_id.as_str())
        .bind(&suggestion.quote_location)
        .bind(&suggestion.source_method)
        .bind(suggestion.action.as_str())
        .bind(suggestion.proposed_due_at.map(|at| at.timestamp()))
        .bind(&suggestion.proposed_due_words)
        .bind(suggestion.task_updated_at.timestamp())
        .bind(&suggestion.task_fingerprint)
        .bind(suggestion.created_at.timestamp())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn pending_todo_resolution(
        &self,
        todo_id: &str,
        account_id: &AccountId,
    ) -> Result<Option<TodoResolutionSuggestion>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT * FROM todo_resolution_suggestions
             WHERE todo_id = ? AND account_id = ? AND status = 'pending'",
        )
        .bind(todo_id)
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.as_ref().map(from_row).transpose()
    }

    pub async fn latest_todo_resolution(
        &self,
        todo_id: &str,
        account_id: &AccountId,
    ) -> Result<Option<TodoResolutionSuggestion>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT s.* FROM todo_resolution_suggestions s
             WHERE s.todo_id = ?1 AND s.account_id = ?2
               AND NOT EXISTS (
                 SELECT 1 FROM todo_resolution_evidence e
                 WHERE e.todo_id = s.todo_id AND e.outcome = 'contradiction'
                   AND (e.source_date > s.source_date
                        OR (e.source_date = s.source_date AND e.source_message_id > s.source_message_id))
               )
             ORDER BY s.source_date DESC, s.source_message_id DESC LIMIT 1",
        )
        .bind(todo_id)
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.as_ref().map(from_row).transpose()
    }

    pub async fn dismiss_todo_resolution(
        &self,
        suggestion_id: &str,
        todo_id: &str,
        account_id: &AccountId,
        _now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todo_resolution_suggestions SET status = 'dismissed'
             WHERE id = ? AND todo_id = ? AND account_id = ? AND status = 'pending'
               AND EXISTS (SELECT 1 FROM todos WHERE id = ? AND account_id = ? AND state = 'open')",
        )
        .bind(suggestion_id)
        .bind(todo_id)
        .bind(account_id.as_str())
        .bind(todo_id)
        .bind(account_id.as_str())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Apply exactly the pending suggestion if the task and evidence still
    /// match what was previewed. The row and offer update share one SQLite
    /// transaction; no user fields outside the proposed dates are written.
    pub async fn accept_todo_resolution(
        &self,
        suggestion_id: &str,
        todo_id: &str,
        account_id: &AccountId,
        dates: Option<&TodoResolutionDates>,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let suggestion = sqlx::query(
            "SELECT * FROM todo_resolution_suggestions
             WHERE id = ? AND todo_id = ? AND account_id = ? AND status = 'pending'",
        )
        .bind(suggestion_id)
        .bind(todo_id)
        .bind(account_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(suggestion) = suggestion else {
            tx.rollback().await?;
            return Ok(false);
        };
        let suggestion = from_row(&suggestion)?;
        let task_sql = format!(
            "SELECT {} FROM todos WHERE id = ? AND account_id = ? AND state = 'open'",
            crate::todos::COLUMNS
        );
        let task = sqlx::query(sqlx::AssertSqlSafe(task_sql))
            .bind(todo_id)
            .bind(account_id.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        let Some(task) = task else {
            tx.rollback().await?;
            return Ok(false);
        };
        if task.try_get::<i64, _>("updated_at")? != suggestion.task_updated_at.timestamp()
            || fingerprint(&task)? != suggestion.task_fingerprint
        {
            tx.rollback().await?;
            return Ok(false);
        }
        let source_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM messages WHERE id = ? AND account_id = ?)",
        )
        .bind(suggestion.source_message_id.as_str())
        .bind(account_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if source_exists != 1 {
            tx.rollback().await?;
            return Ok(false);
        }
        let valid_dates = match (suggestion.action, dates) {
            (TodoResolutionAction::Complete, None) => true,
            (TodoResolutionAction::ReviseDate, Some(dates)) => {
                suggestion.proposed_due_at == Some(dates.due_at)
                    && suggestion.proposed_due_words.as_deref() == Some(dates.due_words.as_str())
            }
            _ => false,
        };
        if !valid_dates {
            tx.rollback().await?;
            return Ok(false);
        }
        let snapshot = snapshot_from_row(&task)?;
        let snapshot_json =
            serde_json::to_string(&snapshot).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        match dates {
            Some(dates) => {
                sqlx::query(
                    "UPDATE todos SET due_at = ?3, due_words = ?4, act_by_at = ?5,
                     surface_at = ?6, relevant_until = ?7, window_source = 'table',
                     field_sources = ?8, user_edited = 1, updated_at = ?9
                     WHERE id = ?1 AND account_id = ?2 AND state = 'open'",
                )
                .bind(todo_id)
                .bind(account_id.as_str())
                .bind(dates.due_at.timestamp())
                .bind(&dates.due_words)
                .bind(dates.act_by_at.map(|at| at.timestamp()))
                .bind(dates.surface_at.map(|at| at.timestamp()))
                .bind(dates.relevant_until.map(|at| at.timestamp()))
                .bind(&dates.field_sources)
                .bind(now.timestamp())
                .execute(&mut *tx)
                .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE todos SET state = 'done', done_at = ?3, user_edited = 1,
                     updated_at = ?3 WHERE id = ?1 AND account_id = ?2 AND state = 'open'",
                )
                .bind(todo_id)
                .bind(account_id.as_str())
                .bind(now.timestamp())
                .execute(&mut *tx)
                .await?;
            }
        }
        let updated = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {} FROM todos WHERE id = ? AND account_id = ?",
            crate::todos::COLUMNS
        )))
        .bind(todo_id)
        .bind(account_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let applied_fingerprint = fingerprint(&updated)?;
        sqlx::query(
            "UPDATE todo_resolution_suggestions SET status = 'accepted', snapshot_json = ?2,
             applied_updated_at = ?3, applied_fingerprint = ?4
             WHERE id = ?1 AND status = 'pending'",
        )
        .bind(suggestion_id)
        .bind(snapshot_json)
        .bind(now.timestamp())
        .bind(applied_fingerprint)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Restore the exact pre-accept fields only while the task still has the
    /// fingerprint written by this acceptance.
    pub async fn undo_todo_resolution(
        &self,
        suggestion_id: &str,
        todo_id: &str,
        account_id: &AccountId,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let row = sqlx::query(
            "SELECT * FROM todo_resolution_suggestions
             WHERE id = ? AND todo_id = ? AND account_id = ? AND status = 'accepted'",
        )
        .bind(suggestion_id)
        .bind(todo_id)
        .bind(account_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.rollback().await?;
            return Ok(false);
        };
        let suggestion = from_row(&row)?;
        let task_sql = format!(
            "SELECT {} FROM todos WHERE id = ? AND account_id = ?",
            crate::todos::COLUMNS
        );
        let task = sqlx::query(sqlx::AssertSqlSafe(task_sql))
            .bind(todo_id)
            .bind(account_id.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        let Some(task) = task else {
            tx.rollback().await?;
            return Ok(false);
        };
        if suggestion.applied_fingerprint.as_deref() != Some(fingerprint(&task)?.as_str()) {
            tx.rollback().await?;
            return Ok(false);
        }
        let Some(snapshot_json) = suggestion.snapshot_json else {
            tx.rollback().await?;
            return Ok(false);
        };
        let snapshot: TodoResolutionSnapshot =
            serde_json::from_str(&snapshot_json).map_err(sqlx::Error::decode)?;
        sqlx::query(
            "UPDATE todos SET state = ?3, done_at = ?4, expired_at = ?5,
             dismissed_at = ?6, due_at = ?7, due_words = ?8, act_by_at = ?9,
             surface_at = ?10, relevant_until = ?11, window_source = ?12,
             field_sources = ?13, user_edited = ?14, updated_at = ?15
             WHERE id = ?1 AND account_id = ?2",
        )
        .bind(todo_id)
        .bind(account_id.as_str())
        .bind(snapshot.state.as_str())
        .bind(snapshot.done_at.map(|at| at.timestamp()))
        .bind(snapshot.expired_at.map(|at| at.timestamp()))
        .bind(snapshot.dismissed_at.map(|at| at.timestamp()))
        .bind(snapshot.due_at.map(|at| at.timestamp()))
        .bind(snapshot.due_words)
        .bind(snapshot.act_by_at.map(|at| at.timestamp()))
        .bind(snapshot.surface_at.map(|at| at.timestamp()))
        .bind(snapshot.relevant_until.map(|at| at.timestamp()))
        .bind(snapshot.window_source)
        .bind(snapshot.field_sources)
        .bind(snapshot.user_edited)
        .bind(now.timestamp())
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE todo_resolution_suggestions SET status = 'undone' WHERE id = ? AND status = 'accepted'")
            .bind(suggestion_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn validate_resolution_source(
        &self,
        suggestion: &TodoResolutionSuggestion,
    ) -> Result<bool, sqlx::Error> {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM messages
             WHERE id = ?1 AND account_id = ?2 AND date = ?3 AND thread_id = ?4)",
        )
        .bind(suggestion.source_message_id.as_str())
        .bind(suggestion.account_id.as_str())
        .bind(suggestion.source_date.timestamp())
        .bind(suggestion.source_thread_id.as_str())
        .fetch_one(self.reader())
        .await?;
        Ok(exists == 1)
    }

    pub async fn validate_todo_resolution_undo(
        &self,
        suggestion: &TodoResolutionSuggestion,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT * FROM todos WHERE id = ?1 AND account_id = ?2")
            .bind(&suggestion.todo_id)
            .bind(suggestion.account_id.as_str())
            .fetch_optional(self.reader())
            .await?;
        let Some(row) = row else { return Ok(false) };
        Ok(suggestion.status == TodoResolutionStatus::Accepted
            && suggestion.applied_fingerprint.as_deref() == Some(fingerprint(&row)?.as_str()))
    }

    pub async fn todo_for_resolution(
        &self,
        todo_id: &str,
        account_id: &AccountId,
    ) -> Result<Option<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM todos WHERE id = ? AND account_id = ?",
            crate::todos::COLUMNS
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(todo_id)
            .bind(account_id.as_str())
            .fetch_optional(self.reader())
            .await?;
        row.as_ref().map(crate::todos::row_to_todo).transpose()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoResolutionSnapshot {
    pub due_at: Option<DateTime<Utc>>,
    pub due_words: Option<String>,
    pub act_by_at: Option<DateTime<Utc>>,
    pub surface_at: Option<DateTime<Utc>>,
    pub relevant_until: Option<DateTime<Utc>>,
    pub window_source: Option<String>,
    pub field_sources: String,
    pub state: TodoState,
    pub done_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
    pub user_edited: bool,
    pub updated_at: DateTime<Utc>,
}

fn snapshot_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TodoResolutionSnapshot, sqlx::Error> {
    let state: String = row.try_get("state")?;
    Ok(TodoResolutionSnapshot {
        due_at: crate::decode_optional_timestamp(row.try_get("due_at")?)?,
        due_words: row.try_get("due_words")?,
        act_by_at: crate::decode_optional_timestamp(row.try_get("act_by_at")?)?,
        surface_at: crate::decode_optional_timestamp(row.try_get("surface_at")?)?,
        relevant_until: crate::decode_optional_timestamp(row.try_get("relevant_until")?)?,
        window_source: row.try_get("window_source")?,
        field_sources: row.try_get("field_sources")?,
        state: TodoState::parse(&state)
            .ok_or_else(|| sqlx::Error::Protocol("invalid to-do state".into()))?,
        done_at: crate::decode_optional_timestamp(row.try_get("done_at")?)?,
        expired_at: crate::decode_optional_timestamp(row.try_get("expired_at")?)?,
        dismissed_at: crate::decode_optional_timestamp(row.try_get("dismissed_at")?)?,
        user_edited: row.try_get::<i64, _>("user_edited")? != 0,
        updated_at: crate::decode_timestamp(row.try_get("updated_at")?)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use crate::{Store, TodoState};
    use chrono::Duration;
    use mxr_core::types::{Address, MessageDirection};

    struct Fx {
        store: Store,
        account: AccountId,
        thread: mxr_core::id::ThreadId,
        task: TodoRecord,
        fingerprint: String,
    }

    impl Fx {
        async fn new() -> Self {
            let store = Store::in_memory().await.unwrap();
            let account = test_account();
            store.insert_account(&account).await.unwrap();
            let now = Utc::now() - Duration::days(3);
            let mut source = TestEnvelopeBuilder::new()
                .account_id(account.id.clone())
                .build();
            source.date = now;
            source.subject = "Council tax payment due".into();
            source.from = Address {
                name: Some("Camden Council".into()),
                email: "bills@camden.gov.uk".into(),
            };
            let thread = source.thread_id.clone();
            store
                .upsert_envelope_with_direction(&source, MessageDirection::Inbound)
                .await
                .unwrap();
            let task = TodoRecord {
                id: "todo_resolution_test".into(),
                account_id: account.id.clone(),
                thread_id: Some(thread.clone()),
                source_message_id: Some(source.id.clone()),
                source_date: Some(now),
                kind: "bill".into(),
                verb: "pay".into(),
                doc_type: None,
                title: "Pay council tax".into(),
                counterparty: Some("Camden Council".into()),
                sender_domain: Some("camden.gov.uk".into()),
                amount_minor: Some(14200),
                currency: Some("GBP".into()),
                due_at: Some(now + Duration::days(7)),
                due_words: Some("payment due next week".into()),
                act_by_at: Some(now + Duration::days(5)),
                surface_at: Some(now + Duration::days(3)),
                scheduled_for: None,
                action_url: None,
                action_domain: None,
                relevant_until: Some(now + Duration::days(30)),
                window_source: Some("rule".into()),
                state: TodoState::Open,
                expired_at: None,
                expired_at_birth: false,
                catchup: None,
                looks_done_message_id: None,
                looks_done_reason: None,
                origin: "rule".into(),
                reason: "payment due next week".into(),
                field_sources: "{}".into(),
                user_edited: false,
                commitment_id: None,
                rules_version: 1,
                dedup_key: "bill|camden|council-tax".into(),
                surfaced_at: None,
                created_at: now,
                updated_at: now,
                done_at: None,
                dismissed_at: None,
            };
            store.upsert_detected_todo(&task).await.unwrap();
            // Match production scanning: fingerprint the exact persisted row
            // that supplied the matcher with its task snapshot.
            let task = store.get_todo(&task.id).await.unwrap().unwrap();
            let fingerprint = todo_resolution_fingerprint(&task);
            Self {
                store,
                account: account.id,
                thread,
                task,
                fingerprint,
            }
        }

        async fn source(&self, at: DateTime<Utc>) -> MessageId {
            let mut envelope = TestEnvelopeBuilder::new()
                .account_id(self.account.clone())
                .build();
            envelope.date = at;
            envelope.thread_id = self.thread.clone();
            envelope.from = Address {
                name: Some("Camden Council".into()),
                email: "bills@camden.gov.uk".into(),
            };
            envelope.subject = "Payment received".into();
            envelope.provider_id = format!("todo-resolution-source-{}", envelope.id);
            self.store
                .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
                .await
                .unwrap()
        }

        async fn offer(&self, id: MessageId, date: DateTime<Utc>, offer_id: &str) -> bool {
            self.store
                .offer_todo_resolution(&NewTodoResolutionSuggestion {
                    id: offer_id.into(),
                    todo_id: self.task.id.clone(),
                    account_id: self.account.clone(),
                    source_message_id: id,
                    source_thread_id: self.thread.clone(),
                    source_date: date,
                    source_quote: "Payment received".into(),
                    quote_location: "subject".into(),
                    source_method: "rule".into(),
                    action: TodoResolutionAction::Complete,
                    proposed_due_at: None,
                    proposed_due_words: None,
                    task_updated_at: self.task.updated_at,
                    task_fingerprint: self.fingerprint.clone(),
                    created_at: date,
                })
                .await
                .unwrap()
        }
    }

    #[tokio::test]
    async fn newer_evidence_supersedes_and_replay_cannot_restore_old_offer() {
        let fx = Fx::new().await;
        let first_date = fx.task.updated_at + Duration::days(1);
        let first = fx.source(first_date).await;
        assert!(fx.offer(first.clone(), first_date, "offer_first").await);
        let later_date = first_date + Duration::days(1);
        let later = fx.source(later_date).await;
        assert!(fx.offer(later.clone(), later_date, "offer_later").await);
        let current = fx
            .store
            .pending_todo_resolution(&fx.task.id, &fx.account)
            .await
            .unwrap()
            .expect("current offer");
        assert_eq!(current.id, "offer_later");
        assert!(!fx.offer(first, first_date, "offer_replay").await);
    }

    #[tokio::test]
    async fn newer_contradiction_remains_a_watermark_after_dismissal() {
        let fx = Fx::new().await;
        let first_date = fx.task.updated_at + Duration::days(1);
        let first = fx.source(first_date).await;
        assert!(fx.offer(first, first_date, "offer_pending").await);
        assert!(fx
            .store
            .dismiss_todo_resolution(
                "offer_pending",
                &fx.task.id,
                &fx.account,
                first_date + Duration::minutes(1),
            )
            .await
            .unwrap());
        let contradiction_date = first_date + Duration::days(1);
        let contradiction = fx.source(contradiction_date).await;
        assert!(fx
            .store
            .record_todo_resolution_contradiction(
                &fx.task.id,
                &fx.account,
                &contradiction,
                contradiction_date,
                &fx.fingerprint,
            )
            .await
            .unwrap());
        let older = fx.source(first_date + Duration::minutes(2)).await;
        assert!(
            !fx.offer(older, first_date + Duration::minutes(2), "offer_old_replay")
                .await
        );
    }

    #[tokio::test]
    async fn offer_refuses_same_second_task_edit_during_analysis() {
        let fx = Fx::new().await;
        let date = fx.task.updated_at + Duration::days(1);
        let source = fx.source(date).await;
        let mut edited = fx.store.get_todo(&fx.task.id).await.unwrap().unwrap();
        edited.title = "Pay the corrected council tax".into();
        fx.store
            .update_todo_by_user(&edited, fx.task.updated_at)
            .await
            .unwrap();
        let current = fx.store.get_todo(&fx.task.id).await.unwrap().unwrap();
        assert_eq!(current.title, edited.title);
        assert_eq!(
            current.updated_at.timestamp(),
            fx.task.updated_at.timestamp()
        );
        assert!(!fx.offer(source, date, "offer_stale_analysis").await);
    }

    #[tokio::test]
    async fn preview_refuses_a_task_edited_before_review() {
        let fx = Fx::new().await;
        let date = fx.task.updated_at + Duration::days(1);
        let source = fx.source(date).await;
        assert!(fx.offer(source, date, "offer_preview_stale").await);
        let suggestion = fx
            .store
            .pending_todo_resolution(&fx.task.id, &fx.account)
            .await
            .unwrap()
            .unwrap();
        let mut edited = fx.store.get_todo(&fx.task.id).await.unwrap().unwrap();
        edited.title = "Pay the manually corrected council tax".into();
        fx.store
            .update_todo_by_user(&edited, date + Duration::minutes(1))
            .await
            .unwrap();
        assert!(!fx
            .store
            .validate_todo_resolution_task(&suggestion)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn complete_acceptance_and_undo_restore_the_same_task() {
        let fx = Fx::new().await;
        let date = fx.task.updated_at + Duration::days(1);
        let source = fx.source(date).await;
        assert!(fx.offer(source, date, "offer_complete").await);
        assert!(fx
            .store
            .accept_todo_resolution(
                "offer_complete",
                &fx.task.id,
                &fx.account,
                None,
                date + Duration::minutes(1),
            )
            .await
            .unwrap());
        let accepted = fx
            .store
            .latest_todo_resolution(&fx.task.id, &fx.account)
            .await
            .unwrap()
            .unwrap();
        assert!(fx
            .store
            .validate_todo_resolution_undo(&accepted)
            .await
            .unwrap());
        assert_eq!(
            fx.store.get_todo(&fx.task.id).await.unwrap().unwrap().state,
            TodoState::Done
        );
        assert!(fx
            .store
            .undo_todo_resolution(
                "offer_complete",
                &fx.task.id,
                &fx.account,
                date + Duration::minutes(2),
            )
            .await
            .unwrap());
        let restored = fx.store.get_todo(&fx.task.id).await.unwrap().unwrap();
        assert_eq!(restored.state, TodoState::Open);
        assert_eq!(restored.due_at, fx.task.due_at);
        assert_eq!(restored.title, fx.task.title);
    }

    #[tokio::test]
    async fn undo_refuses_to_overwrite_a_later_user_edit() {
        let fx = Fx::new().await;
        let date = fx.task.updated_at + Duration::days(1);
        let source = fx.source(date).await;
        assert!(fx.offer(source, date, "offer_stale").await);
        assert!(fx
            .store
            .accept_todo_resolution(
                "offer_stale",
                &fx.task.id,
                &fx.account,
                None,
                date + Duration::minutes(1),
            )
            .await
            .unwrap());
        let mut edited = fx.store.get_todo(&fx.task.id).await.unwrap().unwrap();
        edited.title = "Pay the revised council tax".into();
        fx.store
            .update_todo_by_user(&edited, date + Duration::minutes(2))
            .await
            .unwrap();
        let accepted = fx
            .store
            .latest_todo_resolution(&fx.task.id, &fx.account)
            .await
            .unwrap()
            .unwrap();
        assert!(!fx
            .store
            .validate_todo_resolution_undo(&accepted)
            .await
            .unwrap());
        assert!(!fx
            .store
            .undo_todo_resolution(
                "offer_stale",
                &fx.task.id,
                &fx.account,
                date + Duration::minutes(3),
            )
            .await
            .unwrap());
        assert_eq!(
            fx.store.get_todo(&fx.task.id).await.unwrap().unwrap().title,
            "Pay the revised council tax"
        );
    }

    #[tokio::test]
    async fn cached_candidates_keep_newest_related_messages_within_limit() {
        let fx = Fx::new().await;
        let first_date = fx.task.source_date.unwrap() + Duration::minutes(1);
        let mut latest = None;
        for minute in 0..45 {
            let id = fx.source(first_date + Duration::minutes(minute)).await;
            latest = Some(id);
        }

        let candidates = fx
            .store
            .list_todo_resolution_candidate_messages(&fx.task.id, 40)
            .await
            .unwrap();
        assert_eq!(candidates.len(), 40);
        assert!(candidates.contains(&latest.unwrap()));
    }
}
