//! `ResolveTime`: preview a natural-language time phrase.

use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::resolve_time;
use mxr_protocol::ResponseData;

use super::HandlerResult;
use crate::state::AppState;

pub(super) fn resolve(state: &AppState, input: &str, now: Option<DateTime<Utc>>) -> HandlerResult {
    let now = now.unwrap_or_else(Utc::now).with_timezone(&Local);
    Ok(ResponseData::resolved_time(
        input,
        resolve_time(input, &now, &state.snooze_time_prefs()),
    ))
}
