use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::{
    ResponseTimeDirection, SearchMode, SemanticProfile, StaleBallInCourt, StorageGroupBy,
};

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

pub(super) async fn list_storage_breakdown(
    state: &AppState,
    account_id: Option<&AccountId>,
    group_by: StorageGroupBy,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_storage_breakdown(state, account_id, group_by, limit).await
}

pub(super) async fn list_stale_threads(
    state: &AppState,
    account_id: Option<&AccountId>,
    perspective: StaleBallInCourt,
    older_than_days: u32,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_stale_threads(state, account_id, perspective, older_than_days, limit)
        .await
}

pub(super) async fn list_contact_asymmetry(
    state: &AppState,
    account_id: Option<&AccountId>,
    min_inbound: u32,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_contact_asymmetry(state, account_id, min_inbound, limit).await
}

pub(super) async fn list_contact_decay(
    state: &AppState,
    account_id: Option<&AccountId>,
    threshold_days: u32,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_contact_decay(state, account_id, threshold_days, limit).await
}

pub(super) async fn refresh_contacts(state: &AppState) -> HandlerResult {
    diagnostics_impl::refresh_contacts(state).await
}

pub(super) async fn rebuild_analytics(state: &AppState) -> HandlerResult {
    diagnostics_impl::rebuild_analytics(state).await
}

pub(super) async fn list_response_time(
    state: &AppState,
    account_id: Option<&AccountId>,
    direction: ResponseTimeDirection,
    counterparty: Option<&str>,
    since_days: Option<u32>,
) -> HandlerResult {
    diagnostics_impl::list_response_time(state, account_id, direction, counterparty, since_days)
        .await
}

pub(super) async fn list_account_addresses(
    state: &AppState,
    account_id: &AccountId,
) -> HandlerResult {
    diagnostics_impl::list_account_addresses(state, account_id).await
}

pub(super) async fn add_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    primary: bool,
) -> HandlerResult {
    diagnostics_impl::add_account_address(state, account_id, email, primary).await
}

pub(super) async fn remove_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    diagnostics_impl::remove_account_address(state, account_id, email).await
}

pub(super) async fn set_primary_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    diagnostics_impl::set_primary_account_address(state, account_id, email).await
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
