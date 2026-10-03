//! To do rows: one per thing the user has to act on, with a date.
//!
//! The store keeps rows and claims; what a row means (kinds, lead times,
//! windows, the first run) lives in the `mxr-todo` crate, which calls these
//! primitives, as `deliveries` does. Queries are unchecked (`sqlx::query`)
//! so the table needs no offline cache entry.
//!
//! Upserts use `ON CONFLICT(account_id, dedup_key)`, never `INSERT OR
//! REPLACE`, and never clear a claim (`surfaced_at`), an expiry or a user
//! decision.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, in_list, in_list_after_first};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoState {
    Open,
    Done,
    Dismissed,
    Expired,
}

impl TodoState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Done => "done",
            Self::Dismissed => "dismissed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "open" => Self::Open,
            "done" => Self::Done,
            "dismissed" => Self::Dismissed,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

/// Where a row sits in the first run's one-time catch-up batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoCatchup {
    /// In the batch, waiting for keep or let go.
    Pending,
    Kept,
    LetGo,
    /// Found for the batch after it was full; expired with a count.
    Overflow,
}

impl TodoCatchup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Kept => "kept",
            Self::LetGo => "let_go",
            Self::Overflow => "overflow",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "kept" => Self::Kept,
            "let_go" => Self::LetGo,
            "overflow" => Self::Overflow,
            _ => return None,
        })
    }
}

/// A row of the `todos` table. Kinds, verbs and sources are plain strings
/// here; their vocabulary lives in `mxr-todo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoRecord {
    pub id: String,
    pub account_id: AccountId,
    pub thread_id: Option<ThreadId>,
    pub source_message_id: Option<MessageId>,
    pub source_date: Option<DateTime<Utc>>,
    pub kind: String,
    pub verb: String,
    pub doc_type: Option<String>,
    pub title: String,
    pub counterparty: Option<String>,
    pub sender_domain: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub due_words: Option<String>,
    pub act_by_at: Option<DateTime<Utc>>,
    pub surface_at: Option<DateTime<Utc>>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub action_url: Option<String>,
    pub action_domain: Option<String>,
    pub relevant_until: Option<DateTime<Utc>>,
    pub window_source: Option<String>,
    pub state: TodoState,
    pub expired_at: Option<DateTime<Utc>>,
    pub expired_at_birth: bool,
    pub catchup: Option<TodoCatchup>,
    pub looks_done_message_id: Option<MessageId>,
    pub looks_done_reason: Option<String>,
    pub origin: String,
    pub reason: String,
    /// JSON object: field name to its provenance.
    pub field_sources: String,
    pub user_edited: bool,
    pub commitment_id: Option<String>,
    pub rules_version: i64,
    pub dedup_key: String,
    pub surfaced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
    pub dismissed_at: Option<DateTime<Utc>>,
}

impl TodoRecord {
    /// Made or touched by the user: never expires, never rewritten by a
    /// re-run, and survives its email's deletion. The SQL side is
    /// `todo_untouched_sql!` (a schedule alone still takes newer evidence).
    pub fn user_touched(&self) -> bool {
        self.user_edited
            || matches!(self.origin.as_str(), "manual" | "handoff")
            || self.scheduled_for.is_some()
    }
}

/// What an upsert did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TodoUpsert {
    /// A new row, in the state the caller placed it.
    Inserted { id: String, state: TodoState },
    /// An existing row took the newer evidence.
    Updated { id: String, reopened: bool },
    /// An existing row kept what it had: older evidence, or the user
    /// touched it.
    Unchanged,
}

/// An expired row reopens when newer evidence is still in its window,
/// unless the user let it go in the catch-up.
const REOPENS: &str = "todos.state = 'expired' AND excluded.state = 'open'
    AND COALESCE(todos.catchup, '') <> 'let_go'";

pub(crate) const COLUMNS: &str =
    "id, account_id, thread_id, source_message_id, source_date, kind, verb,
    doc_type, title, counterparty, sender_domain, amount_minor, currency, due_at, due_words,
    act_by_at, surface_at, scheduled_for, action_url, action_domain, relevant_until, window_source, state, expired_at, expired_at_birth, catchup,
    looks_done_message_id, looks_done_reason, origin, reason, field_sources, user_edited,
    commitment_id, rules_version, dedup_key, surfaced_at, created_at, updated_at, done_at,
    dismissed_at";

