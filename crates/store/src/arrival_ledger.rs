//! Where each email went when it arrived (`arrivals`) and what the user
//! corrected (`mode_corrections`), D119.
//!
//! A trigger on `messages` writes the arrivals row the moment an inbound
//! message is stored; the daemon places it after sync. Every read here
//! counts and lists from the same rows with the same window, so the
//! arrivals line's counts and the lists its counts open cannot disagree.
//!
//! The effective mode of an arrival is `now_mode` when a correction set
//! one, else `mode`, else `sorting` while the daemon hasn't placed it.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp, encode_json, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::MessageFlags;
use sqlx::Row;
use std::collections::HashMap;
use std::time::Instant;

/// The effective mode, in SQL, for an `arrivals a` row.
const EFFECTIVE: &str = "COALESCE(a.now_mode, a.mode, 'sorting')";

/// An arrival the daemon has not placed yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingArrival {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    /// Flagged or labelled Spam when it was placed.
    pub spam: bool,
    /// Sent by the account itself, by stored direction; such rows are
    /// dropped, never placed.
    pub outbound: bool,
}

/// Where the daemon placed one arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalPlacement {
    pub message_id: MessageId,
    /// messages | updates | reading | screened_out | spam
    pub mode: String,
    pub rule: String,
    pub reason: String,
    pub not_sure: Option<String>,
}

/// One arrival with what a row in its list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalRow {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub sender_email: String,
    pub from_name: Option<String>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub first_seen_at: DateTime<Utc>,
    /// Where it arrived; `None` while sorting.
    pub mode: Option<String>,
    pub rule: Option<String>,
    pub reason: Option<String>,
    pub not_sure: Option<String>,
    /// Where it is now: `now_mode`, else `mode`, else `sorting`.
    pub effective: String,
    /// The email's own move (X) as stored, in force or not, for undo.
    pub moved_to: Option<String>,
    pub moved_at: Option<DateTime<Utc>>,
    /// A per-email move set `now_mode` and no later sender decision
    /// overrode it.
    pub moved: bool,
    pub unread: bool,
    pub also_todo: bool,
    pub also_archive: bool,
    /// A Not-sure answer (or any correction of this email) exists.
    pub answered: bool,
}

/// The arrivals in a window, counted once each by effective mode, plus the
/// aspects shown as "also".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArrivalCounts {
    pub total: u32,
    /// Effective mode (or `sorting`) to count.
    pub by_mode: HashMap<String, u32>,
    pub also_todo: u32,
    pub also_archive: u32,
}

/// A correction to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCorrection {
    pub account_id: AccountId,
    /// email | sender
    pub scope: String,
    pub message_id: Option<MessageId>,
    pub sender_email: String,
    pub from_mode: String,
    pub to_mode: String,
    pub rule: Option<String>,
    /// move | sender | not_sure
    pub source: String,
    pub created_at: DateTime<Utc>,
    /// An email move's undo: the move it replaced, if any.
    pub prior_moved_to: Option<String>,
    pub prior_moved_at: Option<DateTime<Utc>>,
    /// A sender move's decision before it: the disposition, or "" for none.
    pub prior_disposition: Option<String>,
    /// When that decision was made, so undo can put it back unchanged.
    pub prior_decided_at: Option<DateTime<Utc>>,
    pub aspect_id: Option<String>,
}

/// A stored correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub id: i64,
    pub fields: NewCorrection,
    pub undone_at: Option<DateTime<Utc>>,
}

fn ids_json(ids: &[MessageId]) -> Result<String, sqlx::Error> {
    encode_json(&ids.iter().map(MessageId::as_str).collect::<Vec<_>>())
}

fn account_ids_json(accounts: &[AccountId]) -> Result<String, sqlx::Error> {
    encode_json(&accounts.iter().map(AccountId::as_str).collect::<Vec<_>>())
}

fn hidden_spam_flag() -> i64 {
    i64::from(MessageFlags::SPAM.bits())
}

/// A per-email move is in force until a sender move overrides it: one made
/// through mxr stamps `superseded_by` (exact even within a second), and a
/// screener decision made anywhere else counts when it is newer.
fn move_in_force(alias: &str) -> String {
    format!(
        "{alias}.moved_at IS NOT NULL AND {alias}.moved_to IS NOT NULL
         AND {alias}.superseded_by IS NULL AND NOT EXISTS (
            SELECT 1 FROM screener_decisions d
            WHERE d.account_id = {alias}.account_id
              AND d.sender_email = {alias}.sender_email COLLATE NOCASE
              AND d.disposition != 'unknown' AND d.decided_at > {alias}.moved_at)"
    )
}

