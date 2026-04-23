use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::{SearchMode, SemanticProfile};

pub(super) async fn list_saved_searches(state: &AppState) -> HandlerResult {
    diagnostics_impl::list_saved_searches(state).await
}

pub(super) async fn list_subscriptions(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_subscriptions(state, account_id, limit).await
}

pub(super) async fn semantic_status(state: &AppState) -> HandlerResult {
    diagnostics_impl::semantic_status(state).await
}

pub(super) async fn enable_semantic(state: &AppState, enabled: bool) -> HandlerResult {
    diagnostics_impl::enable_semantic(state, enabled).await
}

pub(super) async fn install_semantic_profile(
    state: &AppState,
    profile: SemanticProfile,
) -> HandlerResult {
    diagnostics_impl::install_semantic_profile(state, profile).await
}

pub(super) async fn use_semantic_profile(
    state: &AppState,
    profile: SemanticProfile,
) -> HandlerResult {
    diagnostics_impl::use_semantic_profile(state, profile).await
}

pub(super) async fn reindex_semantic(state: &AppState) -> HandlerResult {
    diagnostics_impl::reindex_semantic(state).await
}

pub(super) async fn create_saved_search(
    state: &AppState,
    name: &str,
    query: &str,
    search_mode: SearchMode,
) -> HandlerResult {
    diagnostics_impl::create_saved_search(state, name, query, search_mode).await
}

pub(super) async fn delete_saved_search(state: &AppState, name: &str) -> HandlerResult {
    diagnostics_impl::delete_saved_search(state, name).await
}

pub(super) async fn run_saved_search(state: &AppState, name: &str, limit: u32) -> HandlerResult {
    diagnostics_impl::run_saved_search(state, name, limit).await
}