/// Open rows the sweep may expire: detected, untouched, and not a bill or
/// promise, which stay "was due" until the user acts (D117).
const EXPIRABLE: &str = concat!(
    "state = 'open' AND COALESCE(catchup, '') <> 'pending' AND ",
    todo_untouched_sql!(""),
    " AND scheduled_for IS NULL AND kind NOT IN ('bill', 'payment_failed', 'promise')"
);
const UNTOUCHED_EXISTING: &str = todo_untouched_sql!("todos.");

fn ts(value: Option<DateTime<Utc>>) -> Option<i64> {
    value.map(|value| value.timestamp())
}

fn optional_id<T>(value: Option<String>) -> Result<Option<T>, sqlx::Error>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value.as_deref().map(decode_id).transpose()
}

pub(crate) fn row_to_todo(row: &SqliteRow) -> Result<TodoRecord, sqlx::Error> {
    let state: String = row.try_get("state")?;
    let catchup: Option<String> = row.try_get("catchup")?;
    Ok(TodoRecord {
        id: row.try_get("id")?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        thread_id: optional_id(row.try_get("thread_id")?)?,
        source_message_id: optional_id(row.try_get("source_message_id")?)?,
        source_date: decode_optional_timestamp(row.try_get("source_date")?)?,
        kind: row.try_get("kind")?,
        verb: row.try_get("verb")?,
        doc_type: row.try_get("doc_type")?,
        title: row.try_get("title")?,
        counterparty: row.try_get("counterparty")?,
        sender_domain: row.try_get("sender_domain")?,
        amount_minor: row.try_get("amount_minor")?,
        currency: row.try_get("currency")?,
        due_at: decode_optional_timestamp(row.try_get("due_at")?)?,
        due_words: row.try_get("due_words")?,
        act_by_at: decode_optional_timestamp(row.try_get("act_by_at")?)?,
        surface_at: decode_optional_timestamp(row.try_get("surface_at")?)?,
        scheduled_for: decode_optional_timestamp(row.try_get("scheduled_for")?)?,
        action_url: row.try_get("action_url")?,
        action_domain: row.try_get("action_domain")?,
        relevant_until: decode_optional_timestamp(row.try_get("relevant_until")?)?,
        window_source: row.try_get("window_source")?,
        state: TodoState::parse(&state)
            .ok_or_else(|| sqlx::Error::Decode(format!("unknown to-do state {state}").into()))?,
        expired_at: decode_optional_timestamp(row.try_get("expired_at")?)?,
        expired_at_birth: row.try_get::<i64, _>("expired_at_birth")? != 0,
        catchup: catchup.as_deref().and_then(TodoCatchup::parse),
        looks_done_message_id: optional_id(row.try_get("looks_done_message_id")?)?,
        looks_done_reason: row.try_get("looks_done_reason")?,
        origin: row.try_get("origin")?,
        reason: row.try_get("reason")?,
        field_sources: row.try_get("field_sources")?,
        user_edited: row.try_get::<i64, _>("user_edited")? != 0,
        commitment_id: row.try_get("commitment_id")?,
        rules_version: row.try_get("rules_version")?,
        dedup_key: row.try_get("dedup_key")?,
        surfaced_at: decode_optional_timestamp(row.try_get("surfaced_at")?)?,
        created_at: decode_timestamp(row.try_get("created_at")?)?,
        updated_at: decode_timestamp(row.try_get("updated_at")?)?,
        done_at: decode_optional_timestamp(row.try_get("done_at")?)?,
        dismissed_at: decode_optional_timestamp(row.try_get("dismissed_at")?)?,
    })
}

