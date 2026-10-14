//! Changing a saved highlight: its note, or removing it.
//!
//! A highlight read back from Marginalia is mirrored, not owned: the next
//! read-back would add a removed one again and overwrite an edited one's
//! text, so only mxr's own can change (`LOCAL_HIGHLIGHT_ORIGIN`). The dry
//! run and the real run both start from `own_highlight`, so a preview names
//! exactly the row the mutation will touch.

use super::item::{clean_note, highlight_data};
use super::{ensure_items, Issue};
use crate::handler::mutations::UNDO_WINDOW_SECS;
use crate::handler::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_protocol::{ReadingHighlightChangeData, ReadingHighlightData, ResponseData};
use mxr_store::{
    DeskUndo, HighlightEdit, HighlightPrior, ReadingHighlightRow, UndoEntry, UndoableMutationKind,
    LOCAL_HIGHLIGHT_ORIGIN,
};

const NOTE: &str = "note";
const REMOVE: &str = "remove";

pub(in crate::handler) async fn update_highlight(
    state: &AppState,
    id: &str,
    note: &str,
    dry_run: bool,
) -> HandlerResult {
    let note = clean_note(Some(note))?;
    let row = own_highlight(state, id).await?;
    if dry_run {
        return changed(state, NOTE, &with_note(row, note), None, true, None, false).await;
    }
    let prior = match state.store.set_reading_highlight_note(id, note).await? {
        HighlightEdit::Done(prior) => prior,
        HighlightEdit::Missing => return Err(gone(id)),
        HighlightEdit::ReadOnly(origin) => return Err(read_only(&origin)),
    };
    let after = with_note(prior.clone(), note);
    let previous_note = prior.note.clone();
    let (mutation_id, undo_unavailable) = record_undo(
        state,
        HighlightPrior {
            row: prior,
            removed: false,
        },
    )
    .await;
    reindex(state, &after).await;
    changed(
        state,
        NOTE,
        &after,
        previous_note,
        false,
        mutation_id,
        undo_unavailable,
    )
    .await
}

pub(in crate::handler) async fn remove_highlight(
    state: &AppState,
    id: &str,
    dry_run: bool,
) -> HandlerResult {
    let row = own_highlight(state, id).await?;
    if dry_run {
        return changed(state, REMOVE, &row, None, true, None, false).await;
    }
    let prior = match state.store.remove_reading_highlight(id).await? {
        HighlightEdit::Done(prior) => prior,
        HighlightEdit::Missing => return Err(gone(id)),
        HighlightEdit::ReadOnly(origin) => return Err(read_only(&origin)),
    };
    let (mutation_id, undo_unavailable) = record_undo(
        state,
        HighlightPrior {
            row: prior.clone(),
            removed: true,
        },
    )
    .await;
    reindex(state, &prior).await;
    changed(
        state,
        REMOVE,
        &prior,
        None,
        false,
        mutation_id,
        undo_unavailable,
    )
    .await
}

/// Search finds highlights through the message's semantic chunks, so a
/// change (or its undo) queues the message again.
async fn reindex(state: &AppState, row: &ReadingHighlightRow) {
    if let Err(error) = state
        .semantic
        .enqueue_ingest_messages(std::slice::from_ref(&row.message_id))
        .await
    {
        tracing::warn!(%error, "couldn't queue the highlight change for search");
    }
}

/// Undo: puts back what `update_highlight` and `remove_highlight` changed.
/// Returns how many are back; one whose message was deleted since can't be.
pub(in crate::handler) async fn restore_highlights(
    state: &AppState,
    priors: &[HighlightPrior],
) -> Result<u32, HandlerError> {
    let back = state.store.put_back_reading_highlights(priors).await?;
    for prior in priors {
        reindex(state, &prior.row).await;
    }
    Ok(back)
}

/// The highlight a change would touch, or why none may be touched. Shared
/// by the dry run and the real run.
async fn own_highlight(state: &AppState, id: &str) -> Result<ReadingHighlightRow, HandlerError> {
    let row = state
        .store
        .reading_highlight(id)
        .await?
        .ok_or_else(|| gone(id))?;
    if row.origin != LOCAL_HIGHLIGHT_ORIGIN {
        return Err(read_only(&row.origin));
    }
    Ok(row)
}

fn with_note(mut row: ReadingHighlightRow, note: Option<&str>) -> ReadingHighlightRow {
    row.note = note.map(str::to_string);
    row
}

fn gone(id: &str) -> HandlerError {
    HandlerError::Message(format!("No highlight {id}; it may already be removed"))
}

fn read_only(origin: &str) -> HandlerError {
    let place = if origin == "marginalia" {
        "Marginalia"
    } else {
        origin
    };
    HandlerError::InvalidRequest(format!("made in {place}; change it there"))
}

/// Writes the undo entry. A failure leaves the change in place, flagged so
/// the client can say it can't be undone.
async fn record_undo(state: &AppState, prior: HighlightPrior) -> (Option<String>, bool) {
    let mutation_id = uuid::Uuid::now_v7().to_string();
    let now = Utc::now().timestamp();
    let entry = UndoEntry {
        mutation_id: mutation_id.clone(),
        kind: UndoableMutationKind::Highlight,
        snapshots: Vec::new(),
        desk: Some(DeskUndo {
            highlights: vec![prior],
            ..DeskUndo::default()
        }),
        applied_at: now,
        expires_at: now + UNDO_WINDOW_SECS,
    };
    match state.store.write_undo_entry(&entry).await {
        Ok(()) => (Some(mutation_id), false),
        Err(error) => {
            tracing::warn!(%error, "failed to write the highlight undo entry");
            (None, true)
        }
    }
}

async fn changed(
    state: &AppState,
    action: &str,
    row: &ReadingHighlightRow,
    previous_note: Option<String>,
    dry_run: bool,
    mutation_id: Option<String>,
    undo_unavailable: bool,
) -> HandlerResult {
    Ok(ResponseData::ReadingHighlightChanged {
        change: ReadingHighlightChangeData {
            action: action.to_string(),
            highlight: describe(state, row).await?,
            previous_note,
            dry_run,
            mutation_id,
            undo_unavailable,
        },
    })
}

/// The highlight with its item's title, source and link.
async fn describe(
    state: &AppState,
    row: &ReadingHighlightRow,
) -> Result<ReadingHighlightData, HandlerError> {
    let envelope = state.store.get_envelope(&row.message_id).await?;
    let Some(envelope) = envelope else {
        return Ok(highlight_data(row, "", "", None));
    };
    let issue = Issue::from_envelope(&envelope);
    let items = ensure_items(state, &[&issue]).await?;
    let item = items
        .get(&row.message_id)
        .and_then(|rows| rows.iter().find(|r| r.idx == row.idx));
    Ok(highlight_data(
        row,
        item.map_or(&issue.subject, |r| &r.title),
        &issue.source_name,
        item.and_then(|r| r.url.clone()),
    ))
}
