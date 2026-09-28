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
    ReadAndArchive,
    /// Move and label edits: restoring the prior label set reverses them.
    Labels,
    /// The desk's Done: labels and read state per message, plus the
    /// conversations' dismissals and any promise it resolved (`DeskUndo`).
    DeskDone,
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
    #[serde(default)]
    pub dismissals: Vec<crate::DeskDismissalPrior>,
    #[serde(default)]
    pub commitments: Vec<CommitmentPrior>,
    /// Messages whose reply-later flag Done cleared, with when each was set.
    #[serde(default)]
    pub reply_later: Vec<(MessageId, chrono::DateTime<chrono::Utc>)>,
}

impl DeskUndo {
    pub fn is_empty(&self) -> bool {
        self.dismissals.is_empty() && self.commitments.is_empty() && self.reply_later.is_empty()
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
        let Some(row) = row else { return Ok(None) };

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
        Ok(Some(UndoEntry {
            mutation_id: row.try_get("mutation_id")?,
            kind,
            snapshots,
            desk,
            applied_at: row.try_get("applied_at")?,
            expires_at: row.try_get("expires_at")?,
        }))
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