impl super::Store {
    /// Insert a detected row, or give an existing row with the same dedup
    /// key the newer evidence.
    ///
    /// An existing row is rewritten only when this evidence is at least as
    /// new as what it holds and the user hasn't touched it; its claim, its
    /// user decisions and its catch-up place are never cleared. An expired
    /// row whose new evidence is still in its window reopens: a reminder or
    /// final notice for a bill brings it back. A row the user let go in the
    /// catch-up stays let go.
    pub async fn upsert_detected_todo(
        &self,
        record: &TodoRecord,
    ) -> Result<TodoUpsert, sqlx::Error> {
        // Read for the report only: the conflict clause below decides.
        let prior = self
            .get_todo_by_dedup(&record.account_id, &record.dedup_key)
            .await?;
        let sql = format!(
            "INSERT INTO todos ({COLUMNS})
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                     ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, dedup_key) DO UPDATE SET
                thread_id = excluded.thread_id,
                source_message_id = excluded.source_message_id,
                source_date = excluded.source_date,
                kind = excluded.kind,
                verb = excluded.verb,
                doc_type = excluded.doc_type,
                title = excluded.title,
                counterparty = excluded.counterparty,
                sender_domain = excluded.sender_domain,
                amount_minor = excluded.amount_minor,
                currency = excluded.currency,
                due_at = excluded.due_at,
                due_words = excluded.due_words,
                act_by_at = excluded.act_by_at,
                surface_at = excluded.surface_at,
                action_url = excluded.action_url,
                action_domain = excluded.action_domain,
                relevant_until = excluded.relevant_until,
                window_source = excluded.window_source,
                origin = excluded.origin,
                reason = excluded.reason,
                field_sources = excluded.field_sources,
                rules_version = excluded.rules_version,
                state = CASE WHEN {REOPENS} THEN 'open' ELSE todos.state END,
                expired_at = CASE WHEN {REOPENS} THEN NULL ELSE todos.expired_at END,
                expired_at_birth = CASE WHEN {REOPENS} THEN 0 ELSE todos.expired_at_birth END,
                updated_at = excluded.updated_at
             WHERE {UNTOUCHED_EXISTING}
               AND (todos.source_date IS NULL
                    OR excluded.source_date > todos.source_date
                    OR (excluded.source_date = todos.source_date
                        AND excluded.source_message_id IS todos.source_message_id))
               AND (todos.source_message_id IS NOT excluded.source_message_id
                    OR todos.due_at IS NOT excluded.due_at
                    OR todos.rules_version <> excluded.rules_version
                    OR todos.field_sources <> excluded.field_sources)
             RETURNING id, state"
        );
        let row = bind_record(sqlx::query(sqlx::AssertSqlSafe(sql)), record)
            .fetch_optional(self.writer())
            .await?;
        let Some(row) = row else {
            return Ok(TodoUpsert::Unchanged);
        };
        let id: String = row.try_get("id")?;
        let state =
            TodoState::parse(&row.try_get::<String, _>("state")?).unwrap_or(TodoState::Open);
        Ok(match prior {
            None => TodoUpsert::Inserted { id, state },
            Some(prior) => TodoUpsert::Updated {
                id,
                reopened: prior.state == TodoState::Expired && state == TodoState::Open,
            },
        })
    }

    /// Insert a row the user made. Fails on a duplicate dedup key so a
    /// second `add` of the same thing reports it instead of overwriting.
    pub async fn insert_todo(&self, record: &TodoRecord) -> Result<(), sqlx::Error> {
        let sql = format!(
            "INSERT INTO todos ({COLUMNS})
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                     ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        );
        bind_record(sqlx::query(sqlx::AssertSqlSafe(sql)), record)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn get_todo(&self, id: &str) -> Result<Option<TodoRecord>, sqlx::Error> {
        let sql = format!("SELECT {COLUMNS} FROM todos WHERE id = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(row_to_todo)
            .transpose()
    }

    pub async fn get_todo_by_dedup(
        &self,
        account_id: &AccountId,
        dedup_key: &str,
    ) -> Result<Option<TodoRecord>, sqlx::Error> {
        let sql = format!("SELECT {COLUMNS} FROM todos WHERE account_id = ? AND dedup_key = ?");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(dedup_key)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(row_to_todo)
            .transpose()
    }

    /// Rows by id, in no particular order; unknown ids are skipped.
    pub async fn get_todos(&self, ids: &[String]) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT {COLUMNS} FROM todos WHERE id IN ({})",
                in_list(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id);
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push(row_to_todo(&row)?);
            }
        }
        Ok(out)
    }

    /// Ids that start with `prefix`, at most `limit`, so a client can take
    /// the short form it printed.
    pub async fn find_todo_ids_by_prefix(
        &self,
        prefix: &str,
        limit: u32,
    ) -> Result<Vec<String>, sqlx::Error> {
        // `substr` rather than LIKE keeps `%` and `_` in the prefix literal.
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM todos WHERE substr(id, 1, length(?1)) = ?1 ORDER BY id LIMIT ?2",
        )
        .bind(prefix)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await
    }

    /// What the runway reads: every open row, and rows done since `done_since`.
    pub async fn list_runway_todos(
        &self,
        account_id: Option<&AccountId>,
        done_since: DateTime<Utc>,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos
             WHERE (?1 IS NULL OR account_id = ?1)
               AND (state = 'open' OR (state = 'done' AND done_at >= ?2))"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.map(AccountId::as_str))
            .bind(done_since.timestamp())
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    /// Rows in one state, newest change first.
    pub async fn list_todos_in_state(
        &self,
        account_id: Option<&AccountId>,
        state: TodoState,
        limit: u32,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND state = ?2
             ORDER BY COALESCE(expired_at, done_at, dismissed_at, updated_at) DESC, id
             LIMIT ?3"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.map(AccountId::as_str))
            .bind(state.as_str())
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    pub async fn count_todos_in_state(
        &self,
        account_id: Option<&AccountId>,
        state: TodoState,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM todos WHERE (?1 IS NULL OR account_id = ?1) AND state = ?2",
        )
        .bind(account_id.map(AccountId::as_str))
        .bind(state.as_str())
        .fetch_one(self.reader())
        .await
    }

    /// The catch-up batch: pending rows, by act-by then newest evidence.
    pub async fn list_catchup_todos(
        &self,
        account_id: Option<&AccountId>,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND state = 'open' AND catchup = 'pending'
             ORDER BY act_by_at IS NULL, act_by_at, source_date DESC, id"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.map(AccountId::as_str))
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    pub async fn count_catchup_todos(
        &self,
        account_id: Option<&AccountId>,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND state = 'open' AND catchup = 'pending'",
        )
        .bind(account_id.map(AccountId::as_str))
        .fetch_one(self.reader())
        .await
    }

    pub async fn count_catchup_overflow(
        &self,
        account_id: Option<&AccountId>,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND catchup = 'overflow' AND state = 'expired'",
        )
        .bind(account_id.map(AccountId::as_str))
        .fetch_one(self.reader())
        .await
    }

    /// Keeps the catch-up batch at `max` rows across every account, since
    /// the batch is shown as one list: by act-by, then the newest evidence.
    /// The rest expire as overflow. Returns how many went.
    pub async fn trim_todo_catchup(
        &self,
        max: u32,
        now: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todos
             SET state = 'expired', catchup = 'overflow', expired_at = ?2,
                 expired_at_birth = 1, updated_at = ?2
             WHERE id IN (
                 SELECT id FROM todos
                 WHERE state = 'open' AND catchup = 'pending'
                 ORDER BY act_by_at IS NULL, act_by_at, source_date DESC, id
                 LIMIT -1 OFFSET ?1)",
        )
        .bind(i64::from(max))
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }

    /// Rows expired by the sweep (not at birth) since `since`.
    pub async fn count_todos_expired_since(
        &self,
        account_id: Option<&AccountId>,
        since: DateTime<Utc>,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND state = 'expired'
               AND expired_at_birth = 0 AND expired_at > ?2
               AND COALESCE(catchup, '') NOT IN ('let_go', 'overflow')",
        )
        .bind(account_id.map(AccountId::as_str))
        .bind(since.timestamp())
        .fetch_one(self.reader())
        .await
    }

    /// Moves rows to `state`, only from the states in `from`. Returns the
    /// ids that changed. `done` and `dismissed` stamp their time; `open`
    /// clears every end and marks the row as the user's, so it never
    /// expires again; `expired` is a user's let go and stamps `expired_at`.
    pub async fn set_todos_state(
        &self,
        ids: &[String],
        from: &[TodoState],
        to: TodoState,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        let set = match to {
            TodoState::Done => "state = 'done', done_at = ?1",
            TodoState::Dismissed => "state = 'dismissed', dismissed_at = ?1",
            TodoState::Expired => "state = 'expired', expired_at = ?1, expired_at_birth = 0",
            TodoState::Open => {
                "state = 'open', done_at = NULL, dismissed_at = NULL, expired_at = NULL,
                 expired_at_birth = 0, user_edited = 1,
                 catchup = CASE WHEN catchup IN ('let_go', 'overflow') THEN 'kept' ELSE catchup END"
            }
        };
        let from_list = from
            .iter()
            .map(|state| format!("'{}'", state.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let mut changed = Vec::new();
        for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "UPDATE todos SET {set}, updated_at = ?1
                 WHERE id IN ({}) AND state IN ({from_list})
                 RETURNING id",
                in_list_after_first(chunk.len())
            );
            let mut query =
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(now.timestamp());
            for id in chunk {
                query = query.bind(id);
            }
            changed.extend(query.fetch_all(self.writer()).await?);
        }
        Ok(changed)
    }

    /// Keeps catch-up rows: out of the batch, into the runway, and the
    /// user's from now on.
    pub async fn keep_catchup_todos(
        &self,
        ids: &[String],
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut changed = Vec::new();
        for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "UPDATE todos SET catchup = 'kept', user_edited = 1, updated_at = ?1
                 WHERE id IN ({}) AND state = 'open' AND catchup = 'pending'
                 RETURNING id",
                in_list_after_first(chunk.len())
            );
            let mut query =
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(now.timestamp());
            for id in chunk {
                query = query.bind(id);
            }
            changed.extend(query.fetch_all(self.writer()).await?);
        }
        Ok(changed)
    }

    /// Lets go of catch-up rows: expired, and marked as the user's choice so
    /// newer evidence doesn't bring them back and they never count as
    /// "expired since you last looked".
    pub async fn let_go_catchup_todos(
        &self,
        ids: &[String],
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut changed = Vec::new();
        for chunk in ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "UPDATE todos SET state = 'expired', catchup = 'let_go', expired_at = ?1,
                        expired_at_birth = 0, updated_at = ?1
                 WHERE id IN ({}) AND state = 'open' AND catchup = 'pending'
                 RETURNING id",
                in_list_after_first(chunk.len())
            );
            let mut query =
                sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql)).bind(now.timestamp());
            for id in chunk {
                query = query.bind(id);
            }
            changed.extend(query.fetch_all(self.writer()).await?);
        }
        Ok(changed)
    }

    /// What the first run found already over, by kind (the catch-up's
    /// overflow is counted separately).
    pub async fn count_todos_expired_at_birth_by_kind(
        &self,
        account_id: Option<&AccountId>,
    ) -> Result<Vec<(String, i64)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT kind, COUNT(*) AS n FROM todos
             WHERE (?1 IS NULL OR account_id = ?1) AND state = 'expired'
               AND expired_at_birth = 1 AND COALESCE(catchup, '') <> 'overflow'
             GROUP BY kind ORDER BY n DESC, kind",
        )
        .bind(account_id.map(AccountId::as_str))
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| Ok((row.try_get("kind")?, row.try_get("n")?)))
            .collect()
    }

    /// Writes the user's edit of a row: every user-editable field, the
    /// provenance, and the mark that it is the user's now.
    pub async fn update_todo_by_user(
        &self,
        record: &TodoRecord,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todos SET
                kind = ?2, verb = ?3, title = ?4, counterparty = ?5, amount_minor = ?6,
                currency = ?7, due_at = ?8, due_words = ?9, act_by_at = ?10, surface_at = ?11,
                relevant_until = ?12, window_source = ?13, field_sources = ?14,
                scheduled_for = ?15, user_edited = 1, updated_at = ?16,
                surfaced_at = CASE WHEN surface_at IS ?11 THEN surfaced_at ELSE NULL END
             WHERE id = ?1",
        )
        .bind(&record.id)
        .bind(&record.kind)
        .bind(&record.verb)
        .bind(&record.title)
        .bind(&record.counterparty)
        .bind(record.amount_minor)
        .bind(&record.currency)
        .bind(ts(record.due_at))
        .bind(&record.due_words)
        .bind(ts(record.act_by_at))
        .bind(ts(record.surface_at))
        .bind(ts(record.relevant_until))
        .bind(&record.window_source)
        .bind(&record.field_sources)
        .bind(ts(record.scheduled_for))
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Records that a later message looks like the confirmation, without
    /// closing the row. The first match stands.
    pub async fn set_todo_looks_done(
        &self,
        id: &str,
        message_id: &MessageId,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todos SET looks_done_message_id = ?2, looks_done_reason = ?3, updated_at = ?4
             WHERE id = ?1 AND state = 'open' AND looks_done_message_id IS NULL",
        )
        .bind(id)
        .bind(message_id.as_str())
        .bind(reason)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Open rows from one sender domain, for confirmation matching.
    pub async fn list_open_todos_for_domain(
        &self,
        account_id: &AccountId,
        sender_domain: &str,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos
             WHERE account_id = ? AND sender_domain = ? AND state = 'open'"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(sender_domain)
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    /// The sweep: expires open detected rows past their window. Bills and
    /// promises stay "was due" until the user acts, and rows the user made
    /// or touched never expire.
    pub async fn expire_lapsed_todos(&self, now: DateTime<Utc>) -> Result<u64, sqlx::Error> {
        let sql = format!(
            "UPDATE todos SET state = 'expired', expired_at = ?1, updated_at = ?1
             WHERE {EXPIRABLE} AND relevant_until IS NOT NULL AND relevant_until < ?1"
        );
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now.timestamp())
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }

    /// Claims rows whose surface time has come: sets `surfaced_at` in the
    /// statement that finds them, so each is announced once, even across
    /// restarts. The user's own date wins over the computed one.
    pub async fn claim_surfaced_todos(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "UPDATE todos SET surfaced_at = ?1
             WHERE state = 'open' AND surfaced_at IS NULL
               AND COALESCE(catchup, '') <> 'pending'
               AND COALESCE(scheduled_for, surface_at) <= ?1
             RETURNING id",
        )
        .bind(now.timestamp())
        .fetch_all(self.writer())
        .await
    }

    /// RSVP rows whose invite the user has answered are done: answering is
    /// the action, not a guess about it.
    pub async fn complete_answered_rsvp_todos(
        &self,
        now: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todos SET state = 'done', done_at = ?1, updated_at = ?1
             WHERE kind = 'rsvp' AND state = 'open' AND source_message_id IS NOT NULL
               AND EXISTS (
                   SELECT 1 FROM calendar_invites invite
                   WHERE invite.message_id = todos.source_message_id
                     AND UPPER(COALESCE(invite.current_partstat, ''))
                         IN ('ACCEPTED', 'DECLINED', 'TENTATIVE'))",
        )
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn schedule_todo(
        &self,
        id: &str,
        at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE todos SET scheduled_for = ?2, surfaced_at = NULL, updated_at = ?3
             WHERE id = ?1 AND state = 'open'",
        )
        .bind(id)
        .bind(ts(at))
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn bind_record<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments>,
    record: &'q TodoRecord,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments> {
    query
        .bind(&record.id)
        .bind(record.account_id.as_str())
        .bind(record.thread_id.as_ref().map(ThreadId::as_str))
        .bind(record.source_message_id.as_ref().map(MessageId::as_str))
        .bind(ts(record.source_date))
        .bind(&record.kind)
        .bind(&record.verb)
        .bind(&record.doc_type)
        .bind(&record.title)
        .bind(&record.counterparty)
        .bind(&record.sender_domain)
        .bind(record.amount_minor)
        .bind(&record.currency)
        .bind(ts(record.due_at))
        .bind(&record.due_words)
        .bind(ts(record.act_by_at))
        .bind(ts(record.surface_at))
        .bind(ts(record.scheduled_for))
        .bind(&record.action_url)
        .bind(&record.action_domain)
        .bind(ts(record.relevant_until))
        .bind(&record.window_source)
        .bind(record.state.as_str())
        .bind(ts(record.expired_at))
        .bind(i64::from(record.expired_at_birth))
        .bind(record.catchup.map(TodoCatchup::as_str))
        .bind(record.looks_done_message_id.as_ref().map(MessageId::as_str))
        .bind(&record.looks_done_reason)
        .bind(&record.origin)
        .bind(&record.reason)
        .bind(&record.field_sources)
        .bind(i64::from(record.user_edited))
        .bind(&record.commitment_id)
        .bind(record.rules_version)
        .bind(&record.dedup_key)
        .bind(ts(record.surfaced_at))
        .bind(record.created_at.timestamp())
        .bind(record.updated_at.timestamp())
        .bind(ts(record.done_at))
        .bind(ts(record.dismissed_at))
}

#[cfg(test)]
mod tests;
