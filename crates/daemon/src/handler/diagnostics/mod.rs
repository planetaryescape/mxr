mod search_execute;
mod search_filter;

use search_execute::execute_search;

use super::status_helpers::{
    build_account_sync_status, collect_doctor_report, collect_status_snapshot,
    feature_health_report,
};
use super::{
    handle_export_search, handle_export_thread, helpers::protocol_event_entry, HandlerResult,
};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{ExportFormat, SearchMode, SemanticProfile, SortOrder};
use mxr_protocol::IPC_PROTOCOL_VERSION;
use mxr_protocol::{
    DaemonEvent, IpcMessage, IpcPayload, LlmConfigData, LlmOverrideData, LlmOverridesData,
    ResponseData, SearchExplain, SearchExplainResult, SearchResultItem,
};
use mxr_search::{SearchPage, SearchResult};
use std::collections::HashMap;

#[derive(Debug)]
struct SearchExecution {
    results: Vec<SearchResult>,
    total: usize,
    has_more: bool,
    next_offset: Option<usize>,
    executed_mode: SearchMode,
    explain: Option<SearchExplain>,
}

struct ExecutionExplainInput<'a> {
    include_explain: bool,
    semantic_query: Option<String>,
    lexical_window: usize,
    dense_window: Option<usize>,
    lexical_results: &'a [SearchResult],
    dense_results: &'a [SearchResult],
    rrf_k: Option<usize>,
    notes: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn list_events(
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
    let filter = mxr_store::EventLogFilter {
        limit,
        offset,
        level,
        category,
        category_prefix,
        since,
        until,
        search,
    };
    let entries = state
        .store
        .list_events_filtered(filter)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::EventLogEntries {
        entries: entries.into_iter().map(protocol_event_entry).collect(),
    })
}

pub(crate) async fn get_logs(
    state: &AppState,
    limit: u32,
    level: Option<&str>,
    search: Option<&str>,
) -> HandlerResult {
    let lines =
        super::helpers::recent_log_lines_filtered(state, limit as usize, level, search).await?;
    Ok(ResponseData::LogLines { lines })
}

pub(crate) async fn doctor_report(state: &AppState) -> HandlerResult {
    let report = collect_doctor_report(state).await?;
    Ok(ResponseData::DoctorReport { report })
}

pub(crate) async fn bug_report(
    verbose: bool,
    full_logs: bool,
    since: Option<String>,
) -> HandlerResult {
    let content = crate::commands::bug_report::generate_report_markdown(
        &crate::commands::bug_report::BugReportOptions {
            edit: false,
            stdout: false,
            clipboard: false,
            github: false,
            output: None,
            verbose,
            full_logs,
            no_sanitize: false,
            since,
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(ResponseData::BugReport { content })
}

pub(crate) async fn search(
    state: &AppState,
    query: &str,
    limit: u32,
    offset: u32,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> HandlerResult {
    let execution = execute_search(
        state,
        query,
        limit as usize,
        offset as usize,
        mode,
        sort,
        explain,
    )
    .await?;
    Ok(ResponseData::SearchResults {
        results: search_result_items(execution.results, execution.executed_mode),
        total: execution.total as u32,
        has_more: execution.has_more,
        next_offset: execution.next_offset.map(|value| value as u32),
        explain: execution.explain,
    })
}

pub(crate) async fn count(state: &AppState, query: &str, mode: SearchMode) -> HandlerResult {
    let results = execute_search(state, query, 10_000, 0, mode, SortOrder::DateDesc, false).await;
    Ok(ResponseData::Count {
        count: results.map_err(|e| e.to_string())?.results.len() as u32,
    })
}

/// Return just the count of matches for a query — used by surfaces
/// that need a number, not the results themselves (e.g. saved-search
/// unread badges).
pub(crate) async fn count_search_matches(
    state: &AppState,
    query: &str,
    mode: SearchMode,
) -> Result<u32, String> {
    let execution =
        execute_search(state, query, 10_000, 0, mode, SortOrder::DateDesc, false).await?;
    Ok(execution.results.len() as u32)
}

pub(crate) async fn get_headers(state: &AppState, message_id: &MessageId) -> HandlerResult {
    match state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(envelope) => {
            let mut headers = Vec::new();
            headers.push((
                "From".to_string(),
                format!(
                    "{} <{}>",
                    envelope.from.name.as_deref().unwrap_or(""),
                    envelope.from.email
                ),
            ));
            headers.push(("Subject".to_string(), envelope.subject.clone()));
            headers.push(("Date".to_string(), envelope.date.to_rfc3339()));
            for addr in &envelope.to {
                headers.push((
                    "To".to_string(),
                    format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                ));
            }
            for addr in &envelope.cc {
                headers.push((
                    "Cc".to_string(),
                    format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                ));
            }
            if let Some(mid) = &envelope.message_id_header {
                headers.push(("Message-ID".to_string(), mid.clone()));
            }
            if let Some(irt) = &envelope.in_reply_to {
                headers.push(("In-Reply-To".to_string(), irt.clone()));
            }
            if let Ok(Some(body)) = state.store.get_body(message_id).await {
                if let Some(list_id) = body.metadata.list_id {
                    headers.push(("List-Id".to_string(), list_id));
                }
                for auth_result in body.metadata.auth_results {
                    headers.push(("Authentication-Results".to_string(), auth_result));
                }
                if !body.metadata.content_language.is_empty() {
                    headers.push((
                        "Content-Language".to_string(),
                        body.metadata.content_language.join(", "),
                    ));
                }
            }
            Ok(ResponseData::Headers { headers })
        }
        None => Err("Not found".to_string()),
    }
}

pub(crate) async fn list_saved_searches(state: &AppState) -> HandlerResult {
    let searches = state
        .store
        .list_saved_searches()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SavedSearches { searches })
}

pub(crate) async fn list_subscriptions(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: u32,
) -> HandlerResult {
    // Resolve to default account if not specified
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let subscriptions = state
        .store
        .list_subscriptions(resolved.as_ref(), limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Subscriptions { subscriptions })
}

pub(crate) async fn list_storage_breakdown(
    state: &AppState,
    account_id: Option<&AccountId>,
    group_by: mxr_core::types::StorageGroupBy,
    limit: u32,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let rows = state
        .store
        .storage_breakdown(resolved.as_ref(), group_by, limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::StorageBreakdown { rows })
}

pub(crate) async fn list_largest_messages(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_days: Option<u32>,
    limit: u32,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let since_unix = since_days.map(|d| chrono::Utc::now().timestamp() - i64::from(d) * 86_400);
    let rows = state
        .store
        .largest_messages(resolved.as_ref(), since_unix, limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::LargestMessages { rows })
}

pub(crate) async fn wrapped(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_unix: i64,
    until_unix: i64,
    label: &str,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let cache_key = crate::state::WrappedCacheKey {
        account_id: resolved.clone(),
        label: label.to_string(),
    };
    if let Some(cached) = state.wrapped_cache_get(&cache_key) {
        return Ok(ResponseData::Wrapped {
            summary: (*cached).clone(),
        });
    }
    let summary = state
        .store
        .wrapped_summary(resolved.as_ref(), since_unix, until_unix, label)
        .await
        .map_err(|e| e.to_string())?;
    state.wrapped_cache_put(cache_key, std::sync::Arc::new(summary.clone()));
    Ok(ResponseData::Wrapped { summary })
}

pub(crate) async fn list_stale_threads(
    state: &AppState,
    account_id: Option<&AccountId>,
    perspective: mxr_core::types::StaleBallInCourt,
    older_than_days: u32,
    within_days: u32,
    limit: u32,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let now = chrono::Utc::now().timestamp();
    let older_than_unix = now - i64::from(older_than_days) * 86_400;
    let within_unix = now - i64::from(within_days) * 86_400;
    let rows = state
        .store
        .list_stale_threads(
            resolved.as_ref(),
            perspective,
            older_than_unix,
            within_unix,
            limit,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::StaleThreads { rows })
}

pub(crate) async fn list_contact_asymmetry(
    state: &AppState,
    account_id: Option<&AccountId>,
    min_inbound: u32,
    limit: u32,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let rows = state
        .store
        .list_contact_asymmetry(resolved.as_ref(), min_inbound, limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::ContactAsymmetry { rows })
}

pub(crate) async fn list_contact_decay(
    state: &AppState,
    account_id: Option<&AccountId>,
    threshold_days: u32,
    max_lookback_days: u32,
    limit: u32,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let rows = state
        .store
        .list_contact_decay(resolved.as_ref(), threshold_days, max_lookback_days, limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::ContactDecay { rows })
}

pub(crate) async fn refresh_contacts(state: &AppState) -> HandlerResult {
    let rows = state
        .store
        .refresh_contacts()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::RefreshedContacts { rows })
}

