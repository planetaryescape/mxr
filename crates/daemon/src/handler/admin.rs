use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use mxr_store::EventLogFilter;

pub(super) async fn list_events(state: &AppState, filter: EventLogFilter<'_>) -> HandlerResult {
    diagnostics_impl::list_events(state, filter).await
}

pub(super) async fn get_logs(
    state: &AppState,
    limit: u32,
    level: Option<&str>,
    search: Option<&str>,
) -> HandlerResult {
    diagnostics_impl::get_logs(state, limit, level, search).await
}

pub(super) async fn list_event_categories(state: &AppState) -> HandlerResult {
    let categories = state
        .store
        .list_event_categories()
        .await
        .map_err(|e| e.to_string())?;
    Ok(mxr_protocol::ResponseData::EventCategories { categories })
}

pub(super) async fn count_events(state: &AppState, filter: EventLogFilter<'_>) -> HandlerResult {
    let count_filter = EventLogFilter {
        limit: 0,
        offset: 0,
        ..filter
    };
    let count = state
        .store
        .count_events_filtered(count_filter)
        .await
        .map_err(|e| e.to_string())?;
    Ok(mxr_protocol::ResponseData::EventLogCount { count })
}

pub(super) async fn doctor_report(state: &AppState) -> HandlerResult {
    diagnostics_impl::doctor_report(state).await
}

pub(super) async fn bug_report(
    verbose: bool,
    full_logs: bool,
    since: Option<String>,
) -> HandlerResult {
    diagnostics_impl::bug_report(verbose, full_logs, since).await
}

pub(super) async fn get_status(state: &AppState) -> HandlerResult {
    diagnostics_impl::get_status(state).await
}

pub(super) async fn shutdown(state: &AppState) -> HandlerResult {
    state.request_shutdown();
    Ok(mxr_protocol::ResponseData::Ack)
}
