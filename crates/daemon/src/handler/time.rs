use chrono::{DateTime, Local, Utc};
use mxr_core::natural_time::{due_words, resolve_time, DueWords, TimeResolution, TimeResolveError};
use mxr_protocol::ResponseData;

use super::{HandlerError, HandlerResult};
use crate::state::AppState;

pub(super) fn resolve(
    state: &AppState,
    input: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> HandlerResult {
    let result = resolve_in_zone(state, input, now, time_zone)?;
    Ok(ResponseData::resolved_time(input, result))
}

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

pub(super) fn due_words_in_zone(
    state: &AppState,
    source: &str,
    model_words: &str,
    now: Option<DateTime<Utc>>,
    time_zone: Option<&str>,
) -> Result<Option<DueWords>, HandlerError> {
    let now = now.unwrap_or_else(Utc::now);
    let prefs = state.snooze_time_prefs();
    Ok(match parse_zone(time_zone)? {
        Some(zone) => due_words(source, model_words, &now.with_timezone(&zone), &prefs),
        None => due_words(source, model_words, &now.with_timezone(&Local), &prefs),
    })
}

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