/// Walk every message body, recompute `link_count` + `body_word_count` using
/// the current `mxr_sync::links` extractor, and persist back. Used by
/// `mxr doctor --recompute-link-counts` to backfill rows synced before the
/// link-extractor existed.
///
/// Single-pass, paginated; avoids loading every message into memory.
pub(crate) async fn recompute_link_counts(state: &AppState) -> HandlerResult {
    let mut offset: u32 = 0;
    let page_size: u32 = 200;
    let mut updated: u32 = 0;
    loop {
        let envelopes = state
            .store
            .list_all_envelopes_paginated(page_size, offset)
            .await
            .map_err(|e| e.to_string())?;
        if envelopes.is_empty() {
            break;
        }
        for envelope in &envelopes {
            let body = match state.store.get_body(&envelope.id).await {
                Ok(Some(body)) => body,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(message_id = %envelope.id, "recompute_link_counts: get_body failed: {error}");
                    continue;
                }
            };
            let metrics = mxr_sync::links::body_link_metrics(&body);
            if let Err(error) = state
                .store
                .update_link_metrics(&envelope.id, metrics.link_count, metrics.body_word_count)
                .await
            {
                tracing::warn!(message_id = %envelope.id, "recompute_link_counts: update failed: {error}");
                continue;
            }
            updated += 1;
        }
        if (envelopes.len() as u32) < page_size {
            break;
        }
        offset += page_size;
    }
    tracing::info!(updated, "recompute_link_counts complete");
    Ok(ResponseData::Ack)
}

