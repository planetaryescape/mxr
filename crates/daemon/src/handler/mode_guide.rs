//! How each mode explains itself (D118): the copy table from the protocol
//! plus whether the mode's first-encounter card is retired on this
//! profile. The daemon keeps that state so closing the card in one client
//! closes it in all of them.

use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_protocol::{mode_guide, ModeGuideCopy, ResponseData, MODE_GUIDES};

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

pub(super) async fn get(state: &AppState, mode: Option<&str>) -> HandlerResult {
    let seen = state.store.mode_guides_seen().await?;
    let guides = match mode {
        Some(mode) => vec![find(mode)?],
        None => MODE_GUIDES.iter().collect(),
    };
    Ok(ResponseData::ModeGuides {
        guides: guides
            .into_iter()
            .map(|guide| guide.to_data(seen.get(guide.mode).copied()))
            .collect(),
    })
}

pub(super) async fn set_seen(state: &AppState, mode: &str, seen: bool) -> HandlerResult {
    let guide = find(mode)?;
    state
        .store
        .set_mode_guide_seen(guide.mode, seen.then(Utc::now))
        .await?;
    get(state, Some(guide.mode)).await
}

/// Using a mode's main verb retires its card: a tip about something
/// already done is noise.
pub(super) async fn retire(state: &AppState, mode: &'static str) -> Result<(), HandlerError> {
    state
        .store
        .set_mode_guide_seen(mode, Some(Utc::now()))
        .await?;
    Ok(())
}
