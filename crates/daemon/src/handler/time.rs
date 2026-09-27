//! `ResolveTime`: preview a natural-language time phrase.

use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::resolve_time;
use mxr_protocol::ResponseData;

use super::HandlerResult;
use crate::state::AppState;

/// Resolve in the caller's IANA zone when it sends one (a remote browser),
/// otherwise in the daemon's local zone.
pub(super) fn resolve(
    state: &AppState,
    input: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> HandlerResult {
    let now = now.unwrap_or_else(Utc::now);
    let prefs = state.snooze_time_prefs();
    let result = match time_zone {
        Some(name) => {
            let zone: chrono_tz::Tz = name.parse().map_err(|_| {
                format!("Unknown time zone \"{name}\". Use an IANA name like \"Europe/London\".")
            })?;
            resolve_time(input, &now.with_timezone(&zone), &prefs)
        }
        None => resolve_time(input, &now.with_timezone(&Local), &prefs),
    };
    Ok(ResponseData::resolved_time(input, result))
}