pub(crate) async fn rebuild_analytics(state: &AppState) -> HandlerResult {
    use mxr_core::types::AccountAddressLookup;
    // The handler runs six sequential SQL passes. Each emits an
    // `OperationProgress` event (with `current`/`total = 6`) so
    // clients (CLI spinner, TUI status bar, `mxr events` tail) can
    // show live per-step feedback instead of blocking blind on a
    // single `AnalyticsRebuildSummary` response.
    let operation_id = uuid::Uuid::now_v7().to_string();
    let operation = "rebuild-analytics".to_string();
    const TOTAL_STEPS: u32 = 6;

    emit_operation_event(
        state,
        DaemonEvent::OperationStarted {
            operation_id: operation_id.clone(),
            operation: operation.clone(),
            account_id: None,
            message: "Starting analytics rebuild".to_string(),
        },
    );

    // Refresh address cache so reclassification has the latest set.
    state.refresh_account_addresses().await;
    let lookup = state.account_addresses.clone();

    let progress = |current: u32, message: String| DaemonEvent::OperationProgress {
        operation_id: operation_id.clone(),
        operation: operation.clone(),
        account_id: None,
        current,
        total: Some(TOTAL_STEPS),
        message,
    };
    let fail = |error: &str, retryable: bool| DaemonEvent::OperationFailed {
        operation_id: operation_id.clone(),
        operation: operation.clone(),
        account_id: None,
        error: error.to_string(),
        retryable,
    };

    emit_operation_event(
        state,
        progress(1, "Reclassifying unknown directions".into()),
    );
    let directions_reclassified = match state
        .store
        .reclassify_unknown_directions(|email| lookup.is_account_address(email))
        .await
    {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };

    emit_operation_event(state, progress(2, "Backfilling message list_ids".into()));
    let list_ids_backfilled = match state.store.backfill_message_list_ids().await {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };

    // Backfill reply_pairs from already-stored messages — the sync
    // hook only fires going forward, so existing data needs a
    // one-time scan.
    emit_operation_event(
        state,
        progress(3, "Backfilling reply pairs from messages".into()),
    );
    let backfilled = match state.store.backfill_reply_pairs_from_messages().await {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };

    emit_operation_event(state, progress(4, "Reconciling pending reply pairs".into()));
    let pending_resolved = match state.store.reconcile_reply_pair_pending().await {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };
    let reply_pairs_resolved = backfilled + pending_resolved;

    emit_operation_event(
        state,
        progress(5, "Backfilling business-hours latency".into()),
    );
    let business_hours_backfilled = match state.store.backfill_business_hours_latency().await {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };

    emit_operation_event(state, progress(6, "Refreshing contacts table".into()));
    let contacts_rows = match state.store.refresh_contacts().await {
        Ok(n) => n,
        Err(e) => {
            let err = e.to_string();
            emit_operation_event(state, fail(&err, true));
            return Err(err);
        }
    };

    emit_operation_event(
        state,
        DaemonEvent::OperationCompleted {
            operation_id,
            operation,
            account_id: None,
            message: format!(
                "Rebuild complete: {directions_reclassified} directions, \
                 {list_ids_backfilled} list_ids, {reply_pairs_resolved} reply pairs, \
                 {business_hours_backfilled} business-hours, {contacts_rows} contacts"
            ),
        },
    );

    Ok(ResponseData::AnalyticsRebuildSummary {
        directions_reclassified,
        list_ids_backfilled,
        reply_pairs_resolved,
        business_hours_backfilled,
        contacts_rows,
    })
}

pub(crate) async fn list_response_time(
    state: &AppState,
    account_id: Option<&AccountId>,
    direction: mxr_core::types::ResponseTimeDirection,
    counterparty: Option<&str>,
    since_days: Option<u32>,
) -> HandlerResult {
    let resolved = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt());
    let summary = state
        .store
        .list_response_time(resolved.as_ref(), direction, counterparty, since_days)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::ResponseTime { summary })
}

pub(crate) async fn list_account_addresses(
    state: &AppState,
    account_id: &AccountId,
) -> HandlerResult {
    let addresses = state
        .store
        .list_account_addresses(account_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::AccountAddresses { addresses })
}

pub(crate) async fn add_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
    primary: bool,
) -> HandlerResult {
    state
        .store
        .add_account_address(account_id, email, primary)
        .await
        .map_err(|e| e.to_string())?;
    state.refresh_account_addresses().await;
    Ok(ResponseData::Ack)
}

pub(crate) async fn remove_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    state
        .store
        .remove_account_address(account_id, email)
        .await
        .map_err(|e| e.to_string())?;
    state.refresh_account_addresses().await;
    Ok(ResponseData::Ack)
}

pub(crate) async fn set_primary_account_address(
    state: &AppState,
    account_id: &AccountId,
    email: &str,
) -> HandlerResult {
    state
        .store
        .set_primary_address(account_id, email)
        .await
        .map_err(|e| e.to_string())?;
    state.refresh_account_addresses().await;
    Ok(ResponseData::Ack)
}

pub(crate) async fn semantic_status(state: &AppState) -> HandlerResult {
    let snapshot = state
        .semantic
        .status_snapshot()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SemanticStatus { snapshot })
}

pub(crate) async fn llm_status(state: &AppState) -> HandlerResult {
    let config = state.config_snapshot().llm;
    let capabilities = state.llm.capabilities();
    let api_key_env = (!config.api_key_env.is_empty()).then_some(config.api_key_env.clone());
    let api_key_present = api_key_env
        .as_deref()
        .and_then(|name| std::env::var(name).ok())
        .is_some_and(|value| !value.is_empty());
    let snapshot = mxr_protocol::LlmStatusSnapshot {
        enabled: config.enabled,
        provider: if config.enabled {
            "openai_compatible".to_string()
        } else {
            "noop".to_string()
        },
        model: state.llm.model_name(),
        configured_model: config.model,
        base_url: config.enabled.then_some(config.base_url),
        api_key_env,
        api_key_present,
        context_window: capabilities.context_window,
        supports_streaming: capabilities.supports_streaming,
        request_timeout_secs: config.request_timeout_secs,
    };
    Ok(ResponseData::LlmStatus { snapshot })
}

pub(crate) async fn llm_config(state: &AppState) -> HandlerResult {
    Ok(ResponseData::LlmConfig {
        config: llm_config_data(state.config_snapshot().llm),
    })
}

pub(crate) async fn update_llm_config(state: &AppState, config: LlmConfigData) -> HandlerResult {
    let config = normalize_llm_config(config, &state.config_snapshot().llm)?;
    let saved = state
        .mutate_config(|current| {
            current.llm = config;
        })
        .await?;
    Ok(ResponseData::LlmConfig {
        config: llm_config_data(saved.llm),
    })
}

fn llm_config_data(config: mxr_config::LlmConfig) -> LlmConfigData {
    LlmConfigData {
        enabled: config.enabled,
        base_url: config.base_url,
        model: config.model,
        api_key_env: config.api_key_env,
        context_window: config.context_window,
        request_timeout_secs: config.request_timeout_secs,
        allow_cloud_relationship_data: config.allow_cloud_relationship_data,
        overrides: Some(llm_overrides_data(config.overrides)),
    }
}

