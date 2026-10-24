//! User corrections and full-state undo share a transaction and compare the
//! complete row, so a same-second concurrent edit cannot be overwritten.
use crate::{decode_json, encode_json, Store, TodoRecord};
use chrono::{DateTime, Utc};
use sqlx::Row;

impl Store {
    pub async fn correct_todo(
        &self,
        id: &str,
        reason: &str,
        before: &TodoRecord,
        after: &TodoRecord,
        dry_run: bool,
        now: DateTime<Utc>,
    ) -> Result<TodoRecord, sqlx::Error> {
        if !["not_a_todo", "not_mine", "already_done", "wrong_details"].contains(&reason)
            || before.id != after.id
            || before.account_id != after.account_id
            || before.reason != after.reason
            || before.source_message_id != after.source_message_id
        {
            return Err(sqlx::Error::Protocol("invalid to-do correction".into()));
        }
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let current = correction_row(&mut tx, &before.id).await?;
        if current != *before {
            return Err(sqlx::Error::Protocol(
                "To-do changed; refresh before correcting it".into(),
            ));
        }
        if dry_run {
            tx.rollback().await?;
            return Ok(after.clone());
        }
        replace_row(&mut tx, after).await?;
        let saved = correction_row(&mut tx, &before.id).await?;
        sqlx::query("INSERT INTO todo_corrections (id, todo_id, reason, before_json, after_json, created_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(id).bind(&before.id).bind(reason).bind(encode_json(before)?).bind(encode_json(&saved)?).bind(now.timestamp()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(saved)
    }

    pub async fn undo_todo_correction(
        &self,
        todo_id: &str,
        correction_id: &str,
        dry_run: bool,
        now: DateTime<Utc>,
    ) -> Result<TodoRecord, sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT before_json, after_json FROM todo_corrections WHERE id = ? AND todo_id = ? AND undone_at IS NULL AND rowid = (SELECT max(rowid) FROM todo_corrections WHERE todo_id = ?)")
            .bind(correction_id).bind(todo_id).bind(todo_id).fetch_optional(&mut *tx).await?
            .ok_or_else(|| sqlx::Error::Protocol("That correction is no longer the latest; refresh the to-do".into()))?;
        let before: TodoRecord = decode_json(row.try_get("before_json")?)?;
        let after: TodoRecord = decode_json(row.try_get("after_json")?)?;
        let current = correction_row(&mut tx, todo_id).await?;
        if current != after {
            return Err(sqlx::Error::Protocol(
                "To-do changed after that correction; newer work was kept".into(),
            ));
        }
        if dry_run {
            tx.rollback().await?;
            return Ok(before);
        }
        replace_row(&mut tx, &before).await?;
        sqlx::query("UPDATE todo_corrections SET undone_at = ? WHERE id = ?")
            .bind(now.timestamp())
            .bind(correction_id)
            .execute(&mut *tx)
            .await?;
        let saved = correction_row(&mut tx, todo_id).await?;
        tx.commit().await?;
        Ok(saved)
    }
}

async fn correction_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
) -> Result<TodoRecord, sqlx::Error> {
    let sql = format!("SELECT {} FROM todos WHERE id = ?", crate::todos::COLUMNS);
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    crate::todos::row_to_todo(&row)
}

pub(crate) async fn replace_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    record: &TodoRecord,
) -> Result<(), sqlx::Error> {
    let assignments = crate::todos::COLUMNS
        .split(',')
        .enumerate()
        .map(|(i, column)| format!("{} = ?{}", column.trim(), i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("UPDATE todos SET {assignments} WHERE id = ?1");
    crate::todos::bind_record(sqlx::query(sqlx::AssertSqlSafe(sql)), record)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
