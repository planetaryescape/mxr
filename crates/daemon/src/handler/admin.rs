use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;

#[allow(clippy::too_many_arguments)]
pub(super) async fn list_events(
    state: &AppState,
    limit: u32,
    offset: u32,
    level: Option<&str>,
    category: Option<&str>,
    category_prefix: Option<&str>,
    since: Option<i64>,
    until: Option<i64>,
    search: Option<&str>,
) -> HandlerResult {
    diagnostics_impl::list_events(
        state,
        limit,
        offset,
        level,
        category,
        category_prefix,
        since,
        until,
        search,
    )
    .await
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn count_events(
    state: &AppState,
    level: Option<&str>,
    category: Option<&str>,
    category_prefix: Option<&str>,
    since: Option<i64>,
    until: Option<i64>,
    search: Option<&str>,
) -> HandlerResult {
    let filter = mxr_store::EventLogFilter {
        limit: 0,
        offset: 0,
        level,
        category,
        category_prefix,
        since,
        until,
        search,
    };
    let count = state
        .store
        .count_events_filtered(filter)
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