fn llm_overrides_data(overrides: mxr_config::LlmOverrides) -> LlmOverridesData {
    LlmOverridesData {
        summarize: overrides.summarize.map(llm_override_data),
        relationship_summary: overrides.relationship_summary.map(llm_override_data),
        commitments: overrides.commitments.map(llm_override_data),
        draft_assist: overrides.draft_assist.map(llm_override_data),
        draft_new: overrides.draft_new.map(llm_override_data),
        draft_refine: overrides.draft_refine.map(llm_override_data),
        voice_match: overrides.voice_match.map(llm_override_data),
        humanize_rewrite: overrides.humanize_rewrite.map(llm_override_data),
        answer_coverage: overrides.answer_coverage.map(llm_override_data),
        archive_ask: overrides.archive_ask.map(llm_override_data),
        decision_log: overrides.decision_log.map(llm_override_data),
        briefing: overrides.briefing.map(llm_override_data),
        expert: overrides.expert.map(llm_override_data),
    }
}

fn llm_override_data(config: mxr_config::LlmOverrideConfig) -> LlmOverrideData {
    LlmOverrideData {
        enabled: config.enabled,
        base_url: config.base_url,
        model: config.model,
        api_key_env: config.api_key_env,
        context_window: config.context_window,
        request_timeout_secs: config.request_timeout_secs,
    }
}

fn normalize_llm_config(
    config: LlmConfigData,
    current: &mxr_config::LlmConfig,
) -> Result<mxr_config::LlmConfig, String> {
    let base_url = config.base_url.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return Err("llm.base_url must not be empty".to_string());
    }
    let parsed = url::Url::parse(&base_url).map_err(|e| format!("invalid llm.base_url: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("llm.base_url must use http or https".to_string());
    }

    let model = config.model.trim().to_string();
    if model.is_empty() {
        return Err("llm.model must not be empty".to_string());
    }
    if config.context_window == 0 {
        return Err("llm.context_window must be greater than 0".to_string());
    }
    if config.request_timeout_secs == 0 {
        return Err("llm.request_timeout_secs must be greater than 0".to_string());
    }

    Ok(mxr_config::LlmConfig {
        enabled: config.enabled,
        base_url,
        model,
        api_key_env: config.api_key_env.trim().to_string(),
        context_window: config.context_window,
        request_timeout_secs: config.request_timeout_secs,
        allow_cloud_relationship_data: config.allow_cloud_relationship_data,
        overrides: match config.overrides {
            Some(overrides) => normalize_llm_overrides(overrides)?,
            None => current.overrides.clone(),
        },
    })
}

fn normalize_llm_overrides(config: LlmOverridesData) -> Result<mxr_config::LlmOverrides, String> {
    Ok(mxr_config::LlmOverrides {
        summarize: normalize_llm_override(config.summarize)?,
        relationship_summary: normalize_llm_override(config.relationship_summary)?,
        commitments: normalize_llm_override(config.commitments)?,
        draft_assist: normalize_llm_override(config.draft_assist)?,
        draft_new: normalize_llm_override(config.draft_new)?,
        draft_refine: normalize_llm_override(config.draft_refine)?,
        voice_match: normalize_llm_override(config.voice_match)?,
        humanize_rewrite: normalize_llm_override(config.humanize_rewrite)?,
        answer_coverage: normalize_llm_override(config.answer_coverage)?,
        archive_ask: normalize_llm_override(config.archive_ask)?,
        decision_log: normalize_llm_override(config.decision_log)?,
        briefing: normalize_llm_override(config.briefing)?,
        expert: normalize_llm_override(config.expert)?,
    })
}

fn normalize_llm_override(
    config: Option<LlmOverrideData>,
) -> Result<Option<mxr_config::LlmOverrideConfig>, String> {
    let Some(config) = config else {
        return Ok(None);
    };
    let base_url = normalize_optional_string(config.base_url);
    if let Some(base_url) = &base_url {
        let parsed =
            url::Url::parse(base_url).map_err(|e| format!("invalid llm override base_url: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("llm override base_url must use http or https".to_string());
        }
    }
    if config.context_window == Some(0) {
        return Err("llm override context_window must be greater than 0".to_string());
    }
    if config.request_timeout_secs == Some(0) {
        return Err("llm override request_timeout_secs must be greater than 0".to_string());
    }
    let override_config = mxr_config::LlmOverrideConfig {
        enabled: config.enabled,
        base_url,
        model: normalize_optional_string(config.model),
        api_key_env: normalize_optional_string(config.api_key_env),
        context_window: config.context_window,
        request_timeout_secs: config.request_timeout_secs,
    };
    if override_config.enabled.is_none()
        && override_config.base_url.is_none()
        && override_config.model.is_none()
        && override_config.api_key_env.is_none()
        && override_config.context_window.is_none()
        && override_config.request_timeout_secs.is_none()
    {
        return Ok(None);
    }
    Ok(Some(override_config))
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) async fn enable_semantic(state: &AppState, enabled: bool) -> HandlerResult {
    state
        .mutate_config(|config| {
            config.search.semantic.enabled = enabled;
        })
        .await?;
    if enabled {
        let profile = state.config_snapshot().search.semantic.active_profile;
        if let Err(error) = state.semantic.use_profile(profile).await {
            tracing::warn!(profile = profile.as_str(), %error, "semantic enable saved; profile activation deferred");
        }
    }
    semantic_status(state).await
}

pub(crate) async fn install_semantic_profile(
    state: &AppState,
    profile: SemanticProfile,
) -> HandlerResult {
    state
        .semantic
        .install_profile(profile)
        .await
        .map_err(|e| e.to_string())?;
    semantic_status(state).await
}

pub(crate) async fn use_semantic_profile(
    state: &AppState,
    profile: SemanticProfile,
) -> HandlerResult {
    state
        .mutate_config(|config| {
            config.search.semantic.enabled = true;
            config.search.semantic.active_profile = profile;
        })
        .await?;
    if let Err(error) = state.semantic.use_profile(profile).await {
        tracing::warn!(profile = profile.as_str(), %error, "semantic profile selected; activation deferred");
    }
    semantic_status(state).await
}

pub(crate) async fn reindex_semantic(state: &AppState) -> HandlerResult {
    state
        .semantic
        .reindex_active()
        .await
        .map_err(|e| e.to_string())?;
    semantic_status(state).await
}

pub(crate) async fn backfill_semantic(state: &AppState) -> HandlerResult {
    state
        .semantic
        .backfill_active()
        .await
        .map_err(|e| e.to_string())?;
    semantic_status(state).await
}

pub(crate) async fn create_saved_search(
    state: &AppState,
    name: &str,
    query: &str,
    search_mode: SearchMode,
) -> HandlerResult {
    let search = mxr_core::types::SavedSearch {
        id: mxr_core::SavedSearchId::new(),
        account_id: None,
        name: name.to_string(),
        query: query.to_string(),
        search_mode,
        sort: mxr_core::types::SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: chrono::Utc::now(),
    };
    state
        .store
        .insert_saved_search(&search)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SavedSearchData { search })
}