/// Columns of an `ArrivalRow`, from `arrivals a JOIN messages m`.
fn row_columns() -> String {
    format!(
        "a.message_id, a.account_id, m.thread_id, a.sender_email, m.from_name, m.subject,
         m.date, m.flags, a.first_seen_at, a.mode, a.rule, a.reason, a.not_sure,
         a.moved_to, a.moved_at,
         {EFFECTIVE} AS effective,
         ({moved}) AS moved,
         EXISTS (SELECT 1 FROM todos t
                 WHERE t.source_message_id = a.message_id AND t.state = 'open') AS also_todo,
         EXISTS (SELECT 1 FROM record_messages rm JOIN records r ON r.id = rm.record_id
                 WHERE rm.message_id = a.message_id AND r.dismissed_at IS NULL) AS also_archive,
         EXISTS (SELECT 1 FROM mode_corrections c
                 WHERE c.message_id = a.message_id AND c.undone_at IS NULL) AS answered",
        moved = move_in_force("a")
    )
}

fn decode_row(row: &sqlx::sqlite::SqliteRow) -> Result<ArrivalRow, sqlx::Error> {
    let flags = MessageFlags::from_bits_truncate(row.try_get::<i64, _>("flags")? as u32);
    Ok(ArrivalRow {
        message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
        sender_email: row.try_get("sender_email")?,
        from_name: row.try_get("from_name")?,
        subject: row.try_get("subject")?,
        date: decode_timestamp(row.try_get("date")?)?,
        first_seen_at: decode_timestamp(row.try_get("first_seen_at")?)?,
        mode: row.try_get("mode")?,
        rule: row.try_get("rule")?,
        reason: row.try_get("reason")?,
        not_sure: row.try_get("not_sure")?,
        effective: row.try_get("effective")?,
        moved_to: row.try_get("moved_to")?,
        moved_at: decode_optional_timestamp(row.try_get("moved_at")?)?,
        moved: row.try_get("moved")?,
        unread: !flags.contains(MessageFlags::READ),
        also_todo: row.try_get("also_todo")?,
        also_archive: row.try_get("also_archive")?,
        answered: row.try_get("answered")?,
    })
}

fn decode_correction(row: &sqlx::sqlite::SqliteRow) -> Result<Correction, sqlx::Error> {
    let message_id: Option<String> = row.try_get("message_id")?;
    Ok(Correction {
        id: row.try_get("id")?,
        undone_at: decode_optional_timestamp(row.try_get("undone_at")?)?,
        fields: NewCorrection {
            account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
            scope: row.try_get("scope")?,
            message_id: message_id.as_deref().map(decode_id).transpose()?,
            sender_email: row.try_get("sender_email")?,
            from_mode: row.try_get("from_mode")?,
            to_mode: row.try_get("to_mode")?,
            rule: row.try_get("rule")?,
            source: row.try_get("source")?,
            created_at: decode_timestamp(row.try_get("created_at")?)?,
            prior_moved_to: row.try_get("prior_moved_to")?,
            prior_moved_at: decode_optional_timestamp(row.try_get("prior_moved_at")?)?,
            prior_disposition: row.try_get("prior_disposition")?,
            prior_decided_at: decode_optional_timestamp(row.try_get("prior_decided_at")?)?,
            aspect_id: row.try_get("aspect_id")?,
        },
    })
}

const CORRECTION_COLUMNS: &str = "id, account_id, scope, message_id, sender_email, from_mode,
    to_mode, rule, source, created_at, undone_at, prior_moved_to, prior_moved_at,
    prior_disposition, prior_decided_at, aspect_id";

