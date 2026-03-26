use super::search_filter::{
    has_negated_semantic_terms, matches_structured_filters, semantic_query_text,
};
use super::{build_execution, ExecutionExplainInput, SearchExecution};
use crate::mxr_core::types::{SearchMode, SortOrder};
use crate::mxr_search::{ast::QueryNode, parse_query, QueryBuilder, SearchPage, SearchResult};
use crate::mxr_semantic::{should_use_semantic, SemanticHit};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::Arc;

use super::{paginate_results, sort_results};
use crate::handler::should_fallback_to_tantivy;
use crate::mxr_protocol::SearchExplain;

pub(super) async fn execute_search(
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
                    results: super::build_explain_results(&page.results, &page.results, &[]),
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
    let fused_results = super::reciprocal_rank_fusion(&lexical_results, &dense_results, 60);
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

pub(super) async fn lexical_search(
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

pub(super) async fn filter_dense_hits(
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