pub(crate) async fn update_saved_search(
    state: &AppState,
    name: &str,
    new_name: Option<&str>,
    query: Option<&str>,
    search_mode: Option<&SearchMode>,
    sort: Option<&mxr_core::types::SortOrder>,
    icon: Option<&str>,
    position: Option<i32>,
) -> HandlerResult {
    let updated = state
        .store
        .update_saved_search_by_name(name, new_name, query, search_mode, sort, icon, position)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Saved search '{name}' not found"))?;
    Ok(ResponseData::SavedSearchData { search: updated })
}

pub(crate) async fn delete_saved_search(state: &AppState, name: &str) -> HandlerResult {
    match state
        .store
        .delete_saved_search_by_name(name)
        .await
        .map_err(|e| e.to_string())?
    {
        true => Ok(ResponseData::Ack),
        false => Err(format!("Saved search '{name}' not found")),
    }
}

pub(crate) async fn run_saved_search(state: &AppState, name: &str, limit: u32) -> HandlerResult {
    let saved = state
        .store
        .get_saved_search_by_name(name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Saved search '{name}' not found"))?;
    let execution = execute_search(
        state,
        &saved.query,
        limit as usize,
        0,
        saved.search_mode,
        SortOrder::DateDesc,
        false,
    )
    .await?;
    Ok(ResponseData::SearchResults {
        results: search_result_items(execution.results, execution.executed_mode),
        total: execution.total as u32,
        has_more: execution.has_more,
        next_offset: execution.next_offset.map(|value| value as u32),
        explain: None,
    })
}

pub(crate) async fn get_status(state: &AppState) -> HandlerResult {
    let (accounts, total_messages, sync_statuses) = collect_status_snapshot(state).await?;
    let repair_required = crate::server::search_requires_repair(state, total_messages).await;
    let semantic_runtime = state
        .semantic
        .status_snapshot()
        .await
        .ok()
        .map(|snapshot| snapshot.runtime);
    Ok(ResponseData::Status {
        uptime_secs: state.uptime_secs(),
        accounts,
        total_messages,
        daemon_pid: Some(std::process::id()),
        sync_statuses,
        protocol_version: IPC_PROTOCOL_VERSION,
        daemon_version: Some(crate::server::current_daemon_version().to_string()),
        daemon_build_id: Some(crate::server::current_build_id()),
        repair_required,
        semantic_runtime,
        feature_health: Some(feature_health_report(state)),
    })
}

pub(crate) async fn sync_now(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let operation_id = uuid::Uuid::now_v7().to_string();
    let operation = "sync".to_string();
    let account_id = account_id.cloned();
    emit_operation_event(
        state,
        DaemonEvent::OperationStarted {
            operation_id: operation_id.clone(),
            operation: operation.clone(),
            account_id: account_id.clone(),
            message: "Starting sync".to_string(),
        },
    );

    let provider = match state.get_provider(account_id.as_ref()) {
        Ok(provider) => provider.clone(),
        Err(error) => {
            emit_operation_event(
                state,
                DaemonEvent::OperationFailed {
                    operation_id,
                    operation,
                    account_id,
                    error: error.clone(),
                    retryable: false,
                },
            );
            return Err(error);
        }
    };

    emit_operation_event(
        state,
        DaemonEvent::OperationProgress {
            operation_id: operation_id.clone(),
            operation: operation.clone(),
            account_id: account_id.clone(),
            current: 0,
            total: None,
            message: "Syncing provider".to_string(),
        },
    );

    let outcome = match state
        .sync_engine
        .sync_account_with_outcome(provider.as_ref())
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            let error = error.to_string();
            emit_operation_event(
                state,
                DaemonEvent::OperationFailed {
                    operation_id,
                    operation,
                    account_id,
                    error: error.clone(),
                    retryable: true,
                },
            );
            return Err(error);
        }
    };
    if !outcome.upserted_message_ids.is_empty() {
        emit_operation_event(
            state,
            DaemonEvent::OperationProgress {
                operation_id: operation_id.clone(),
                operation: operation.clone(),
                account_id: account_id.clone(),
                current: outcome.upserted_message_ids.len() as u32,
                total: Some(outcome.upserted_message_ids.len() as u32),
                message: "Queueing semantic ingest".to_string(),
            },
        );
        if let Err(error) = state
            .semantic
            .enqueue_ingest_messages(&outcome.upserted_message_ids)
            .await
        {
            tracing::warn!(error = %error, "semantic ingest enqueue failed after sync");
            emit_operation_event(
                state,
                DaemonEvent::OperationProgress {
                    operation_id: operation_id.clone(),
                    operation: operation.clone(),
                    account_id: account_id.clone(),
                    current: outcome.upserted_message_ids.len() as u32,
                    total: Some(outcome.upserted_message_ids.len() as u32),
                    message: format!(
                        "Sync complete; semantic ingest deferred after enqueue failure: {error}"
                    ),
                },
            );
        }
        if let Some(account_id) = account_id.as_ref() {
            if let Err(error) = state
                .contacts_refresh
                .enqueue_accounts(std::slice::from_ref(account_id))
                .await
            {
                tracing::warn!(%account_id, "contacts refresh enqueue failed after sync: {error}");
            }
        }
        if let Err(error) = state
            .relationship
            .enqueue_contacts_from_messages(&outcome.upserted_message_ids)
            .await
        {
            tracing::warn!("relationship profile enqueue failed after sync: {error}");
        }
    }
    emit_operation_event(
        state,
        DaemonEvent::OperationCompleted {
            operation_id,
            operation,
            account_id,
            message: format!(
                "Sync complete: {} message(s) updated",
                outcome.upserted_message_ids.len()
            ),
        },
    );
    Ok(ResponseData::Ack)
}