impl super::Store {
    /// Arrivals of `account_id` not placed yet, oldest first.
    pub async fn pending_arrivals(
        &self,
        account_id: &AccountId,
        limit: u32,
    ) -> Result<Vec<PendingArrival>, sqlx::Error> {
        let started_at = Instant::now();
        let rows = sqlx::query(
            "SELECT a.message_id, m.thread_id, m.direction,
                    ((m.flags & ?2) != 0 OR EXISTS (
                        SELECT 1 FROM message_labels ml JOIN labels l ON l.id = ml.label_id
                        WHERE ml.message_id = m.id AND l.provider_id = 'SPAM')) AS spam
             FROM arrivals a JOIN messages m ON m.id = a.message_id
             WHERE a.account_id = ?1 AND a.mode IS NULL
             ORDER BY a.first_seen_at, a.message_id
             LIMIT ?3",
        )
        .bind(account_id.as_str())
        .bind(hidden_spam_flag())
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        trace_query("arrivals.pending", started_at, rows.len());
        rows.iter()
            .map(|row| {
                Ok(PendingArrival {
                    message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
                    thread_id: decode_id(row.try_get::<&str, _>("thread_id")?)?,
                    spam: row.try_get("spam")?,
                    outbound: row.try_get::<String, _>("direction")? == "outbound",
                })
            })
            .collect()
    }

