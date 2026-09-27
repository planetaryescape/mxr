//! `ResolveTime`: preview a natural-language time phrase.

use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::{resolve_time, TimeResolution, TimeResolveError};
use mxr_protocol::ResponseData;

use super::{HandlerError, HandlerResult};
use crate::state::AppState;

/// Resolve in the caller's IANA zone when it sends one (a remote browser),
/// otherwise in the daemon's local zone.
pub(super) fn resolve(
    state: &AppState,
    input: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> HandlerResult {
    let result = resolve_in_zone(state, input, now, time_zone)?;
    Ok(ResponseData::resolved_time(input, result))
}

/// The one resolution every daemon-side phrase goes through. The outer error
/// is an unknown zone; the inner result is the parser's answer.
pub(super) fn resolve_in_zone(
    state: &AppState,
    input: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> Result<Result<TimeResolution, TimeResolveError>, HandlerError> {
    let now = now.unwrap_or_else(Utc::now);
    let prefs = state.snooze_time_prefs();
    Ok(match parse_zone(time_zone)? {
        Some(zone) => resolve_time(input, &now.with_timezone(&zone), &prefs),
        None => resolve_time(input, &now.with_timezone(&Local), &prefs),
    })
}

/// The caller's IANA zone; `None` means the daemon's local zone.
pub(super) fn parse_zone(time_zone: Option<&str>) -> Result<Option<chrono_tz::Tz>, HandlerError> {
    time_zone
        .map(|name| {
            name.parse().map_err(|_| {
                format!("Unknown time zone \"{name}\". Use an IANA name like \"Europe/London\".")
                    .into()
            })
        })
        .transpose()
}