/// Summary of what the post-sync incremental backfill actually did.
/// Used by callers (sync loop) to decide whether to log a line or
/// stay quiet — we don't want a "did nothing" line on every sync.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct AnalyticsBackfillReport {
    pub directions_reclassified: u32,
    pub list_ids_backfilled: u32,
    pub reply_pairs_resolved: u32,
    pub business_hours_backfilled: u32,
    /// Whether the heavier "scan all messages for new reply_pairs"
    /// step ran — only on the first sync after daemon restart.
    pub startup_repair_ran: bool,
}

impl AnalyticsBackfillReport {
    pub(crate) fn did_work(&self) -> bool {
        self.directions_reclassified > 0
            || self.list_ids_backfilled > 0
            || self.reply_pairs_resolved > 0
            || self.business_hours_backfilled > 0
    }
}

/// Run the cheap incremental analytics backfill steps. Each store
/// method is `WHERE column IS NULL / = 'unknown'` filtered, so calls
/// are near-free when there's nothing to do — making the function
/// itself the cheapest possible probe ("just run it"). On the first
/// invocation per daemon process, additionally runs the heavier
/// reply_pairs-from-messages scan to catch cases where a release
/// added a derived column that needs backfilling — the AtomicBool
/// guard means subsequent syncs skip it.
///
/// Errors are surfaced individually via `tracing::warn` (one bad
/// step shouldn't block the others) and the report counts only the
/// successful steps.
pub(crate) async fn incremental_analytics_backfill(state: &AppState) -> AnalyticsBackfillReport {
    use mxr_core::types::AccountAddressLookup;
    use std::sync::atomic::Ordering;

    let mut report = AnalyticsBackfillReport::default();

    // The 4 cheap delta steps. Each filters by NULL / 'unknown' so
    // is a no-op on healthy data. Refresh address cache once for the
    // direction reclassifier.
    state.refresh_account_addresses().await;
    let lookup = state.account_addresses.clone();
    match state
        .store
        .reclassify_unknown_directions(|email| lookup.is_account_address(email))
        .await
    {
        Ok(n) => report.directions_reclassified = n,
        Err(e) => tracing::warn!("post-sync reclassify_unknown_directions: {e}"),
    }
    match state.store.backfill_message_list_ids().await {
        Ok(n) => report.list_ids_backfilled = n,
        Err(e) => tracing::warn!("post-sync backfill_message_list_ids: {e}"),
    }

    // One-shot heavy step: scan messages for reply_pairs we haven't
    // captured yet (covers release upgrades adding the table). The
    // AtomicBool flips on first attempt regardless of outcome — a
    // failed attempt shouldn't loop forever; the user can `mxr
    // doctor --rebuild-analytics` to retry explicitly.
    if !state
        .analytics_startup_repair_done
        .swap(true, Ordering::SeqCst)
    {
        match state.store.backfill_reply_pairs_from_messages().await {
            Ok(n) => {
                report.reply_pairs_resolved += n;
                report.startup_repair_ran = true;
            }
            Err(e) => tracing::warn!("startup backfill_reply_pairs_from_messages: {e}"),
        }
    }

    match state.store.reconcile_reply_pair_pending().await {
        Ok(n) => report.reply_pairs_resolved += n,
        Err(e) => tracing::warn!("post-sync reconcile_reply_pair_pending: {e}"),
    }
    match state.store.backfill_business_hours_latency().await {
        Ok(n) => report.business_hours_backfilled = n,
        Err(e) => tracing::warn!("post-sync backfill_business_hours_latency: {e}"),
    }

    report
}

fn emit_operation_event(state: &AppState, event: DaemonEvent) {
    let _ = state.event_tx.send(IpcMessage {
        id: 0,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Event(event),
    });
}

pub(crate) async fn export_thread(
    state: &AppState,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_thread(state, thread_id, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message, .. } => Err(message),
    }
}

pub(crate) async fn export_search(
    state: &AppState,
    query: &str,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_search(state, query, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message, .. } => Err(message),
    }
}

pub(crate) async fn get_sync_status(state: &AppState, account_id: &AccountId) -> HandlerResult {
    let sync = build_account_sync_status(state, account_id).await?;
    Ok(ResponseData::SyncStatus { sync })
}

fn search_result_items(results: Vec<SearchResult>, mode: SearchMode) -> Vec<SearchResultItem> {
    results
        .into_iter()
        .filter_map(|result| {
            Some(SearchResultItem {
                message_id: parse_message_id(&result.message_id)?,
                account_id: mxr_core::AccountId::from_uuid(
                    uuid::Uuid::parse_str(&result.account_id).ok()?,
                ),
                thread_id: mxr_core::ThreadId::from_uuid(
                    uuid::Uuid::parse_str(&result.thread_id).ok()?,
                ),
                score: result.score,
                mode,
            })
        })
        .collect()
}

fn build_execution(
    requested_mode: SearchMode,
    executed_mode: SearchMode,
    results: Vec<SearchResult>,
    total: usize,
    has_more: bool,
    next_offset: Option<usize>,
    explain_input: ExecutionExplainInput<'_>,
) -> SearchExecution {
    let explain = explain_input.include_explain.then(|| SearchExplain {
        requested_mode,
        executed_mode,
        semantic_query: explain_input.semantic_query,
        lexical_window: explain_input.lexical_window as u32,
        dense_window: explain_input.dense_window.map(|value| value as u32),
        lexical_candidates: explain_input.lexical_results.len() as u32,
        dense_candidates: explain_input.dense_results.len() as u32,
        final_results: results.len() as u32,
        rrf_k: explain_input.rrf_k.map(|value| value as u32),
        notes: explain_input.notes,
        results: build_explain_results(
            &results,
            explain_input.lexical_results,
            explain_input.dense_results,
        ),
    });

    SearchExecution {
        results,
        total,
        has_more,
        next_offset,
        executed_mode,
        explain,
    }
}

