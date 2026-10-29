//! Persistence for the mutation-undo log.
//!
//! Each row records the prior state of a batch of envelopes affected by a
//! recent destructive mutation, plus an `expires_at` timestamp. The daemon
//! reads a row when handling `Request::UndoMutation`, applies the reverse
//! op, and deletes the row.

use mxr_core::{AccountId, MessageId};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Coarse classification of an undoable mutation, used so the daemon
/// knows what reverse op to issue against the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoableMutationKind {
    Archive,
    Trash,
    Spam,
    SetRead,
    /// Star or unstar: each message's own prior star, from its flags.
    Star,
    ReadAndArchive,
    /// Move and label edits: restoring the prior label set reverses them.
    Labels,
    /// The desk's Done: labels and read state per message, plus the
    /// conversations' dismissals and any promise it resolved (`DeskUndo`).
    DeskDone,
    /// Reply later or "bring it back if no reply" at a time: only the
    /// flags and reminders it replaced (`DeskUndo`), no message changes.
    Deferral,
    /// Done in one mode: that mode's mark or ticked-off to-dos
    /// (`DeskUndo`), plus labels and read state when the last mode let go
    /// and the thread was archived.
    ModeDone,
    /// A Reading highlight removed or re-noted (`DeskUndo::highlights`);
    /// no message changes.
    Highlight,
}

/// Snapshot of a single envelope's state right before a mutation was
/// applied. The daemon uses this to restore the local store and to
/// derive the reverse provider op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntrySnapshot {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub provider_id: String,
    /// `MessageFlags::bits()` of the prior flags. Stored as a raw integer
    /// so the schema is forward-compatible with future flag additions.
    pub prior_flags_bits: u32,
    /// Provider label IDs the message had before the mutation. Restoring
    /// this set re-attaches the message to the right local labels and
    /// implies what reverse provider mutation to send.
    pub prior_label_provider_ids: Vec<String>,
    /// The mutation failed on this message, possibly after the provider
    /// applied it (the local copy may not show the change). Undo sends the
    /// reverse to the provider whatever the local diff says.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub uncertain: bool,
}

/// What the desk's Done changed beyond messages, as it was before.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeskUndo {
    /// Exact promise/task states changed together by Desk Done.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promise_resolutions: Vec<crate::PromiseResolution>,
    #[serde(default)]
    pub dismissals: Vec<crate::DeskDismissalPrior>,
    #[serde(default)]
    pub commitments: Vec<CommitmentPrior>,
    /// Messages whose reply-later flag Done cleared, with when each was set.
    /// Written by earlier versions; new entries use `reply_later_priors`.
    #[serde(default)]
    pub reply_later: Vec<(MessageId, chrono::DateTime<chrono::Utc>)>,
    /// Reply-later flags as they were before, time included (`None`: not
    /// flagged).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reply_later_priors: Vec<(MessageId, Option<crate::ReplyLaterState>)>,
    /// "Bring it back if no reply" reminders as they were before (`None`:
    /// there was none).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reminder_priors: Vec<(MessageId, Option<crate::ReminderState>)>,
    /// Per-mode done marks as they were before (`mode_done`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mode_done: Vec<crate::ModeDonePrior>,
    /// To-dos a mode's done ticked off, to reopen as they were.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todos_ticked: Vec<TodoTickPrior>,
    /// Reading highlights removed, or whose note was edited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<HighlightPrior>,
}

/// A Reading highlight before a removal or note edit, to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightPrior {
    pub row: crate::ReadingHighlightRow,
    /// Removed (insert it again) rather than edited (restore its note).
    pub removed: bool,
}

/// A to-do before a mode's done ticked it off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoTickPrior {
    pub id: String,
    /// Reopening puts this back, so an undo doesn't make the row the
    /// user's (a user's row never expires).
    pub user_edited: bool,
}

impl DeskUndo {
    pub fn is_empty(&self) -> bool {
        self.promise_resolutions.is_empty()
            && self.dismissals.is_empty()
            && self.commitments.is_empty()
            && self.reply_later.is_empty()
            && self.reply_later_priors.is_empty()
            && self.reminder_priors.is_empty()
            && self.mode_done.is_empty()
            && self.todos_ticked.is_empty()
            && self.highlights.is_empty()
    }
}