    /// Drop rows that are not arrivals after all (mail the account sent).
    pub async fn delete_arrivals(&self, message_ids: &[MessageId]) -> Result<u64, sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(0);
        }
        let result = sqlx::query(
            "DELETE FROM arrivals WHERE message_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(ids_json(message_ids)?)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }

    /// Write first placements. A row placed already keeps its placement:
    /// where mail went on arrival is a past fact.
    pub async fn set_arrival_placements(
        &self,
        placements: &[ArrivalPlacement],
        at: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        if placements.is_empty() {
            return Ok(0);
        }
        let mut tx = self.writer().begin().await?;
        let mut changed = 0;
        for placement in placements {
            changed += sqlx::query(
                "UPDATE arrivals SET mode = ?2, rule = ?3, reason = ?4, not_sure = ?5,
                        placed_at = ?6
                 WHERE message_id = ?1 AND mode IS NULL",
            )
            .bind(placement.message_id.as_str())
            .bind(&placement.mode)
            .bind(&placement.rule)
            .bind(&placement.reason)
            .bind(&placement.not_sure)
            .bind(at.timestamp())
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        tx.commit().await?;
        Ok(changed)
    }

    /// Where each arrival is now, re-placed after a correction. A move in
    /// force re-places to where it moved, since placement honours it.
    pub async fn set_arrival_now_modes(
        &self,
        now_modes: &[(MessageId, Option<String>)],
    ) -> Result<(), sqlx::Error> {
        if now_modes.is_empty() {
            return Ok(());
        }
        let mut tx = self.writer().begin().await?;
        for (message_id, now_mode) in now_modes {
            sqlx::query(
                "UPDATE arrivals SET now_mode = CASE WHEN ?2 = mode THEN NULL ELSE ?2 END
                 WHERE message_id = ?1 AND mode IS NOT NULL",
            )
            .bind(message_id.as_str())
            .bind(now_mode)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// The placed arrivals from one sender, Spam aside: mail the provider
    /// called spam stays counted there whatever the sender's mode.
    pub async fn arrivals_from_sender(
        &self,
        account_id: &AccountId,
        sender_email: &str,
    ) -> Result<Vec<(MessageId, ThreadId)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT a.message_id, m.thread_id FROM arrivals a
             JOIN messages m ON m.id = a.message_id
             WHERE a.account_id = ?1 AND a.sender_email = lower(?2)
               AND a.mode IS NOT NULL AND a.mode != 'spam'",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("message_id")?)?,
                    decode_id(row.try_get::<&str, _>("thread_id")?)?,
                ))
            })
            .collect()
    }

    /// Per-email moves in force in this account, message to mode. Few rows:
    /// one per email the user moved.
    pub async fn arrival_moves_in_force(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<MessageId, String>, sqlx::Error> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT a.message_id, a.moved_to FROM arrivals a
             WHERE a.account_id = ?1 AND {}",
            move_in_force("a")
        )))
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    decode_id(row.try_get::<&str, _>("message_id")?)?,
                    row.try_get::<String, _>("moved_to")?,
                ))
            })
            .collect()
    }

    /// Make sure an email has an arrivals row, so a move of mail older than
    /// the ledger still has somewhere to live. Its first-seen time is its
    /// date, so it never lands in a recent window. Returns false for an
    /// unknown message.
    pub async fn ensure_arrival(&self, message_id: &MessageId) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "INSERT INTO arrivals (message_id, account_id, sender_email, first_seen_at)
             SELECT id, account_id, lower(from_email),
                    MIN(date, CAST(strftime('%s', 'now') AS INTEGER))
             FROM messages WHERE id = ?1
             ON CONFLICT(message_id) DO NOTHING",
        )
        .bind(message_id.as_str())
        .execute(self.writer())
        .await?;
        if result.rows_affected() > 0 {
            return Ok(true);
        }
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM arrivals WHERE message_id = ?1")
                .bind(message_id.as_str())
                .fetch_one(self.reader())
                .await?
                > 0,
        )
    }

    /// Set or put back one email's move. A move is where the email is now
    /// until something re-places it; `None` clears the move and leaves
    /// `now_mode` for the caller to re-place.
    pub async fn set_arrival_move(
        &self,
        message_id: &MessageId,
        moved_to: Option<&str>,
        moved_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE arrivals SET moved_to = ?2, moved_at = ?3, superseded_by = NULL,
                    now_mode = CASE WHEN ?2 IS NULL THEN now_mode
                                    WHEN ?2 = mode THEN NULL ELSE ?2 END
             WHERE message_id = ?1",
        )
        .bind(message_id.as_str())
        .bind(moved_to)
        .bind(moved_at.map(|at| at.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Whether a later move of this email still stands: undoing an older
    /// move then would throw the newer one away. Moves that only kept the
    /// email where it was, and the To do and Archive aspects, don't count.
    pub async fn newer_email_move_stands(
        &self,
        message_id: &MessageId,
        after_correction_id: i64,
    ) -> Result<bool, sqlx::Error> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mode_corrections
             WHERE message_id = ?1 AND id > ?2 AND scope = 'email' AND undone_at IS NULL
               AND from_mode != to_mode AND to_mode IN ('messages', 'updates', 'reading')",
        )
        .bind(message_id.as_str())
        .bind(after_correction_id)
        .fetch_one(self.reader())
        .await?;
        Ok(n > 0)
    }

    /// The open to-do made from this email, if any.
    pub async fn open_todo_for_message(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT id FROM todos WHERE source_message_id = ?1 AND state = 'open'
             ORDER BY created_at LIMIT 1",
        )
        .bind(message_id.as_str())
        .fetch_optional(self.reader())
        .await
    }

    /// Put a sender decision's date back after undo restored the decision:
    /// a new date would override the email moves made after the original.
    pub async fn set_screener_decided_at(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        decided_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE screener_decisions SET decided_at = ?3
             WHERE account_id = ?1 AND sender_email = ?2 COLLATE NOCASE",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(decided_at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// A sender move overrides every move of that sender's mail in force.
    pub async fn supersede_sender_moves(
        &self,
        account_id: &AccountId,
        sender_email: &str,
        correction_id: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE arrivals SET superseded_by = ?3
             WHERE account_id = ?1 AND sender_email = lower(?2)
               AND moved_at IS NOT NULL AND superseded_by IS NULL",
        )
        .bind(account_id.as_str())
        .bind(sender_email)
        .bind(correction_id)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }

    /// Undoing a sender move lets the email moves it overrode stand again.
    pub async fn restore_superseded_moves(&self, correction_id: i64) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("UPDATE arrivals SET superseded_by = NULL WHERE superseded_by = ?1")
                .bind(correction_id)
                .execute(self.writer())
                .await?;
        Ok(result.rows_affected())
    }

    /// Arrival rows by message id, in no particular order.
    pub async fn arrivals_by_ids(
        &self,
        message_ids: &[MessageId],
    ) -> Result<Vec<ArrivalRow>, sqlx::Error> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {} FROM json_each(?1) wanted
             CROSS JOIN arrivals a ON a.message_id = wanted.value
             JOIN messages m ON m.id = a.message_id",
            row_columns()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ids_json(message_ids)?)
            .fetch_all(self.reader())
            .await?;
        rows.iter().map(decode_row).collect()
    }

    /// Every arrival first seen in `[since, until)` in `accounts`, counted
    /// once each by effective mode.
    pub async fn arrival_counts(
        &self,
        accounts: &[AccountId],
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<ArrivalCounts, sqlx::Error> {
        let started_at = Instant::now();
        let sql = format!(
            "SELECT {EFFECTIVE} AS effective, COUNT(*) AS n,
                    SUM(EXISTS (SELECT 1 FROM todos t
                        WHERE t.source_message_id = a.message_id AND t.state = 'open')) AS todo,
                    SUM(EXISTS (SELECT 1 FROM record_messages rm JOIN records r ON r.id = rm.record_id
                        WHERE rm.message_id = a.message_id AND r.dismissed_at IS NULL)) AS archive
             FROM arrivals a
             WHERE a.account_id IN (SELECT value FROM json_each(?1))
               AND a.first_seen_at >= ?2 AND a.first_seen_at < ?3
             GROUP BY effective"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_ids_json(accounts)?)
            .bind(since.timestamp())
            .bind(until.timestamp())
            .fetch_all(self.reader())
            .await?;
        trace_query("arrivals.counts", started_at, rows.len());
        let mut counts = ArrivalCounts::default();
        for row in &rows {
            let n = u32::try_from(row.try_get::<i64, _>("n")?).unwrap_or(u32::MAX);
            let effective: String = row.try_get("effective")?;
            counts.total += n;
            counts.also_todo += u32::try_from(row.try_get::<i64, _>("todo")?).unwrap_or(0);
            counts.also_archive += u32::try_from(row.try_get::<i64, _>("archive")?).unwrap_or(0);
            *counts.by_mode.entry(effective).or_default() += n;
        }
        Ok(counts)
    }

    /// The arrivals first seen in `[since, until)`, newest first, optionally
    /// only one effective mode (`todo` and `archive` select the aspects).
    /// Returns the page and how many match in all.
    pub async fn list_arrivals(
        &self,
        accounts: &[AccountId],
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        bucket: Option<&str>,
        limit: u32,
    ) -> Result<(Vec<ArrivalRow>, u32), sqlx::Error> {
        let started_at = Instant::now();
        // Every filter names ?4, so the bound parameters always match.
        let filter = match bucket {
            None => "?4 = ''",
            Some("todo") => "(also_todo AND ?4 = 'todo')",
            Some("archive") => "(also_archive AND ?4 = 'archive')",
            Some(_) => "effective = ?4",
        };
        let inner = format!(
            "SELECT {} FROM arrivals a JOIN messages m ON m.id = a.message_id
             WHERE a.account_id IN (SELECT value FROM json_each(?1))
               AND a.first_seen_at >= ?2 AND a.first_seen_at < ?3",
            row_columns()
        );
        let total_sql = format!("SELECT COUNT(*) FROM ({inner}) WHERE {filter}");
        let page_sql = format!(
            "SELECT * FROM ({inner}) WHERE {filter}
             ORDER BY first_seen_at DESC, date DESC, message_id DESC LIMIT ?5"
        );
        let accounts = account_ids_json(accounts)?;
        let bucket = bucket.unwrap_or_default().to_string();
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(total_sql))
            .bind(&accounts)
            .bind(since.timestamp())
            .bind(until.timestamp())
            .bind(&bucket)
            .fetch_one(self.reader())
            .await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(page_sql))
            .bind(&accounts)
            .bind(since.timestamp())
            .bind(until.timestamp())
            .bind(&bucket)
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?;
        trace_query("arrivals.list", started_at, rows.len());
        let rows = rows.iter().map(decode_row).collect::<Result<Vec<_>, _>>()?;
        Ok((rows, u32::try_from(total).unwrap_or(u32::MAX)))
    }

    /// Arrivals with a rule conflict, first seen in `[since, until)`, in
    /// arrival order.
    pub async fn not_sure_arrivals(
        &self,
        accounts: &[AccountId],
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<ArrivalRow>, sqlx::Error> {
        let sql = format!(
            "SELECT {} FROM arrivals a JOIN messages m ON m.id = a.message_id
             WHERE a.account_id IN (SELECT value FROM json_each(?1))
               AND a.first_seen_at >= ?2 AND a.first_seen_at < ?3
               AND a.not_sure IS NOT NULL
             ORDER BY a.first_seen_at, a.message_id",
            row_columns()
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_ids_json(accounts)?)
            .bind(since.timestamp())
            .bind(until.timestamp())
            .fetch_all(self.reader())
            .await?;
        rows.iter().map(decode_row).collect()
    }

    /// When the newest arrival was first seen.
    pub async fn latest_arrival_at(
        &self,
        accounts: &[AccountId],
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let at: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(first_seen_at) FROM arrivals
             WHERE account_id IN (SELECT value FROM json_each(?1))",
        )
        .bind(account_ids_json(accounts)?)
        .fetch_one(self.reader())
        .await?;
        decode_optional_timestamp(at)
    }

    pub async fn insert_correction(&self, correction: &NewCorrection) -> Result<i64, sqlx::Error> {
        let id = sqlx::query(
            "INSERT INTO mode_corrections
                 (account_id, scope, message_id, sender_email, from_mode, to_mode, rule, source,
                  created_at, prior_moved_to, prior_moved_at, prior_disposition,
                  prior_decided_at, aspect_id)
             VALUES (?1, ?2, ?3, lower(?4), ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(correction.account_id.as_str())
        .bind(&correction.scope)
        .bind(correction.message_id.as_ref().map(MessageId::as_str))
        .bind(&correction.sender_email)
        .bind(&correction.from_mode)
        .bind(&correction.to_mode)
        .bind(&correction.rule)
        .bind(&correction.source)
        .bind(correction.created_at.timestamp())
        .bind(&correction.prior_moved_to)
        .bind(correction.prior_moved_at.map(|at| at.timestamp()))
        .bind(&correction.prior_disposition)
        .bind(correction.prior_decided_at.map(|at| at.timestamp()))
        .bind(&correction.aspect_id)
        .execute(self.writer())
        .await?
        .last_insert_rowid();
        Ok(id)
    }

    pub async fn get_correction(&self, id: i64) -> Result<Option<Correction>, sqlx::Error> {
        let sql = format!("SELECT {CORRECTION_COLUMNS} FROM mode_corrections WHERE id = ?1");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(decode_correction)
            .transpose()
    }

    /// Stamp a correction undone. False when it was undone already, so a
    /// double undo changes nothing.
    pub async fn mark_correction_undone(
        &self,
        id: i64,
        at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mode_corrections SET undone_at = ?2 WHERE id = ?1 AND undone_at IS NULL",
        )
        .bind(id)
        .bind(at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// The newest sender correction still standing, for a sender.
    pub async fn latest_sender_correction(
        &self,
        account_id: &AccountId,
        sender_email: &str,
    ) -> Result<Option<Correction>, sqlx::Error> {
        let sql = format!(
            "SELECT {CORRECTION_COLUMNS} FROM mode_corrections
             WHERE account_id = ?1 AND sender_email = lower(?2) AND scope = 'sender'
               AND undone_at IS NULL
             ORDER BY created_at DESC, id DESC LIMIT 1"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_id.as_str())
            .bind(sender_email)
            .fetch_optional(self.reader())
            .await?
            .as_ref()
            .map(decode_correction)
            .transpose()
    }

    /// Corrections in `accounts`, newest first, undone ones included.
    pub async fn list_corrections(
        &self,
        accounts: &[AccountId],
        limit: u32,
    ) -> Result<Vec<Correction>, sqlx::Error> {
        let sql = format!(
            "SELECT {CORRECTION_COLUMNS} FROM mode_corrections
             WHERE account_id IN (SELECT value FROM json_each(?1))
             ORDER BY created_at DESC, id DESC LIMIT ?2"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(account_ids_json(accounts)?)
            .bind(i64::from(limit))
            .fetch_all(self.reader())
            .await?;
        rows.iter().map(decode_correction).collect()
    }

    /// Moves standing since `since`: corrections that changed a mode and
    /// were not undone. A sender move counts once, however much mail it
    /// moved.
    pub async fn count_moves_since(
        &self,
        accounts: &[AccountId],
        since: DateTime<Utc>,
    ) -> Result<u32, sqlx::Error> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mode_corrections
             WHERE account_id IN (SELECT value FROM json_each(?1))
               AND created_at >= ?2 AND undone_at IS NULL AND from_mode != to_mode",
        )
        .bind(account_ids_json(accounts)?)
        .bind(since.timestamp())
        .fetch_one(self.reader())
        .await?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    /// Whether the user has ever made a move that stands.
    pub async fn has_moves(&self, accounts: &[AccountId]) -> Result<bool, sqlx::Error> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM mode_corrections
             WHERE account_id IN (SELECT value FROM json_each(?1))
               AND undone_at IS NULL AND from_mode != to_mode",
        )
        .bind(account_ids_json(accounts)?)
        .fetch_one(self.reader())
        .await?;
        Ok(n > 0)
    }
}

#[cfg(test)]
mod tests;
