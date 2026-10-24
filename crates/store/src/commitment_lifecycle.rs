//! Promise resolution and Desk undo compare both sides in one transaction.
use crate::{CommitmentStatus, ContactCommitmentRecord, Store, TodoRecord, TodoState};
use chrono::{DateTime, Utc};
use mxr_core::AccountId;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PromiseResolution {
    pub before: ContactCommitmentRecord,
    pub after: ContactCommitmentRecord,
    pub todos: Vec<(TodoRecord, TodoRecord)>,
}

async fn commitment(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account: &AccountId,
    id: &str,
) -> Result<Option<ContactCommitmentRecord>, sqlx::Error> {
    sqlx::query("SELECT * FROM contact_commitments WHERE account_id=? AND id=?")
        .bind(account.as_str())
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .map(crate::contact_commitments::row_to_commitment)
        .transpose()
}

async fn todo(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
) -> Result<Option<TodoRecord>, sqlx::Error> {
    let sql = format!("SELECT {} FROM todos WHERE id=?", crate::todos::COLUMNS);
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .as_ref()
        .map(crate::todos::row_to_todo)
        .transpose()
}

impl Store {
    /// Desk requires Open; an explicit direct command may repair closed history.
    pub async fn resolve_promise_with_todos(
        &self,
        account: &AccountId,
        id: &str,
        expected_status: Option<CommitmentStatus>,
        now: DateTime<Utc>,
    ) -> Result<Option<PromiseResolution>, sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let Some(before) = commitment(&mut tx, account, id).await? else {
            return Ok(None);
        };
        if expected_status.is_some_and(|status| before.status != status) {
            return Ok(None);
        }
        let sql = format!(
            "SELECT {} FROM todos WHERE account_id=? AND commitment_id=? AND state='open'",
            crate::todos::COLUMNS
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account.as_str())
            .bind(id)
            .fetch_all(&mut *tx)
            .await?;
        let mut todos = Vec::new();
        for row in &rows {
            let prior = crate::todos::row_to_todo(row)?;
            let mut after = prior.clone();
            after.state = TodoState::Done;
            after.done_at = Some(now);
            after.updated_at = now;
            crate::todo_corrections::replace_row(&mut tx, &after).await?;
            let saved = todo(&mut tx, &prior.id)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            todos.push((prior, saved));
        }
        sqlx::query("UPDATE contact_commitments SET status='resolved', resolved_at=? WHERE account_id=? AND id=?")
            .bind(now.timestamp()).bind(account.as_str()).bind(id).execute(&mut *tx).await?;
        let after = commitment(&mut tx, account, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        tx.commit().await?;
        Ok(Some(PromiseResolution {
            before,
            after,
            todos,
        }))
    }

    /// Restore both sides only while every row still matches this Done.
    pub async fn undo_promise_resolution(
        &self,
        snapshot: &PromiseResolution,
    ) -> Result<Vec<TodoRecord>, sqlx::Error> {
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let account = &snapshot.after.account_id;
        if commitment(&mut tx, account, &snapshot.after.id)
            .await?
            .as_ref()
            != Some(&snapshot.after)
        {
            return Ok(Vec::new());
        }
        for (_, after) in &snapshot.todos {
            if todo(&mut tx, &after.id).await?.as_ref() != Some(after) {
                return Ok(Vec::new());
            }
        }
        for (before, _) in &snapshot.todos {
            crate::todo_corrections::replace_row(&mut tx, before).await?;
        }
        sqlx::query(
            "UPDATE contact_commitments SET status=?, resolved_at=? WHERE account_id=? AND id=?",
        )
        .bind(snapshot.before.status.as_str())
        .bind(snapshot.before.resolved_at.map(|at| at.timestamp()))
        .bind(account.as_str())
        .bind(&snapshot.before.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(snapshot
            .todos
            .iter()
            .map(|(before, _)| before.clone())
            .collect())
    }
}
