//! How each mode explains itself (D118, amended 2026-10-07): the copy table
//! from the protocol plus which hints are dismissed on this profile. The
//! daemon keeps that state so a hint dismissed in one client never shows
//! in another.

use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_protocol::{
    hint, mode_guide, ModeGuideCopy, ResponseData, MODE_GUIDES, NEXT_TIME_HINT_PREFIX,
};

fn find(mode: &str) -> Result<&'static ModeGuideCopy, HandlerError> {
    mode_guide(mode).ok_or_else(|| {
        HandlerError::InvalidRequest(format!(
            "No mode \"{mode}\". Modes so far: {}.",
            MODE_GUIDES
                .iter()
                .map(|guide| guide.mode)
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

async fn guides(state: &AppState, chosen: Vec<&'static ModeGuideCopy>) -> HandlerResult {
    let seen = state.store.hints_seen().await?;
    Ok(ResponseData::ModeGuides {
        guides: chosen
            .into_iter()
            .map(|guide| guide.to_data(|id| seen.get(id).copied(), &mxr_keymap::guide_key))
            .collect(),
    })
}

pub(super) async fn get(state: &AppState, mode: Option<&str>) -> HandlerResult {
    let chosen = match mode {
        Some(mode) => vec![find(mode)?],
        None => MODE_GUIDES.iter().collect(),
    };
    guides(state, chosen).await
}

/// Answers with every mode the hint attaches in, so a client holding any
/// of those guides can take the new state from the answer.
pub(super) async fn set_seen(state: &AppState, id: &str, seen: bool) -> HandlerResult {
    if let Some(action) = id.strip_prefix(NEXT_TIME_HINT_PREFIX) {
        return next_time_seen(state, id, action, seen).await;
    }
    let Some(found) = hint(id) else {
        let known: Vec<&str> = MODE_GUIDES
            .iter()
            .flat_map(|guide| guide.hints)
            .map(|hint| hint.id)
            .collect();
        return Err(HandlerError::InvalidRequest(format!(
            "No hint \"{id}\". Hints: {}.",
            known.join(", ")
        )));
    };
    state
        .store
        .set_hint_seen(found.id, seen.then(Utc::now))
        .await?;
    let attached = MODE_GUIDES
        .iter()
        .filter(|guide| guide.hints.iter().any(|h| h.id == found.id))
        .collect();
    guides(state, attached).await
}

/// A "Next time" hint (D171): one per action in the key definition. The
/// answer says whether this request marked it, so the client that ran the
/// action first is the only one that names the key.
async fn next_time_seen(state: &AppState, id: &str, action: &str, seen: bool) -> HandlerResult {
    if mxr_keymap::defaults().get(action).is_none() {
        return Err(HandlerError::InvalidRequest(format!(
            "No action \"{action}\" in the key definition; `mxr keys` lists them."
        )));
    }
    let newly_seen = state.store.set_hint_seen(id, seen.then(Utc::now)).await?;
    Ok(ResponseData::HintSeen {
        hint: id.to_string(),
        newly_seen,
    })
}

/// Acting on the element a hint explains dismisses it: a hint about
/// something already done is noise. Used where the act reaches the daemon
/// from any client, so the CLI dismisses it too.
pub(super) async fn dismiss(state: &AppState, id: &'static str) -> Result<(), HandlerError> {
    debug_assert!(hint(id).is_some(), "unknown hint {id}");
    state.store.set_hint_seen(id, Some(Utc::now())).await?;
    Ok(())
}
