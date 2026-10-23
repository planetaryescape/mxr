//! Upgrade a bounded batch before new automatic identities can be inserted.
use super::HandlerError;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use mxr_core::AccountId;

pub(super) async fn page(
    state: &AppState,
    account: &AccountId,
    now: DateTime<Utc>,
) -> Result<bool, HandlerError> {
    let legacy = state.store.legacy_asks_todos(account).await?;
    if legacy.is_empty() {
        return Ok(true);
    }
    let ids: Vec<_> = legacy
        .iter()
        .filter_map(|r| r.source_message_id.clone())
        .collect();
    super::assessments::reconcile_messages(state, account, &ids).await?;
    let answers = state.store.asks_for_messages(account, &ids).await?;
    let envelopes = state.store.list_envelopes_by_ids(&ids).await?;
    let rows = state.store.list_todo_scan_rows(&ids).await?;
    let cfg = super::todos::pass_config(state, now);
    for old in &legacy {
        let Some(id) = &old.source_message_id else {
            state.store.upgrade_asks_todo(old, None, None).await?;
            continue;
        };
        let Some(answer) = answers.get(id).filter(|a| !a.pending_body) else {
            continue;
        };
        let Some(envelope) = envelopes.iter().find(|e| &e.id == id) else {
            continue;
        };
        let Some(row) = rows.iter().find(|r| &r.id == id) else {
            continue;
        };
        let candidates: Vec<_> = mxr_todo::asks::items::detections(answer, envelope)
            .into_iter()
            .filter(|(_, detection)| detection.kind.as_str() == old.kind)
            .collect();
        if candidates.len() > 1 {
            continue;
        }
        let replacement = candidates
            .into_iter()
            .next()
            .map(|(_, detection)| mxr_todo::pass::stored_record(&cfg, row, detection, false).0);
        state
            .store
            .upgrade_asks_todo(old, replacement.as_ref(), Some(answer))
            .await?;
    }
    Ok(false)
}
