use super::{
    build_account_sync_status, collect_doctor_report, collect_status_snapshot,
    handle_export_search, handle_export_thread, protocol_event_entry, recent_log_lines,
    should_fallback_to_tantivy, HandlerResult,
};
use crate::mxr_core::id::{AccountId, MessageId, ThreadId};
use crate::mxr_core::types::{ExportFormat, SearchMode, SemanticProfile, SortOrder};
use crate::mxr_protocol::IPC_PROTOCOL_VERSION;
use crate::mxr_protocol::{ResponseData, SearchExplain, SearchExplainResult, SearchResultItem};
use crate::mxr_search::{
    ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp},
    parse_query, QueryBuilder, SearchPage, SearchResult,
};
use crate::mxr_semantic::{should_use_semantic, SemanticHit};
use crate::state::AppState;
use chrono::Datelike;
use std::collections::HashMap;
use std::sync::Arc;

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

pub(super) async fn list_events(
    state: &Arc<AppState>,
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

pub(super) fn get_logs(limit: u32, level: Option<&str>) -> HandlerResult {
    let lines = recent_log_lines(limit as usize, level).map_err(|e| e.to_string())?;
    Ok(ResponseData::LogLines { lines })
}

pub(super) async fn doctor_report(state: &Arc<AppState>) -> HandlerResult {
    let report = collect_doctor_report(state).await?;
    Ok(ResponseData::DoctorReport { report })
}

pub(super) async fn bug_report(
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

pub(super) async fn search(
    state: &Arc<AppState>,
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

pub(super) async fn count(state: &Arc<AppState>, query: &str, mode: SearchMode) -> HandlerResult {
    let results = execute_search(state, query, 10_000, 0, mode, SortOrder::DateDesc, false).await;
    Ok(ResponseData::Count {
        count: results.map_err(|e| e.to_string())?.results.len() as u32,
    })
}

pub(super) async fn get_headers(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
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

pub(super) async fn list_saved_searches(state: &Arc<AppState>) -> HandlerResult {
    let searches = state
        .store
        .list_saved_searches()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SavedSearches { searches })
}

pub(super) async fn list_subscriptions(
    state: &Arc<AppState>,
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

pub(super) async fn semantic_status(state: &Arc<AppState>) -> HandlerResult {
    let snapshot = state
        .semantic
        .lock()
        .await
        .status_snapshot()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SemanticStatus { snapshot })
}

pub(super) async fn enable_semantic(state: &Arc<AppState>, enabled: bool) -> HandlerResult {
    if enabled {
        let profile = state.config_snapshot().search.semantic.active_profile;
        state
            .semantic
            .lock()
            .await
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

pub(super) async fn install_semantic_profile(
    state: &Arc<AppState>,
    profile: SemanticProfile,
) -> HandlerResult {
    state
        .semantic
        .lock()
        .await
        .install_profile(profile)
        .await
        .map_err(|e| e.to_string())?;
    semantic_status(state).await
}

pub(super) async fn use_semantic_profile(
    state: &Arc<AppState>,
    profile: SemanticProfile,
) -> HandlerResult {
    state
        .semantic
        .lock()
        .await
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

pub(super) async fn reindex_semantic(state: &Arc<AppState>) -> HandlerResult {
    state
        .semantic
        .lock()
        .await
        .reindex_active()
        .await
        .map_err(|e| e.to_string())?;
    semantic_status(state).await
}

pub(super) async fn create_saved_search(
    state: &Arc<AppState>,
    name: &str,
    query: &str,
    search_mode: SearchMode,
) -> HandlerResult {
    let search = crate::mxr_core::types::SavedSearch {
        id: crate::mxr_core::SavedSearchId::new(),
        account_id: None,
        name: name.to_string(),
        query: query.to_string(),
        search_mode,
        sort: crate::mxr_core::types::SortOrder::DateDesc,
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

pub(super) async fn delete_saved_search(state: &Arc<AppState>, name: &str) -> HandlerResult {
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

pub(super) async fn run_saved_search(
    state: &Arc<AppState>,
    name: &str,
    limit: u32,
) -> HandlerResult {
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

pub(super) async fn get_status(state: &Arc<AppState>) -> HandlerResult {
    let (accounts, total_messages, sync_statuses) = collect_status_snapshot(state).await?;
    let repair_required = crate::server::search_requires_repair(state, total_messages).await;
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
    })
}

pub(super) async fn sync_now(
    state: &Arc<AppState>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let provider = state.get_provider(account_id)?.clone();
    let outcome = state
        .sync_engine
        .sync_account_with_outcome(provider.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    if !outcome.upserted_message_ids.is_empty() {
        state
            .semantic
            .lock()
            .await
            .reindex_messages(&outcome.upserted_message_ids)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn export_thread(
    state: &Arc<AppState>,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_thread(state, thread_id, format).await {
        crate::mxr_protocol::Response::Ok { data } => Ok(data),
        crate::mxr_protocol::Response::Error { message } => Err(message),
    }
}

pub(super) async fn export_search(
    state: &Arc<AppState>,
    query: &str,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_search(state, query, format).await {
        crate::mxr_protocol::Response::Ok { data } => Ok(data),
        crate::mxr_protocol::Response::Error { message } => Err(message),
    }
}

pub(super) async fn get_sync_status(
    state: &Arc<AppState>,
    account_id: &AccountId,
) -> HandlerResult {
    let sync = build_account_sync_status(state, account_id).await?;
    Ok(ResponseData::SyncStatus { sync })
}

fn search_result_items(results: Vec<SearchResult>, mode: SearchMode) -> Vec<SearchResultItem> {
    results
        .into_iter()
        .filter_map(|result| {
            Some(SearchResultItem {
                message_id: parse_message_id(&result.message_id)?,
                account_id: crate::mxr_core::AccountId::from_uuid(
                    uuid::Uuid::parse_str(&result.account_id).ok()?,
                ),
                thread_id: crate::mxr_core::ThreadId::from_uuid(
                    uuid::Uuid::parse_str(&result.thread_id).ok()?,
                ),
                score: result.score,
                mode,
            })
        })
        .collect()
}

async fn execute_search(
    state: &Arc<AppState>,
    query: &str,
    limit: usize,
    offset: usize,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> Result<SearchExecution, String> {
    match parse_query(query) {
        Ok(ast) => execute_search_ast(state, query, &ast, limit, offset, mode, sort, explain).await,
        Err(error) => {
            let search = state.search.lock().await;
            if should_fallback_to_tantivy(query, &error) {
                let page = search
                    .search(query, limit, offset, sort)
                    .map_err(|e| e.to_string())?;
                let explain = explain.then(|| SearchExplain {
                    requested_mode: mode,
                    executed_mode: SearchMode::Lexical,
                    semantic_query: None,
                    lexical_window: limit as u32,
                    dense_window: None,
                    lexical_candidates: page.results.len() as u32,
                    dense_candidates: 0,
                    final_results: page.results.len() as u32,
                    rrf_k: None,
                    notes: vec![format!(
                        "structured parser rejected query ({error}); used Tantivy fallback"
                    )],
                    results: build_explain_results(&page.results, &page.results, &[]),
                });
                Ok(SearchExecution {
                    results: page.results,
                    has_more: page.has_more,
                    executed_mode: SearchMode::Lexical,
                    explain,
                })
            } else {
                Err(format!("Invalid search query: {error}"))
            }
        }
    }
}

async fn execute_search_ast(
    state: &Arc<AppState>,
    _query: &str,
    ast: &QueryNode,
    limit: usize,
    offset: usize,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> Result<SearchExecution, String> {
    let requested_window = limit.saturating_add(offset).saturating_add(1);
    let lexical_window = if mode == SearchMode::Lexical {
        limit
    } else {
        requested_window.saturating_mul(4).max(100)
    };
    let lexical_page = lexical_search(
        state,
        ast,
        if mode == SearchMode::Lexical {
            limit
        } else {
            lexical_window
        },
        if mode == SearchMode::Lexical {
            offset
        } else {
            0
        },
        if mode == SearchMode::Lexical {
            sort.clone()
        } else {
            SortOrder::Relevance
        },
    )
    .await?;
    let lexical_results = lexical_page.results.clone();

    if mode == SearchMode::Lexical {
        return Ok(build_execution(
            mode,
            SearchMode::Lexical,
            lexical_page.results,
            lexical_page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: None,
                lexical_window,
                dense_window: None,
                lexical_results: &lexical_results,
                dense_results: &[],
                rrf_k: None,
                notes: Vec::new(),
            },
        ));
    }

    if !should_use_semantic(mode) {
        let page = paginate_results(lexical_results.clone(), offset, limit);
        return Ok(build_execution(
            mode,
            SearchMode::Lexical,
            page.results,
            page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: None,
                lexical_window,
                dense_window: None,
                lexical_results: &lexical_results,
                dense_results: &[],
                rrf_k: None,
                notes: vec!["semantic search unavailable in this binary".to_string()],
            },
        ));
    }
    let semantic_enabled = state.config_snapshot().search.semantic.enabled;

    let Some(semantic_query) = semantic_query_text(ast) else {
        let mut notes = vec!["query has no semantic text terms; used lexical ranking".to_string()];
        if !semantic_enabled {
            notes.push("semantic search disabled in config".to_string());
        }
        let page = paginate_results(lexical_results.clone(), offset, limit);
        return Ok(build_execution(
            mode,
            SearchMode::Lexical,
            page.results,
            page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: None,
                lexical_window,
                dense_window: None,
                lexical_results: &lexical_results,
                dense_results: &[],
                rrf_k: None,
                notes,
            },
        ));
    };
    if semantic_query.is_empty() || has_negated_semantic_terms(ast) {
        let mut notes =
            vec!["query contains negated semantic terms; used lexical ranking".to_string()];
        if !semantic_enabled {
            notes.push("semantic search disabled in config".to_string());
        }
        let page = paginate_results(lexical_results.clone(), offset, limit);
        return Ok(build_execution(
            mode,
            SearchMode::Lexical,
            page.results,
            page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: Some(semantic_query),
                lexical_window,
                dense_window: None,
                lexical_results: &lexical_results,
                dense_results: &[],
                rrf_k: None,
                notes,
            },
        ));
    }

    if !semantic_enabled {
        let page = paginate_results(lexical_results.clone(), offset, limit);
        return Ok(build_execution(
            mode,
            SearchMode::Lexical,
            page.results,
            page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: Some(semantic_query),
                lexical_window,
                dense_window: None,
                lexical_results: &lexical_results,
                dense_results: &[],
                rrf_k: None,
                notes: vec!["semantic search disabled in config; used lexical ranking".to_string()],
            },
        ));
    }

    let dense_window = requested_window.saturating_mul(8).max(200);
    let semantic_hits = {
        let mut semantic = state.semantic.lock().await;
        semantic
            .search(&semantic_query, dense_window)
            .await
            .map_err(|e| e.to_string())?
    };

    let dense_results = filter_dense_hits(state, ast, semantic_hits).await?;
    if mode == SearchMode::Semantic {
        if dense_results.is_empty() {
            let page = paginate_results(lexical_results.clone(), offset, limit);
            return Ok(build_execution(
                mode,
                SearchMode::Lexical,
                page.results,
                page.has_more,
                ExecutionExplainInput {
                    include_explain: explain,
                    semantic_query: Some(semantic_query),
                    lexical_window,
                    dense_window: Some(dense_window),
                    lexical_results: &lexical_results,
                    dense_results: &dense_results,
                    rrf_k: None,
                    notes: vec![
                        "semantic retrieval returned no dense candidates; fell back to lexical"
                            .into(),
                    ],
                },
            ));
        }
        let dense_results = sort_results(state, dense_results, sort).await?;
        let page = paginate_results(dense_results.clone(), offset, limit);
        return Ok(build_execution(
            mode,
            SearchMode::Semantic,
            page.results,
            page.has_more,
            ExecutionExplainInput {
                include_explain: explain,
                semantic_query: Some(semantic_query),
                lexical_window,
                dense_window: Some(dense_window),
                lexical_results: &lexical_results,
                dense_results: &dense_results,
                rrf_k: None,
                notes: Vec::new(),
            },
        ));
    }

    let mut notes = Vec::new();
    if dense_results.is_empty() {
        notes.push(
            "dense retrieval returned no candidates; hybrid ranking used lexical results only"
                .to_string(),
        );
    }
    let fused_results = reciprocal_rank_fusion(&lexical_results, &dense_results, 60);
    let fused_results = sort_results(state, fused_results, sort).await?;
    let page = paginate_results(fused_results.clone(), offset, limit);
    Ok(build_execution(
        mode,
        SearchMode::Hybrid,
        page.results,
        page.has_more,
        ExecutionExplainInput {
            include_explain: explain,
            semantic_query: Some(semantic_query),
            lexical_window,
            dense_window: Some(dense_window),
            lexical_results: &lexical_results,
            dense_results: &dense_results,
            rrf_k: Some(60),
            notes,
        },
    ))
}

async fn lexical_search(
    state: &Arc<AppState>,
    ast: &QueryNode,
    limit: usize,
    offset: usize,
    sort: SortOrder,
) -> Result<SearchPage, String> {
    let search = state.search.lock().await;
    let builder = QueryBuilder::new(search.schema());
    let tantivy_query = builder.build(ast);
    search
        .search_ast(tantivy_query, limit, offset, sort)
        .map_err(|e| e.to_string())
}

async fn filter_dense_hits(
    state: &Arc<AppState>,
    ast: &QueryNode,
    hits: Vec<SemanticHit>,
) -> Result<Vec<SearchResult>, String> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }

    let message_ids = hits
        .iter()
        .map(|hit| hit.message_id.clone())
        .collect::<Vec<_>>();
    let envelopes = state
        .store
        .list_envelopes_by_ids(&message_ids)
        .await
        .map_err(|e| e.to_string())?;
    let envelopes_by_id = envelopes
        .into_iter()
        .map(|envelope| (envelope.id.clone(), envelope))
        .collect::<HashMap<_, _>>();

    let mut results = Vec::new();
    for hit in hits {
        let Some(envelope) = envelopes_by_id.get(&hit.message_id) else {
            continue;
        };
        if !matches_structured_filters(ast, envelope) {
            continue;
        }
        results.push(SearchResult {
            message_id: envelope.id.as_str(),
            account_id: envelope.account_id.as_str(),
            thread_id: envelope.thread_id.as_str(),
            score: hit.score,
        });
    }
    Ok(results)
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
    state: &Arc<AppState>,
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

fn semantic_query_text(ast: &QueryNode) -> Option<String> {
    let mut parts = Vec::new();
    collect_semantic_terms(ast, false, &mut parts);
    let query = parts.join(" ").trim().to_string();
    if query.is_empty() {
        None
    } else {
        Some(query)
    }
}

fn collect_semantic_terms(node: &QueryNode, negated: bool, parts: &mut Vec<String>) {
    match node {
        QueryNode::Text(text) if !negated => parts.push(text.clone()),
        QueryNode::Phrase(text) if !negated => parts.push(text.clone()),
        QueryNode::Field { field, value }
            if !negated
                && matches!(
                    field,
                    QueryField::Subject | QueryField::Body | QueryField::Filename
                ) =>
        {
            parts.push(value.clone());
        }
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            collect_semantic_terms(left, negated, parts);
            collect_semantic_terms(right, negated, parts);
        }
        QueryNode::Not(inner) => collect_semantic_terms(inner, true, parts),
        _ => {}
    }
}

fn has_negated_semantic_terms(node: &QueryNode) -> bool {
    match node {
        QueryNode::Not(inner) => contains_semantic_term(inner),
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            has_negated_semantic_terms(left) || has_negated_semantic_terms(right)
        }
        _ => false,
    }
}

fn contains_semantic_term(node: &QueryNode) -> bool {
    match node {
        QueryNode::Text(_) | QueryNode::Phrase(_) => true,
        QueryNode::Field { field, .. } => matches!(
            field,
            QueryField::Subject | QueryField::Body | QueryField::Filename
        ),
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            contains_semantic_term(left) || contains_semantic_term(right)
        }
        QueryNode::Not(inner) => contains_semantic_term(inner),
        _ => false,
    }
}

fn matches_structured_filters(node: &QueryNode, envelope: &crate::mxr_core::Envelope) -> bool {
    match node {
        QueryNode::Text(_) | QueryNode::Phrase(_) => true,
        QueryNode::Field { field, value } => match field {
            QueryField::Subject | QueryField::Body | QueryField::Filename => true,
            QueryField::From => {
                address_matches(&envelope.from.email, envelope.from.name.as_deref(), value)
            }
            QueryField::To => envelope
                .to
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
            QueryField::Cc => envelope
                .cc
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
            QueryField::Bcc => envelope
                .bcc
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
        },
        QueryNode::Filter(filter) => matches_filter(filter, envelope),
        QueryNode::Label(label) => envelope
            .label_provider_ids
            .iter()
            .any(|provider_id| provider_id.eq_ignore_ascii_case(label)),
        QueryNode::DateRange { bound, date } => matches_date(bound, date, envelope),
        QueryNode::Size { op, bytes } => matches_size(op, *bytes, envelope.size_bytes),
        QueryNode::And(left, right) => {
            matches_structured_filters(left, envelope)
                && matches_structured_filters(right, envelope)
        }
        QueryNode::Or(left, right) => {
            matches_structured_filters(left, envelope)
                || matches_structured_filters(right, envelope)
        }
        QueryNode::Not(inner) => !matches_structured_filters(inner, envelope),
    }
}

fn address_matches(email: &str, name: Option<&str>, value: &str) -> bool {
    let needle = value.to_ascii_lowercase();
    email.to_ascii_lowercase().contains(&needle)
        || name
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains(&needle)
}

fn matches_filter(filter: &FilterKind, envelope: &crate::mxr_core::Envelope) -> bool {
    match filter {
        FilterKind::Unread => !envelope.flags.contains(crate::mxr_core::MessageFlags::READ),
        FilterKind::Read => envelope.flags.contains(crate::mxr_core::MessageFlags::READ),
        FilterKind::Starred => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::STARRED),
        FilterKind::Draft => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::DRAFT),
        FilterKind::Sent => envelope.flags.contains(crate::mxr_core::MessageFlags::SENT),
        FilterKind::Trash => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::TRASH),
        FilterKind::Spam => envelope.flags.contains(crate::mxr_core::MessageFlags::SPAM),
        FilterKind::Answered => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::ANSWERED),
        FilterKind::Inbox => envelope
            .label_provider_ids
            .iter()
            .any(|label| label.eq_ignore_ascii_case("INBOX")),
        FilterKind::Archived => {
            !envelope
                .label_provider_ids
                .iter()
                .any(|label| label.eq_ignore_ascii_case("INBOX"))
                && !envelope.flags.contains(crate::mxr_core::MessageFlags::SENT)
                && !envelope
                    .flags
                    .contains(crate::mxr_core::MessageFlags::DRAFT)
                && !envelope
                    .flags
                    .contains(crate::mxr_core::MessageFlags::TRASH)
                && !envelope.flags.contains(crate::mxr_core::MessageFlags::SPAM)
        }
        FilterKind::HasAttachment => envelope.has_attachments,
    }
}

