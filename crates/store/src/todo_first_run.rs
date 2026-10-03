//! The first run's view of mail and of what it mirrors: the newest-first
//! scan, the domain check for the pay-link gate, promises you made and
//! invites waiting for an answer. Rows themselves live in `todos.rs`.

use crate::todos::{row_to_todo, TodoRecord, COLUMNS};
use crate::{decode_id, decode_optional_timestamp, decode_timestamp, in_list};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

/// One message as the first run's newest-first scan sees it, before its
/// body is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoScanRow {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub from_name: Option<String>,
    pub from_email: String,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub snippet: String,
    pub flags: u32,
    pub outbound: bool,
}

/// A first run's progress for one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoRun {
    pub rules_version: i64,
    /// The last message classified, newest first: `(date, id)`.
    pub cursor: Option<(DateTime<Utc>, MessageId)>,
    pub scanned: i64,
    pub completed_at: Option<DateTime<Utc>>,
}

/// A promise you made, with what a to-do row needs from around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromiseForTodo {
    pub commitment_id: String,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub evidence_msg_id: MessageId,
    pub evidence_date: Option<DateTime<Utc>>,
    pub email: String,
    pub contact_name: Option<String>,
    pub what: String,
    pub by_when: Option<DateTime<Utc>>,
    /// open | resolved | expired
    pub status: String,
}

const SCAN_COLUMNS: &str =
    "id, account_id, thread_id, from_name, from_email, date, subject, snippet, flags, direction";

fn row_to_scan_row(row: &SqliteRow) -> Result<TodoScanRow, sqlx::Error> {
    Ok(TodoScanRow {
        id: decode_id(&row.try_get::<String, _>("id")?)?,
        account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
        thread_id: decode_id(&row.try_get::<String, _>("thread_id")?)?,
        from_name: row.try_get("from_name")?,
        from_email: row.try_get("from_email")?,
        date: decode_timestamp(row.try_get("date")?)?,
        subject: row.try_get("subject")?,
        snippet: row.try_get("snippet")?,
        flags: u32::try_from(row.try_get::<i64, _>("flags")?).unwrap_or(0),
        outbound: row.try_get::<String, _>("direction")? == "outbound",
    })
}

