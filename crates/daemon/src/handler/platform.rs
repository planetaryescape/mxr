use super::{diagnostics_impl, HandlerResult};
use crate::state::AppState;
use mxr_core::id::AccountId;
use mxr_core::types::{
    ResponseTimeDirection, SearchMode, SemanticProfile, StaleBallInCourt, StorageGroupBy,
};

pub(super) async fn list_saved_searches(state: &AppState) -> HandlerResult {
    diagnostics_impl::list_saved_searches(state).await
}

/// Run each saved search's query intersected with `is:unread` and
/// return a map of saved-search id to match count. Queries that fail
/// to parse return 0 — the tab strip surface MUST not silently
/// disappear because one saved search has a malformed expression.
pub(super) async fn list_saved_search_unread_counts(state: &AppState) -> HandlerResult {
    use std::collections::HashMap;

    let saved = state.store.list_saved_searches().await?;
    let mut counts = HashMap::with_capacity(saved.len());
    for entry in saved {
        let combined = if entry.query.trim().is_empty() {
            "is:unread".to_string()
        } else {
            format!("({}) AND is:unread", entry.query)
        };
        match diagnostics_impl::count_search_matches(state, &combined, entry.search_mode).await {
            Ok(count) => {
                counts.insert(entry.id, count);
            }
            Err(error) => {
                // Don't poison the whole response — record a zero
                // for this saved search and log the failure.
                tracing::debug!(
                    saved_search_id = %entry.id,
                    %error,
                    "unread-count query failed; reporting 0"
                );
                counts.insert(entry.id, 0);
            }
        }
    }
    Ok(mxr_protocol::ResponseData::SavedSearchUnreadCounts { counts })
}

pub(super) async fn list_subscriptions(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_subscriptions(state, account_id, limit).await
}

pub(super) async fn list_senders(
    state: &AppState,
    limit: u32,
    since_unix: Option<i64>,
) -> HandlerResult {
    let senders = state
        .store
        .list_top_senders_since(limit, since_unix)
        .await?
        .into_iter()
        .map(|row| mxr_protocol::SenderSummaryData {
            account_id: row.account_id,
            display_name: row.display_name,
            sender_email: row.sender_email,
            message_count: row.message_count,
            unread_count: row.unread_count,
            latest_subject: row.latest_subject,
            latest_at: row.latest_at,
        })
        .collect();
    Ok(mxr_protocol::ResponseData::Senders { senders })
}

pub(super) async fn list_storage_breakdown(
    state: &AppState,
    account_id: Option<&AccountId>,
    group_by: StorageGroupBy,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_storage_breakdown(state, account_id, group_by, limit).await
}

pub(super) async fn list_largest_messages(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_days: Option<u32>,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_largest_messages(state, account_id, since_days, limit).await
}

pub(super) async fn wrapped(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_unix: i64,
    until_unix: i64,
    label: &str,
) -> HandlerResult {
    diagnostics_impl::wrapped(state, account_id, since_unix, until_unix, label).await
}

pub(super) async fn list_stale_threads(
    state: &AppState,
    account_id: Option<&AccountId>,
    perspective: StaleBallInCourt,
    older_than_days: u32,
    within_days: u32,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_stale_threads(
        state,
        account_id,
        perspective,
        older_than_days,
        within_days,
        limit,
    )
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
    max_lookback_days: u32,
    limit: u32,
) -> HandlerResult {
    diagnostics_impl::list_contact_decay(
        state,
        account_id,
        threshold_days,
        max_lookback_days,
        limit,
    )
    .await
}

pub(super) async fn refresh_contacts(state: &AppState) -> HandlerResult {
    diagnostics_impl::refresh_contacts(state).await
}

pub(super) async fn rebuild_analytics(state: &AppState) -> HandlerResult {
    diagnostics_impl::rebuild_analytics(state).await
}

pub(super) async fn recompute_link_counts(state: &AppState) -> HandlerResult {
    diagnostics_impl::recompute_link_counts(state).await
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

pub(super) async fn llm_status(state: &AppState) -> HandlerResult {
    diagnostics_impl::llm_status(state).await
}

pub(super) async fn llm_config(state: &AppState) -> HandlerResult {
    diagnostics_impl::llm_config(state).await
}

pub(super) async fn update_llm_config(
    state: &AppState,
    config: mxr_protocol::LlmConfigData,
) -> HandlerResult {
    diagnostics_impl::update_llm_config(state, config).await
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

pub(super) async fn backfill_semantic(state: &AppState) -> HandlerResult {
    diagnostics_impl::backfill_semantic(state).await
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

pub(super) async fn update_saved_search(
    state: &AppState,
    name: &str,
    update: mxr_store::SavedSearchUpdate<'_>,
) -> HandlerResult {
    diagnostics_impl::update_saved_search(state, name, update).await
}

pub(super) async fn run_saved_search(state: &AppState, name: &str, limit: u32) -> HandlerResult {
    diagnostics_impl::run_saved_search(state, name, limit).await
}
