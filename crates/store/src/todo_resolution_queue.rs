//! Durable per-input attempts for the single to-do background consumer.

use crate::{decode_id, decode_timestamp, todo_resolution_fingerprint, Store, TodoRecord};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::MessageFlags;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};

#[derive(Debug, Clone)]
pub struct TodoResolutionWork {
    pub account_id: AccountId,
    pub todo_id: String,
    pub message_id: MessageId,
    pub revision: String,
    pub input_fingerprint: String,
    pub attempts: u32,
}

/// Bodies are read at execution time; the queue contains only their digest.
#[derive(Debug)]
pub struct TodoResolutionInput {
    pub task: TodoRecord,
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub original_subject: String,
    pub original_text_plain: Option<String>,
    pub original_text_html: Option<String>,
    pub fingerprint: String,
}

const INPUT_SQL: &str = "SELECT t.*, later.id AS later_id,
    later.thread_id AS later_thread, later.date AS later_date,
    later.from_email AS later_sender, later.subject AS later_subject,
    later.flags AS later_flags, later.direction AS later_direction,
    lb.text_plain AS later_plain, lb.text_html AS later_html,
    source.subject AS original_subject, source.date AS original_date,
    source.thread_id AS original_thread, source.from_email AS original_sender,
    source.flags AS original_flags, source.direction AS original_direction,
    sb.text_plain AS original_plain, sb.text_html AS original_html
    FROM todos t JOIN accounts a ON a.id = t.account_id AND a.enabled = 1
    JOIN messages source ON source.id = t.source_message_id AND source.account_id = t.account_id
    JOIN bodies sb ON sb.message_id = source.id
    JOIN messages later ON later.id = ?2 AND later.account_id = t.account_id
    JOIN bodies lb ON lb.message_id = later.id
    WHERE t.id = ?1 AND t.state = 'open'
    AND later.direction = 'inbound'
    AND (later.date > source.date OR (later.date = source.date AND later.id > source.id))
    AND (LOWER(later.from_email) = LOWER(source.from_email) OR later.thread_id = source.thread_id)";

async fn read_input(
    connection: &mut SqliteConnection,
    todo_id: &str,
    message_id: &MessageId,
) -> Result<Option<TodoResolutionInput>, sqlx::Error> {
    let row = sqlx::query(INPUT_SQL)
        .bind(todo_id)
        .bind(message_id.as_str())
        .fetch_optional(connection)
        .await?;
    let Some(row) = row else { return Ok(None) };
    let flags = MessageFlags::from_bits_truncate(row.try_get::<i64, _>("later_flags")? as u32);
    if flags.intersects(MessageFlags::SPAM | MessageFlags::TRASH) {
        return Ok(None);
    }
    let task = crate::todos::row_to_todo(&row)?;
    // Length framing avoids concatenation collisions and hashing raw bytes
    // detects edits outside the model's truncated reader text too.
    let mut digest = Sha256::new();
    for column in [
        "later_id",
        "later_thread",
        "later_sender",
        "later_subject",
        "later_direction",
        "later_plain",
        "later_html",
        "original_subject",
        "original_thread",
        "original_sender",
        "original_direction",
        "original_plain",
        "original_html",
    ] {
        match row.try_get::<Option<String>, _>(column)? {
            Some(value) => {
                digest.update([1]);
                digest.update((value.len() as u64).to_be_bytes());
                digest.update(value.as_bytes());
            }
            None => digest.update([0]),
        }
    }
    for column in ["later_date", "original_date"] {
        digest.update(row.try_get::<i64, _>(column)?.to_be_bytes());
    }
    for column in ["later_flags", "original_flags"] {
        let flags = row.try_get::<i64, _>(column)? as u32;
        digest.update((flags & (MessageFlags::SPAM | MessageFlags::TRASH).bits()).to_be_bytes());
    }
    digest.update(todo_resolution_fingerprint(&task).as_bytes());
    Ok(Some(TodoResolutionInput {
        task,
        message_id: decode_id(&row.try_get::<String, _>("later_id")?)?,
        thread_id: decode_id(&row.try_get::<String, _>("later_thread")?)?,
        date: decode_timestamp(row.try_get("later_date")?)?,
        subject: row.try_get("later_subject")?,
        text_plain: row.try_get("later_plain")?,
        text_html: row.try_get("later_html")?,
        original_subject: row.try_get("original_subject")?,
        original_text_plain: row.try_get("original_plain")?,
        original_text_html: row.try_get("original_html")?,
        fingerprint: base16ct::lower::encode_string(&digest.finalize()),
    }))
}

