use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{ExportFormat, SearchMode, SortOrder};
use mxr_protocol::SearchAggregationGroupBy;

#[allow(clippy::too_many_arguments)]
pub(super) async fn search(
    state: &AppState,
    query: &str,
    limit: u32,
    offset: u32,
    account_id: Option<&AccountId>,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> HandlerResult {
    diagnostics_impl::search(state, query, limit, offset, account_id, mode, sort, explain).await
}

pub(super) async fn count(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
    mode: SearchMode,
) -> HandlerResult {
    diagnostics_impl::count(state, query, account_id, mode).await
}

pub(super) async fn search_aggregation(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
    mode: SearchMode,
    group_by: SearchAggregationGroupBy,
    limit: Option<u32>,
) -> HandlerResult {
    diagnostics_impl::search_aggregation(state, query, account_id, mode, group_by, limit).await
}

pub(super) async fn get_headers(state: &AppState, message_id: &MessageId) -> HandlerResult {
    diagnostics_impl::get_headers(state, message_id).await
}

pub(super) async fn sync_now(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    diagnostics_impl::sync_now(state, account_id).await
}

pub(super) async fn export_thread(
    state: &AppState,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    diagnostics_impl::export_thread(state, thread_id, format).await
}

pub(super) async fn export_search(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
    format: &ExportFormat,
) -> HandlerResult {
    diagnostics_impl::export_search(state, query, account_id, format).await
}

pub(super) async fn get_sync_status(state: &AppState, account_id: &AccountId) -> HandlerResult {
    diagnostics_impl::get_sync_status(state, account_id).await
}
