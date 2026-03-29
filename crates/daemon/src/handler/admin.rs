use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use std::sync::Arc;

pub(super) async fn list_events(
    state: &Arc<AppState>,
    limit: u32,
    level: Option<&str>,
    category: Option<&str>,
) -> HandlerResult {
    diagnostics_impl::list_events(state, limit, level, category).await
}

pub(super) fn get_logs(limit: u32, level: Option<&str>) -> HandlerResult {
    diagnostics_impl::get_logs(limit, level)
}

pub(super) async fn doctor_report(state: &Arc<AppState>) -> HandlerResult {
    diagnostics_impl::doctor_report(state).await
}

pub(super) async fn bug_report(
    verbose: bool,
    full_logs: bool,
    since: Option<String>,
) -> HandlerResult {
    diagnostics_impl::bug_report(verbose, full_logs, since).await
}

pub(super) async fn get_status(state: &Arc<AppState>) -> HandlerResult {
    diagnostics_impl::get_status(state).await
}
