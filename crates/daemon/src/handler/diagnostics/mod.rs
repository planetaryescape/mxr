mod search_execute;
mod search_filter;

use search_execute::execute_search;

use super::helpers::recent_log_lines;
use super::status_helpers::{
    build_account_sync_status, collect_doctor_report, collect_status_snapshot,
};
use super::{
    handle_export_search, handle_export_thread, helpers::protocol_event_entry, HandlerResult,
};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{ExportFormat, SearchMode, SemanticProfile, SortOrder};
use mxr_protocol::IPC_PROTOCOL_VERSION;
use mxr_protocol::{ResponseData, SearchExplain, SearchExplainResult, SearchResultItem};
use mxr_search::{SearchPage, SearchResult};
use std::collections::HashMap;

#[derive(Debug)]
struct SearchExecution {
    results: Vec<SearchResult>,
    has_more: bool,
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

pub(crate) async fn list_events(
    state: &AppState,
    limit: u32,
    level: Option<&str>,
    category: Option<&str>,
) -> HandlerResult {
    let entries = state
        .store
        .list_events(limit, level, category)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::EventLogEntries {
        entries: entries.into_iter().map(protocol_event_entry).collect(),
    })
}

pub(crate) async fn get_logs(state: &AppState, limit: u32, level: Option<&str>) -> HandlerResult {
    let lines = recent_log_lines(state, limit as usize, level).await?;
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
        has_more: execution.has_more,
        explain: execution.explain,
    })
}

pub(crate) async fn count(state: &AppState, query: &str, mode: SearchMode) -> HandlerResult {
    let results = execute_search(state, query, 10_000, 0, mode, SortOrder::DateDesc, false).await;
    Ok(ResponseData::Count {
        count: results.map_err(|e| e.to_string())?.results.len() as u32,
    })
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

pub(crate) async fn semantic_status(state: &AppState) -> HandlerResult {
    let snapshot = state
        .semantic
        .status_snapshot()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SemanticStatus { snapshot })
}

pub(crate) async fn enable_semantic(state: &AppState, enabled: bool) -> HandlerResult {
    if enabled {
        let profile = state.config_snapshot().search.semantic.active_profile;
        state
            .semantic
            .use_profile(profile)
            .await
            .map_err(|e| e.to_string())?;
    }
    state
        .mutate_config(|config| {
            config.search.semantic.enabled = enabled;
        })
        .await?;
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
        .semantic
        .use_profile(profile)
        .await
        .map_err(|e| e.to_string())?;
    state
        .mutate_config(|config| {
            config.search.semantic.enabled = true;
            config.search.semantic.active_profile = profile;
        })
        .await?;
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
        has_more: execution.has_more,
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
    })
}

pub(crate) async fn sync_now(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let provider = state.get_provider(account_id)?.clone();
    let outcome = state
        .sync_engine
        .sync_account_with_outcome(provider.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    if !outcome.upserted_message_ids.is_empty() {
        state
            .semantic
            .enqueue_ingest_messages(&outcome.upserted_message_ids)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(ResponseData::Ack)
}

pub(crate) async fn export_thread(
    state: &AppState,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_thread(state, thread_id, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message } => Err(message),
    }
}

pub(crate) async fn export_search(
    state: &AppState,
    query: &str,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_search(state, query, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message } => Err(message),
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
    has_more: bool,
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
        has_more,
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
    SearchPage {
        has_more: total > offset.saturating_add(limit),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use mxr_core::types::SortOrder;
    use std::sync::Arc;

    // Semantic chunk persistence runs on the semantic worker, which only spins up
    // with the `semantic-local` feature; without it the chunk poll never resolves.
    #[cfg(feature = "semantic-local")]
    #[tokio::test]
    async fn sync_now_persists_semantic_chunks_without_embeddings_when_semantic_is_disabled() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

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