fn build_explain_results(
    final_results: &[SearchResult],
    lexical_results: &[SearchResult],
    dense_results: &[SearchResult],
) -> Vec<SearchExplainResult> {
    let lexical_lookup = rank_lookup(lexical_results);
    let dense_lookup = rank_lookup(dense_results);

    final_results
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            let message_id = parse_message_id(&result.message_id)?;
            let lexical = lexical_lookup.get(&result.message_id);
            let dense = dense_lookup.get(&result.message_id);
            Some(SearchExplainResult {
                rank: (index + 1) as u32,
                message_id,
                final_score: result.score,
                lexical_rank: lexical.map(|entry| entry.0),
                lexical_score: lexical.map(|entry| entry.1),
                dense_rank: dense.map(|entry| entry.0),
                dense_score: dense.map(|entry| entry.1),
            })
        })
        .collect()
}

fn rank_lookup(results: &[SearchResult]) -> HashMap<String, (u32, f32)> {
    results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            (
                result.message_id.clone(),
                ((index + 1) as u32, result.score),
            )
        })
        .collect()
}

fn parse_message_id(value: &str) -> Option<MessageId> {
    Some(MessageId::from_uuid(uuid::Uuid::parse_str(value).ok()?))
}

fn reciprocal_rank_fusion(
    lexical: &[SearchResult],
    dense: &[SearchResult],
    k: usize,
) -> Vec<SearchResult> {
    let mut fused = HashMap::<String, (f32, SearchResult)>::new();

    for (rank, result) in lexical.iter().enumerate() {
        let score = 1.0 / (k + rank + 1) as f32;
        fused
            .entry(result.message_id.clone())
            .and_modify(|entry| entry.0 += score)
            .or_insert((score, result.clone()));
    }

    for (rank, result) in dense.iter().enumerate() {
        let score = 1.0 / (k + rank + 1) as f32;
        fused
            .entry(result.message_id.clone())
            .and_modify(|entry| entry.0 += score)
            .or_insert((score, result.clone()));
    }

    let mut results = fused
        .into_iter()
        .map(|(_, (score, mut result))| {
            result.score = score;
            result
        })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| right.score.total_cmp(&left.score));
    results
}

async fn sort_results(
    state: &AppState,
    mut results: Vec<SearchResult>,
    sort: SortOrder,
) -> Result<Vec<SearchResult>, String> {
    if matches!(sort, SortOrder::Relevance) || results.len() <= 1 {
        return Ok(results);
    }

    let message_ids = results
        .iter()
        .filter_map(|result| parse_message_id(&result.message_id))
        .collect::<Vec<_>>();
    let envelopes = state
        .store
        .list_envelopes_by_ids(&message_ids)
        .await
        .map_err(|e| e.to_string())?;
    let by_id = envelopes
        .into_iter()
        .map(|envelope| (envelope.id.as_str(), envelope))
        .collect::<HashMap<_, _>>();

    results.sort_by(|left, right| {
        let left_ts = by_id
            .get(&left.message_id)
            .map(|envelope| sane_search_sort_timestamp(envelope.date.timestamp()))
            .unwrap_or_default();
        let right_ts = by_id
            .get(&right.message_id)
            .map(|envelope| sane_search_sort_timestamp(envelope.date.timestamp()))
            .unwrap_or_default();
        match sort {
            SortOrder::DateDesc => right_ts
                .cmp(&left_ts)
                .then_with(|| right.message_id.cmp(&left.message_id)),
            SortOrder::DateAsc => left_ts
                .cmp(&right_ts)
                .then_with(|| left.message_id.cmp(&right.message_id)),
            SortOrder::Relevance => right.score.total_cmp(&left.score),
        }
    });
    Ok(results)
}

fn paginate_results(results: Vec<SearchResult>, offset: usize, limit: usize) -> SearchPage {
    let total = results.len();
    let has_more = total > offset.saturating_add(limit);
    SearchPage {
        total,
        has_more,
        next_offset: has_more.then_some(offset.saturating_add(limit)),
        results: results.into_iter().skip(offset).take(limit).collect(),
    }
}

fn sane_search_sort_timestamp(timestamp: i64) -> i64 {
    let cutoff = (chrono::Utc::now() + chrono::Duration::days(1)).timestamp();
    if timestamp > cutoff {
        0
    } else {
        timestamp
    }
}

/// Tests that don't need the `semantic-local` feature. Kept in their
/// own module so they run on the default `cargo test` invocation.
#[cfg(test)]
mod tests_no_semantic {
    use super::*;
    use crate::state::AppState;
    use std::sync::Arc;

    /// `rebuild_analytics` must broadcast `OperationStarted`, six
    /// `OperationProgress` events (one per SQL pass) with
    /// `total: Some(6)` and a stable `operation_id`, then
    /// `OperationCompleted` — all carrying the operation name
    /// `"rebuild-analytics"`. Without this the CLI/TUI spinner has
    /// nothing to show during the seconds-to-minutes-long handler
    /// and the user sees a hung process.
    /// `incremental_analytics_backfill` runs the heavy
    /// `backfill_reply_pairs_from_messages` step exactly once per
    /// daemon process — the `analytics_startup_repair_done` flag
    /// flips on the first call and gates subsequent calls. Without
    /// this gate every sync would re-scan the entire messages table
    /// for reply pairs, which is O(n) on a populated mailbox.
    #[tokio::test]
    async fn incremental_backfill_runs_startup_repair_only_on_first_call() {
        use std::sync::atomic::Ordering;
        let state = Arc::new(AppState::in_memory().await.unwrap());

        assert!(
            !state.analytics_startup_repair_done.load(Ordering::SeqCst),
            "guard must start unset — fresh daemon hasn't repaired yet"
        );

        let first = incremental_analytics_backfill(&state).await;
        assert!(
            first.startup_repair_ran,
            "first call must run the heavy startup repair"
        );
        assert!(
            state.analytics_startup_repair_done.load(Ordering::SeqCst),
            "first call must flip the guard"
        );

        let second = incremental_analytics_backfill(&state).await;
        assert!(
            !second.startup_repair_ran,
            "second call must skip the heavy repair — sync loop runs this every \
             sync and re-scanning every time would be O(messages)"
        );
    }