/// A promise's status before Done resolved it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitmentPrior {
    pub account_id: AccountId,
    pub id: String,
    pub status: crate::CommitmentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub mutation_id: String,
    pub kind: UndoableMutationKind,
    pub snapshots: Vec<UndoEntrySnapshot>,
    /// Set by the desk's Done; restored along with the snapshots.
    pub desk: Option<DeskUndo>,
    pub applied_at: i64,
    pub expires_at: i64,
}

/// Where an undo entry stands, for saying why a claim on it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoEntryState {
    Available,
    /// Another undo holds it.
    Claimed,
    Expired,
    /// Never written, already undone, or pruned.
    Missing,
}

fn undo_entry_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UndoEntry, sqlx::Error> {
    let kind_value: String = row.try_get("mutation_kind")?;
    // Re-add quotes so serde can decode the kebab-case unit variant.
    let kind: UndoableMutationKind =
        super::decode_json(&format!("\"{}\"", kind_value.replace('"', "")))?;
    let snapshots_json: String = row.try_get("prior_state_json")?;
    let snapshots: Vec<UndoEntrySnapshot> = super::decode_json(&snapshots_json)?;
    let desk = row
        .try_get::<Option<String>, _>("desk_state_json")?
        .map(|json| super::decode_json::<DeskUndo>(&json))
        .transpose()?;
    Ok(UndoEntry {
        mutation_id: row.try_get("mutation_id")?,
        kind,
        snapshots,
        desk,
        applied_at: row.try_get("applied_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

impl super::Store {
    /// Persist a batch undo entry. Idempotent on `mutation_id` — re-inserts
    /// of the same id replace the row (the caller controls id generation
    /// so this is only relevant for retries).
    pub async fn write_undo_entry(&self, entry: &UndoEntry) -> Result<(), sqlx::Error> {
        let snapshots_json = super::encode_json(&entry.snapshots)?;
        let kind_json = super::encode_json(&entry.kind)?;
        // `kind_json` includes serde-rendered quotes; strip them for a
        // clean column value (the round-trip via decode_json would
        // re-quote, but we want a plain TEXT value for grep-friendliness).
        let kind_value = kind_json.trim_matches('"').to_string();
        let desk_json = entry.desk.as_ref().map(super::encode_json).transpose()?;
        sqlx::query(
            "INSERT OR REPLACE INTO mutation_undo_log
             (mutation_id, mutation_kind, prior_state_json, desk_state_json, applied_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.mutation_id)
        .bind(kind_value)
        .bind(snapshots_json)
        .bind(desk_json)
        .bind(entry.applied_at)
        .bind(entry.expires_at)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Read an undo entry by id. Returns `Ok(None)` if the id is unknown
    /// (already consumed or never existed). The caller is responsible
    /// for the `expires_at` check.
    pub async fn read_undo_entry(
        &self,
        mutation_id: &str,
    ) -> Result<Option<UndoEntry>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT mutation_id, mutation_kind, prior_state_json, desk_state_json,
                    applied_at, expires_at
             FROM mutation_undo_log
             WHERE mutation_id = ?",
        )
        .bind(mutation_id)
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| undo_entry_from_row(&row)).transpose()
    }

    /// Take an undo entry to restore it. One conditional UPDATE: it succeeds
    /// only while nobody holds the entry and its window is open at `now`
    /// (unix seconds), so of any number of undos reaching one entry (a chunk
    /// undo and a whole-job undo) exactly one gets it. `None` when it is
    /// held, expired or gone; [`Self::undo_entry_state`] says which.
    ///
    /// The holder ends the claim with [`Self::consume_undo_entry`] (done),
    /// [`Self::release_undo_entry`] (left as it was) or
    /// [`Self::release_undo_entry_keeping`] (part done).
    pub async fn claim_undo_entry(
        &self,
        mutation_id: &str,
        claimant: &str,
        now: i64,
    ) -> Result<Option<UndoEntry>, sqlx::Error> {
        let row = sqlx::query(
            "UPDATE mutation_undo_log SET claimed_by = ?
             WHERE mutation_id = ? AND claimed_by IS NULL AND expires_at > ?
             RETURNING mutation_id, mutation_kind, prior_state_json, desk_state_json,
                       applied_at, expires_at",
        )
        .bind(claimant)
        .bind(mutation_id)
        .bind(now)
        .fetch_optional(self.writer())
        .await?;
        row.map(|row| undo_entry_from_row(&row)).transpose()
    }

    /// Why an entry could not be claimed at `now`.
    pub async fn undo_entry_state(
        &self,
        mutation_id: &str,
        now: i64,
    ) -> Result<UndoEntryState, sqlx::Error> {
        let row = sqlx::query(
            "SELECT claimed_by IS NOT NULL AS claimed, expires_at FROM mutation_undo_log
             WHERE mutation_id = ?",
        )
        .bind(mutation_id)
        .fetch_optional(self.reader())
        .await?;
        let Some(row) = row else {
            return Ok(UndoEntryState::Missing);
        };
        if row.try_get::<i64, _>("expires_at")? <= now {
            return Ok(UndoEntryState::Expired);
        }
        Ok(if row.try_get::<bool, _>("claimed")? {
            UndoEntryState::Claimed
        } else {
            UndoEntryState::Available
        })
    }

    /// The holder restored everything it could: drop the entry so it cannot
    /// be replayed. Only the holder's claim is consumed.
    pub async fn consume_undo_entry(
        &self,
        mutation_id: &str,
        claimant: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM mutation_undo_log WHERE mutation_id = ? AND claimed_by = ?")
            .bind(mutation_id)
            .bind(claimant)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Give the entry back unchanged. A no-op once the holder consumed or
    /// released it, so a caller can always end with it.
    pub async fn release_undo_entry(
        &self,
        mutation_id: &str,
        claimant: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE mutation_undo_log SET claimed_by = NULL
             WHERE mutation_id = ? AND claimed_by = ?",
        )
        .bind(mutation_id)
        .bind(claimant)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Give the entry back holding only `snapshots`: what a retry can still
    /// restore. Desk state is dropped, since it is restored once and first.
    pub async fn release_undo_entry_keeping(
        &self,
        mutation_id: &str,
        claimant: &str,
        snapshots: &[UndoEntrySnapshot],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE mutation_undo_log
             SET claimed_by = NULL, prior_state_json = ?, desk_state_json = NULL
             WHERE mutation_id = ? AND claimed_by = ?",
        )
        .bind(super::encode_json(&snapshots)?)
        .bind(mutation_id)
        .bind(claimant)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Release every entry `claimant` still holds, for an undo that ended
    /// without finishing one.
    pub async fn release_undo_claims_of(&self, claimant: &str) -> Result<u64, sqlx::Error> {
        let result =
            sqlx::query("UPDATE mutation_undo_log SET claimed_by = NULL WHERE claimed_by = ?")
                .bind(claimant)
                .execute(self.writer())
                .await?;
        Ok(result.rows_affected())
    }

    /// Release every claim. At daemon startup every holder is a process that
    /// died, so nothing is restoring these entries any more.
    pub async fn release_all_undo_claims(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE mutation_undo_log SET claimed_by = NULL WHERE claimed_by IS NOT NULL",
        )
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected())
    }

    /// Delete an undo entry by id. Used after a successful undo so the
    /// same `mutation_id` cannot be replayed.
    pub async fn delete_undo_entry(&self, mutation_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM mutation_undo_log WHERE mutation_id = ?")
            .bind(mutation_id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    /// Set `expires_at` on each of these entries; ids with no entry are
    /// skipped. A job's chunks are written with no expiry while it runs and
    /// all get the same one when it ends.
    pub async fn set_undo_entries_expiry(
        &self,
        mutation_ids: &[String],
        expires_at: i64,
    ) -> Result<u64, sqlx::Error> {
        let mut changed = 0;
        for chunk in mutation_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "UPDATE mutation_undo_log SET expires_at = ? WHERE mutation_id IN ({placeholders})"
            );
            let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(expires_at);
            for id in chunk {
                query = query.bind(id);
            }
            changed += query.execute(self.writer()).await?.rows_affected();
        }
        Ok(changed)
    }

    /// Drop every entry whose `expires_at` is at or before `now`. Bounds
    /// the table size under heavy mutation traffic.
    pub async fn prune_expired_undo_entries(&self, now: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM mutation_undo_log WHERE expires_at <= ?")
            .bind(now)
            .execute(self.writer())
            .await?;
        Ok(result.rows_affected())
    }
}
