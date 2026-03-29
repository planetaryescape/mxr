use super::{diagnostics_impl, HandlerResult};
use crate::mxr_core::id::{AccountId, MessageId, ThreadId};
use crate::mxr_core::types::{ExportFormat, SearchMode, SortOrder};
use crate::state::AppState;
use std::sync::Arc;

pub(super) async fn search(
    state: &Arc<AppState>,
    query: &str,
    limit: u32,
    offset: u32,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> HandlerResult {
    diagnostics_impl::search(state, query, limit, offset, mode, sort, explain).await
}

pub(super) async fn count(state: &Arc<AppState>, query: &str, mode: SearchMode) -> HandlerResult {
    diagnostics_impl::count(state, query, mode).await
}

pub(super) async fn get_headers(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    diagnostics_impl::get_headers(state, message_id).await
}

pub(super) async fn sync_now(
    state: &Arc<AppState>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    diagnostics_impl::sync_now(state, account_id).await
}

pub(super) async fn export_thread(
    state: &Arc<AppState>,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    diagnostics_impl::export_thread(state, thread_id, format).await
}

pub(super) async fn export_search(
    state: &Arc<AppState>,
    query: &str,
    format: &ExportFormat,
) -> HandlerResult {
    diagnostics_impl::export_search(state, query, format).await
}

pub(super) async fn get_sync_status(
    state: &Arc<AppState>,
    account_id: &AccountId,
) -> HandlerResult {
    diagnostics_impl::get_sync_status(state, account_id).await
}