    /// On a fresh in-memory store there are no `Unknown` directions,
    /// `NULL` list_ids, or pending reply pairs to fix, so the cheap
    /// delta steps must all return 0. This pins the "no work" path
    /// — the post-sync hook fires every sync and we want it to be a
    /// silent no-op when data is healthy (no log spam, no perf
    /// regression).
    #[tokio::test]
    async fn incremental_backfill_returns_zeros_on_healthy_empty_store() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        // Pre-flip the guard so we observe only the cheap-delta
        // behaviour (the heavy path is covered by the test above).
        state
            .analytics_startup_repair_done
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let report = incremental_analytics_backfill(&state).await;
        assert_eq!(report.directions_reclassified, 0);
        assert_eq!(report.list_ids_backfilled, 0);
        assert_eq!(report.reply_pairs_resolved, 0);
        assert_eq!(report.business_hours_backfilled, 0);
        assert!(
            !report.did_work(),
            "did_work() must be false so the sync loop stays silent"
        );
        assert!(!report.startup_repair_ran);
    }

    #[tokio::test]
    async fn rebuild_analytics_emits_started_progress_completed_event_sequence() {
        use mxr_protocol::{DaemonEvent, IpcPayload};
        let state = Arc::new(AppState::in_memory().await.unwrap());
        // Subscribe before triggering so we don't lose the leading
        // `OperationStarted` to a race.
        let mut rx = state.event_tx.subscribe();

        let result = rebuild_analytics(&state).await;
        assert!(
            matches!(result, Ok(ResponseData::AnalyticsRebuildSummary { .. })),
            "handler must succeed against the in-memory store; got {result:?}"
        );

        let mut events: Vec<DaemonEvent> = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let IpcPayload::Event(event) = msg.payload {
                events.push(event);
            }
        }

        let started = events
            .iter()
            .find(|e| matches!(e, DaemonEvent::OperationStarted { .. }))
            .expect("must emit OperationStarted");
        let started_op_id = match started {
            DaemonEvent::OperationStarted {
                operation,
                operation_id,
                ..
            } => {
                assert_eq!(operation, "rebuild-analytics");
                operation_id.clone()
            }
            _ => unreachable!(),
        };

        let progress: Vec<&DaemonEvent> = events
            .iter()
            .filter(|e| matches!(e, DaemonEvent::OperationProgress { .. }))
            .collect();
        assert_eq!(
            progress.len(),
            6,
            "must emit one progress event per SQL pass (got {})",
            progress.len()
        );
        for (i, ev) in progress.iter().enumerate() {
            match ev {
                DaemonEvent::OperationProgress {
                    operation,
                    operation_id,
                    current,
                    total,
                    ..
                } => {
                    assert_eq!(operation, "rebuild-analytics");
                    assert_eq!(
                        operation_id, &started_op_id,
                        "operation_id must be stable across the run"
                    );
                    assert_eq!(
                        *current,
                        (i as u32) + 1,
                        "step counter must be 1-indexed and contiguous"
                    );
                    assert_eq!(
                        *total,
                        Some(6),
                        "total must pin to 6 so clients can render N/6"
                    );
                }
                _ => unreachable!(),
            }
        }

        let completed = events
            .iter()
            .find(|e| matches!(e, DaemonEvent::OperationCompleted { .. }))
            .expect("must emit OperationCompleted on success");
        match completed {
            DaemonEvent::OperationCompleted {
                operation,
                operation_id,
                ..
            } => {
                assert_eq!(operation, "rebuild-analytics");
                assert_eq!(operation_id, &started_op_id);
            }
            _ => unreachable!(),
        }

        assert!(
            !events
                .iter()
                .any(|e| matches!(e, DaemonEvent::OperationFailed { .. })),
            "no failure events on the happy path"
        );
    }
}

#[cfg(all(test, feature = "semantic-local"))]
mod tests {
    use super::*;
    use crate::state::AppState;
    use mxr_core::types::SortOrder;
    use std::sync::Arc;

    // Semantic chunk persistence runs on the semantic worker, which only spins up
    // with the `semantic-local` feature; without it the chunk poll never resolves.
    #[tokio::test]
    async fn sync_now_persists_semantic_chunks_without_embeddings_when_semantic_is_disabled() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = false;
        state.set_config_for_test(config).await;

        let response = sync_now(&state, None).await.unwrap();
        assert!(matches!(response, ResponseData::Ack));

        let lexical_hits = state
            .search
            .search("rollback trigger", 10, 0, SortOrder::Relevance)
            .await
            .unwrap();
        assert!(!lexical_hits.results.is_empty());

        let message_id = parse_message_id(&lexical_hits.results[0].message_id).unwrap();
        let body = state.store.get_body(&message_id).await.unwrap().unwrap();
        assert!(body
            .text_plain
            .as_deref()
            .unwrap_or_default()
            .contains("Rollback trigger"));

        let counts = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let counts = state.store.collect_record_counts().await.unwrap();
                if counts.semantic_chunks > 0 {
                    break counts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("semantic chunks should be persisted in the background");
        assert!(counts.semantic_chunks > 0);
        assert_eq!(counts.semantic_embeddings, 0);
        assert!(state
            .store
            .list_semantic_profiles()
            .await
            .unwrap()
            .is_empty());
    }
}