impl super::Store {
    pub async fn get_todo_run(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<TodoRun>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT rules_version, cursor_date, cursor_message_id, scanned, completed_at
             FROM todo_runs WHERE account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            let cursor_date: Option<i64> = row.try_get("cursor_date")?;
            let cursor_id: Option<String> = row.try_get("cursor_message_id")?;
            let cursor = match (cursor_date, cursor_id) {
                (Some(date), Some(id)) => Some((decode_timestamp(date)?, decode_id(&id)?)),
                _ => None,
            };
            Ok(TodoRun {
                rules_version: row.try_get("rules_version")?,
                cursor,
                scanned: row.try_get("scanned")?,
                completed_at: decode_optional_timestamp(row.try_get("completed_at")?)?,
            })
        })
        .transpose()
    }

    /// Starts a run for `rules_version`, replacing any run of an older
    /// version. Rows already found keep their claims and decisions.
    pub async fn start_todo_run(
        &self,
        account_id: &AccountId,
        rules_version: i64,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO todo_runs (account_id, rules_version, scanned, started_at)
             VALUES (?1, ?2, 0, ?3)
             ON CONFLICT(account_id) DO UPDATE SET
                rules_version = excluded.rules_version, cursor_date = NULL,
                cursor_message_id = NULL, scanned = 0, started_at = excluded.started_at,
                completed_at = NULL",
        )
        .bind(account_id.as_str())
        .bind(rules_version)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn advance_todo_run(
        &self,
        account_id: &AccountId,
        cursor: &(DateTime<Utc>, MessageId),
        scanned: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE todo_runs SET cursor_date = ?2, cursor_message_id = ?3, scanned = scanned + ?4
             WHERE account_id = ?1",
        )
        .bind(account_id.as_str())
        .bind(cursor.0.timestamp())
        .bind(cursor.1.as_str())
        .bind(scanned)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn complete_todo_run(
        &self,
        account_id: &AccountId,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE todo_runs SET completed_at = ?2 WHERE account_id = ?1")
            .bind(account_id.as_str())
            .bind(now.timestamp())
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// The next page of the newest-first scan: messages strictly older than
    /// `before` in `(date, id)` order. Headers only; the caller reads a
    /// body when the subject or snippet suggests a to-do.
    pub async fn list_messages_for_todo_scan(
        &self,
        account_id: &AccountId,
        before: Option<&(DateTime<Utc>, MessageId)>,
        limit: u32,
    ) -> Result<Vec<TodoScanRow>, sqlx::Error> {
        let (date, id) = before.map_or((i64::MAX, String::new()), |(date, id)| {
            (date.timestamp(), id.as_str())
        });
        let sql = format!(
            "SELECT {SCAN_COLUMNS} FROM messages
             WHERE account_id = ?1 AND (date < ?2 OR (date = ?2 AND id < ?3))
             ORDER BY date DESC, id DESC
             LIMIT ?4"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(date)
            .bind(id)
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_scan_row)
            .collect()
    }

    /// The scan's view of these messages, newest first.
    pub async fn list_todo_scan_rows(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<TodoScanRow>, sqlx::Error> {
        let mut out = Vec::with_capacity(message_ids.len());
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let sql = format!(
                "SELECT {SCAN_COLUMNS} FROM messages WHERE id IN ({})",
                in_list(chunk.len())
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in chunk {
                query = query.bind(id.as_str());
            }
            for row in query.fetch_all(self.reader()).await? {
                out.push(row_to_scan_row(&row)?);
            }
        }
        out.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| b.id.as_str().cmp(&a.id.as_str()))
        });
        Ok(out)
    }

    /// Promises you made, with their evidence date and the person's name.
    pub async fn list_promises_for_todos(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<PromiseForTodo>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT c.id, c.account_id, c.thread_id, c.evidence_msg_id, c.email, c.what,
                    c.by_when, c.status, m.date AS evidence_date,
                    (SELECT ct.display_name FROM contacts ct
                     WHERE ct.account_id = c.account_id AND ct.email = c.email) AS contact_name
             FROM contact_commitments c
             LEFT JOIN messages m ON m.id = c.evidence_msg_id
             WHERE c.account_id = ? AND c.direction = 'yours'",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(PromiseForTodo {
                    commitment_id: row.try_get("id")?,
                    account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                    thread_id: decode_id(&row.try_get::<String, _>("thread_id")?)?,
                    evidence_msg_id: decode_id(&row.try_get::<String, _>("evidence_msg_id")?)?,
                    evidence_date: decode_optional_timestamp(row.try_get("evidence_date")?)?,
                    email: row.try_get("email")?,
                    contact_name: row.try_get("contact_name")?,
                    what: row.try_get("what")?,
                    by_when: decode_optional_timestamp(row.try_get("by_when")?)?,
                    status: row.try_get("status")?,
                })
            })
            .collect()
    }

    /// The account's to-dos that mirror a promise, by commitment id.
    pub async fn list_promise_todos(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos WHERE account_id = ? AND commitment_id IS NOT NULL"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    /// Changes whenever a promise you made is added, resolved or expired,
    /// so the mirror only reruns when there is something to mirror.
    pub async fn promise_fingerprint(&self, account_id: &AccountId) -> Result<String, sqlx::Error> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS total, COALESCE(MAX(extracted_at), 0) AS extracted,
                    COALESCE(MAX(resolved_at), 0) AS resolved,
                    COALESCE(SUM(status = 'open'), 0) AS open,
                    COALESCE(SUM(by_when), 0) AS dates
             FROM contact_commitments WHERE account_id = ? AND direction = 'yours'",
        )
        .bind(account_id.as_str())
        .fetch_one(self.reader())
        .await?;
        Ok(format!(
            "{}:{}:{}:{}:{}",
            row.try_get::<i64, _>("total")?,
            row.try_get::<i64, _>("extracted")?,
            row.try_get::<i64, _>("resolved")?,
            row.try_get::<i64, _>("open")?,
            row.try_get::<i64, _>("dates")?
        ))
    }

    /// Open promise rows on a thread, for "you wrote to them after".
    pub async fn list_open_promise_todos_for_thread(
        &self,
        account_id: &AccountId,
        thread_id: &ThreadId,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let sql = format!(
            "SELECT {COLUMNS} FROM todos
             WHERE account_id = ? AND thread_id = ? AND kind = 'promise' AND state = 'open'"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(thread_id.as_str())
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(row_to_todo)
            .collect()
    }

    /// The message's calendar invite when it asks for a reply nobody has
    /// given yet: a REQUEST with RSVP asked and no answer recorded.
    pub async fn invite_awaiting_reply(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<mxr_core::types::CalendarMetadata>, sqlx::Error> {
        let metadata: Option<String> = sqlx::query_scalar(
            "SELECT metadata_json FROM calendar_invites
             WHERE message_id = ? AND UPPER(COALESCE(method, '')) = 'REQUEST'
               AND rsvp_requested = 1
               AND UPPER(COALESCE(current_partstat, 'NEEDS-ACTION')) = 'NEEDS-ACTION'
             ORDER BY updated_at DESC LIMIT 1",
        )
        .bind(message_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        metadata.as_deref().map(crate::decode_json).transpose()
    }
}