// Apply this inside the offer transaction, so an edited body or replaced
// queue revision cannot race the final evidence write.
pub(crate) async fn input_is_current(
    connection: &mut SqliteConnection,
    work: &TodoResolutionWork,
) -> Result<bool, sqlx::Error> {
    let revision: Option<String> = sqlx::query_scalar(
        "SELECT revision FROM todo_resolution_queue WHERE account_id = ? AND todo_id = ? AND message_id = ?",
    )
    .bind(work.account_id.as_str())
    .bind(&work.todo_id)
    .bind(work.message_id.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if revision.as_deref() != Some(&work.revision) {
        return Ok(false);
    }
    Ok(read_input(connection, &work.todo_id, &work.message_id)
        .await?
        .is_some_and(|input| input.fingerprint == work.input_fingerprint))
}

async fn enqueue_input(
    connection: &mut SqliteConnection,
    input: &TodoResolutionInput,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO todo_resolution_queue
            (account_id, todo_id, message_id, revision, input_fingerprint, source_date, next_attempt_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(account_id, todo_id, message_id) DO UPDATE SET
                revision = excluded.revision, input_fingerprint = excluded.input_fingerprint,
                source_date = excluded.source_date, attempts = 0,
                next_attempt_at = excluded.next_attempt_at, completed = 0
            WHERE todo_resolution_queue.input_fingerprint <> excluded.input_fingerprint")
            .bind(input.task.account_id.as_str()).bind(&input.task.id)
            .bind(input.message_id.as_str()).bind(uuid::Uuid::now_v7().to_string())
            .bind(&input.fingerprint).bind(input.date.timestamp()).bind(now.timestamp())
            .execute(connection).await?;
    Ok(())
}

const DUE_SQL: &str = "FROM todo_resolution_queue q
    JOIN accounts a ON a.id = q.account_id AND a.enabled = 1
    WHERE q.completed = 0 AND q.attempts < 5 AND q.next_attempt_at <= ?1";

impl Store {
    pub async fn todo_resolution_evidence_is_newer(
        &self,
        todo_id: &str,
        message_id: &MessageId,
        date: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM todo_resolution_evidence WHERE todo_id = ?1 AND (source_date > ?2 OR (source_date = ?2 AND source_message_id >= ?3)))")
            .bind(todo_id).bind(date.timestamp()).bind(message_id.as_str()).fetch_one(self.reader()).await
    }
    pub async fn todo_resolution_input(
        &self,
        todo_id: &str,
        message_id: &MessageId,
    ) -> Result<Option<TodoResolutionInput>, sqlx::Error> {
        let mut connection = self.reader().acquire().await?;
        read_input(&mut connection, todo_id, message_id).await
    }

    pub async fn enqueue_todo_resolution(
        &self,
        input: &TodoResolutionInput,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.writer().acquire().await?;
        enqueue_input(&mut connection, input, now).await
    }

    pub async fn list_due_todo_resolutions(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<TodoResolutionWork>, sqlx::Error> {
        let sql = format!("SELECT q.* {DUE_SQL} ORDER BY q.next_attempt_at, q.source_date DESC, q.message_id DESC, q.todo_id LIMIT ?2");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(now.timestamp())
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?
            .iter()
            .map(|row| {
                Ok(TodoResolutionWork {
                    account_id: decode_id(&row.try_get::<String, _>("account_id")?)?,
                    todo_id: row.try_get("todo_id")?,
                    message_id: decode_id(&row.try_get::<String, _>("message_id")?)?,
                    revision: row.try_get("revision")?,
                    input_fingerprint: row.try_get("input_fingerprint")?,
                    attempts: row.try_get::<i64, _>("attempts")? as u32,
                })
            })
            .collect()
    }

    pub async fn todo_resolution_queue_counts(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(i64, i64), sqlx::Error> {
        let pending = sqlx::query_scalar("SELECT COUNT(*) FROM todo_resolution_queue q JOIN accounts a ON a.id = q.account_id AND a.enabled = 1 WHERE q.completed = 0 AND q.attempts < 5")
            .fetch_one(self.reader()).await?;
        let due = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) {DUE_SQL}")))
            .bind(now.timestamp())
            .fetch_one(self.reader())
            .await?;
        Ok((pending, due))
    }

    /// Reserve before awaiting the provider. Cancellation leaves a durable
    /// bounded retry rather than a lost row or an immediate duplicate call.
    pub async fn reserve_todo_resolution_attempt(
        &self,
        work: &TodoResolutionWork,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let hours = 4_i64.pow(work.attempts).min(24);
        let result = sqlx::query(
            "UPDATE todo_resolution_queue SET attempts = attempts + 1, next_attempt_at = ?1
            WHERE account_id = ?2 AND todo_id = ?3 AND message_id = ?4 AND revision = ?5
            AND completed = 0 AND attempts = ?6 AND attempts < 5 AND next_attempt_at <= ?7",
        )
        .bind(now.timestamp() + hours * 3600)
        .bind(work.account_id.as_str())
        .bind(&work.todo_id)
        .bind(work.message_id.as_str())
        .bind(&work.revision)
        .bind(i64::from(work.attempts))
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn finish_todo_resolution_work(
        &self,
        work: &TodoResolutionWork,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let current_revision: Option<String> = sqlx::query_scalar(
            "SELECT q.revision FROM todo_resolution_queue q
            JOIN accounts a ON a.id = q.account_id AND a.enabled = 1
            WHERE q.account_id = ?1 AND q.todo_id = ?2 AND q.message_id = ?3",
        )
        .bind(work.account_id.as_str())
        .bind(&work.todo_id)
        .bind(work.message_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if current_revision.as_deref() != Some(&work.revision) {
            tx.rollback().await?;
            return Ok(());
        }
        let current = read_input(&mut tx, &work.todo_id, &work.message_id).await?;
        if let Some(input) = current.filter(|input| input.fingerprint != work.input_fingerprint) {
            enqueue_input(&mut tx, &input, Utc::now()).await?;
        } else {
            sqlx::query("UPDATE todo_resolution_queue SET completed = 1 WHERE account_id = ? AND todo_id = ? AND message_id = ? AND revision = ?")
                .bind(work.account_id.as_str()).bind(&work.todo_id).bind(work.message_id.as_str()).bind(&work.revision)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// A breaker or privacy gate made no request; undo only this revision's
    /// reservation and revisit on the normal idle cadence.
    pub async fn defer_unattempted_todo_resolution(
        &self,
        work: &TodoResolutionWork,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE todo_resolution_queue SET attempts = ?1, next_attempt_at = ?2
            WHERE account_id = ?3 AND todo_id = ?4 AND message_id = ?5 AND revision = ?6 AND attempts = ?7")
            .bind(i64::from(work.attempts)).bind(now.timestamp() + 300)
            .bind(work.account_id.as_str()).bind(&work.todo_id).bind(work.message_id.as_str()).bind(&work.revision)
            .bind(i64::from(work.attempts) + 1).execute(self.writer()).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