fn matches_date(bound: &DateBound, date: &DateValue, envelope: &crate::mxr_core::Envelope) -> bool {
    let message_date = envelope.date.date_naive();
    let resolved = resolve_date_value(date);
    match bound {
        DateBound::After => message_date >= resolved,
        DateBound::Before => message_date < resolved,
        DateBound::Exact => message_date == resolved,
    }
}

fn resolve_date_value(value: &DateValue) -> chrono::NaiveDate {
    let today = chrono::Local::now().date_naive();
    match value {
        DateValue::Specific(date) => *date,
        DateValue::Today => today,
        DateValue::Yesterday => today.pred_opt().unwrap_or(today),
        DateValue::ThisWeek => {
            let weekday = today.weekday().num_days_from_monday();
            today - chrono::Duration::days(weekday as i64)
        }
        DateValue::ThisMonth => {
            chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today)
        }
    }
}

fn matches_size(op: &SizeOp, bytes: u64, actual: u64) -> bool {
    match op {
        SizeOp::LessThan => actual < bytes,
        SizeOp::LessThanOrEqual => actual <= bytes,
        SizeOp::Equal => actual == bytes,
        SizeOp::GreaterThan => actual > bytes,
        SizeOp::GreaterThanOrEqual => actual >= bytes,
    }
}
