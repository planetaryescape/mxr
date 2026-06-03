use super::label_resolve::{build_label_name_index, resolve_label_names};
use super::search_filter::{
    ast_contains_owed_reply, has_negated_semantic_terms, matches_structured_filters,
    semantic_query_plan,
};
use super::{build_execution, ExecutionExplainInput, SearchExecution};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::{Label, SearchMode, SortOrder};
use mxr_search::{ast::QueryNode, parse_query, MxrSchema, QueryBuilder, SearchPage, SearchResult};
use mxr_semantic::{should_use_semantic, SemanticHit};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use super::{paginate_results, sort_results};
use crate::handler::should_fallback_to_tantivy;
use mxr_protocol::SearchExplain;

#[derive(Clone)]
struct SearchExecutionOptions {
    limit: usize,
    offset: usize,
    account_id: Option<AccountId>,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute_search(
    state: &AppState,
    query: &str,
    limit: usize,
    offset: usize,
    account_id: Option<&AccountId>,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> Result<SearchExecution, String> {
    let options = SearchExecutionOptions {
        limit,
        offset,
        account_id: account_id.cloned(),
        mode,
        sort,
        explain,
    };
    let (mut execution, needs_owed_filter) = match parse_query(query) {
        Ok(ast) => {
            // Resolve user-typed label display names (e.g.
            // `label:Notto`) to provider IDs (e.g. `Label_101`)
            // before query construction. Tantivy only sees provider
            // IDs because that's what sync writes to the index; the
            // parser doesn't know about a given user's labels.
            let labels = collect_labels_for_resolution(state).await;
            let label_index = build_label_name_index(&labels);
            let ast = resolve_label_names(ast, &label_index);
            let needs_owed_filter = ast_contains_owed_reply(&ast);
            (
                execute_search_ast(state, query, &ast, &options).await?,
                needs_owed_filter,
            )
        }
        Err(error) => {
            if should_fallback_to_tantivy(query, &error) {
                let page = lexical_text_search(
                    state,
                    query,
                    options.account_id.as_ref(),
                    options.limit,
                    options.offset,
                    options.sort,
                )
                .await?;
                let explain = options.explain.then(|| SearchExplain {
                    requested_mode: options.mode,
                    executed_mode: SearchMode::Lexical,
                    semantic_query: None,
                    lexical_window: options.limit as u32,
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
                let execution = SearchExecution {
                    results: page.results,
                    total: page.total,
                    has_more: page.has_more,
                    next_offset: page.next_offset,
                    executed_mode: SearchMode::Lexical,
                    explain,
                };
                (execution, false)
            } else {
                return Err(format!("Invalid search query: {error}"));
            }
        }
    };
    filter_disabled_accounts(state, &mut execution).await?;
    if needs_owed_filter {
        filter_to_owed_threads(state, &mut execution).await?;
    }
    Ok(execution)
}

/// Collect every label across enabled accounts so we can rewrite
/// `label:<display name>` into the provider IDs that Tantivy actually
/// indexes. Failures are non-fatal: if the store is unreachable we
/// fall back to the unresolved AST, which still serves raw provider
/// IDs correctly.
async fn collect_labels_for_resolution(state: &AppState) -> Vec<Label> {
    let accounts = match state.store.list_accounts().await {
        Ok(accounts) => accounts,
        Err(error) => {
            tracing::warn!(
                "label resolution: list_accounts failed ({error}); falling back to verbatim labels"
            );
            return Vec::new();
        }
    };
    let mut labels = Vec::new();
    for account in accounts {
        if !account.enabled {
            continue;
        }
        match state.store.list_labels_by_account(&account.id).await {
            Ok(mut account_labels) => labels.append(&mut account_labels),
            Err(error) => {
                tracing::warn!(
                    account_id = %account.id.as_str(),
                    "label resolution: list_labels_by_account failed ({error})"
                );
            }
        }
    }
    labels
}

/// Reduce `execution.results` to the set of messages whose thread is
/// owed-reply right now. Owed-reply is dynamic (depends on current
/// time and cadence) so this is computed inline rather than indexed.
/// Spans every enabled account that produced a hit.
async fn filter_to_owed_threads(
    state: &AppState,
    execution: &mut SearchExecution,
) -> Result<(), String> {
    if execution.results.is_empty() {
        return Ok(());
    }
    // Collect the unique account IDs from the current result set so we
    // only run owed-reply computation for accounts that produced hits.
    let mut account_ids: Vec<String> = execution
        .results
        .iter()
        .map(|r| r.account_id.clone())
        .collect();
    account_ids.sort();
    account_ids.dedup();

    let mut owed_thread_ids: HashSet<String> = HashSet::new();
    for account_id_str in account_ids {
        let account_id = match mxr_core::AccountId::from_str(&account_id_str) {
            Ok(id) => id,
            Err(_) => continue,
        };
        match state
            .store
            .list_owed_replies(&account_id, None, None, OWED_REPLY_FETCH_CAP)
            .await
        {
            Ok(rows) => {
                for row in rows {
                    owed_thread_ids.insert(row.thread_id.as_str().clone());
                }
            }
            Err(e) => {
                tracing::warn!(
                    account_id = %account_id_str,
                    "owed-reply filter: list_owed_replies failed: {e}"
                );
            }
        }
    }

    execution
        .results
        .retain(|r| owed_thread_ids.contains(&r.thread_id));
    if let Some(explain) = execution.explain.as_mut() {
        let retained_ids: HashSet<_> = execution
            .results
            .iter()
            .map(|r| r.message_id.clone())
            .collect();
        explain
            .results
            .retain(|r| retained_ids.contains(&r.message_id.as_str()));
        for (idx, r) in explain.results.iter_mut().enumerate() {
            r.rank = idx as u32 + 1;
        }
        explain.final_results = execution.results.len() as u32;
    }
    if execution.results.is_empty() {
        execution.has_more = false;
        execution.next_offset = None;
    }
    Ok(())
}

/// Pull at most this many owed-reply rows per account when serving an
/// `is:owed-reply` query. The owed-reply lens is intended for human
/// triage; capping at a few thousand keeps the filter cheap even on
/// inboxes with very long backlogs of unanswered threads.
const OWED_REPLY_FETCH_CAP: u32 = 5_000;

async fn filter_disabled_accounts(
    state: &AppState,
    execution: &mut SearchExecution,
) -> Result<(), String> {
    let enabled_accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|account| account.enabled)
        .map(|account| account.id.as_str())
        .collect::<HashSet<_>>();

    execution
        .results
        .retain(|result| enabled_accounts.contains(&result.account_id));
    if let Some(explain) = execution.explain.as_mut() {
        let retained_ids = execution
            .results
            .iter()
            .map(|result| result.message_id.clone())
            .collect::<HashSet<_>>();
        explain
            .results
            .retain(|result| retained_ids.contains(&result.message_id.as_str()));
        for (index, result) in explain.results.iter_mut().enumerate() {
            result.rank = index as u32 + 1;
        }
        explain.final_results = execution.results.len() as u32;
    }
    if execution.results.is_empty() {
        execution.has_more = false;
        execution.next_offset = None;
    }
    Ok(())
}

async fn execute_search_ast(
    state: &AppState,
    _query: &str,
    ast: &QueryNode,
    options: &SearchExecutionOptions,
) -> Result<SearchExecution, String> {
    let requested_window = options
        .limit
        .saturating_add(options.offset)
        .saturating_add(1);
    let lexical_window = if options.mode == SearchMode::Lexical {
        options.limit
    } else {
        requested_window.saturating_mul(4).max(100)
    };
    let lexical_page = lexical_search(
        state,
        ast,
        options.account_id.as_ref(),
        if options.mode == SearchMode::Lexical {
            options.limit
        } else {
            lexical_window
        },
        if options.mode == SearchMode::Lexical {
            options.offset
        } else {
            0
        },
        if options.mode == SearchMode::Lexical {
            options.sort.clone()
        } else {
            SortOrder::Relevance
        },
    )
    .await?;
    let lexical_results = lexical_page.results.clone();

    if options.mode == SearchMode::Lexical {
        return Ok(build_execution(
            options.mode,
            SearchMode::Lexical,
            lexical_page.results,
            lexical_page.total,
            lexical_page.has_more,
            lexical_page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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

    if !should_use_semantic(options.mode) {
        let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
        return Ok(build_execution(
            options.mode,
            SearchMode::Lexical,
            page.results,
            page.total,
            page.has_more,
            page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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

    let Some(semantic_plan) = semantic_query_plan(ast) else {
        let mut notes = vec!["query has no semantic text terms; used lexical ranking".to_string()];
        if !semantic_enabled {
            notes.push("semantic search disabled in config".to_string());
        }
        let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
        return Ok(build_execution(
            options.mode,
            SearchMode::Lexical,
            page.results,
            page.total,
            page.has_more,
            page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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
    let semantic_query = semantic_plan.text.clone();
    if semantic_query.is_empty() || has_negated_semantic_terms(ast) {
        let mut notes =
            vec!["query contains negated semantic terms; used lexical ranking".to_string()];
        if !semantic_enabled {
            notes.push("semantic search disabled in config".to_string());
        }
        let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
        return Ok(build_execution(
            options.mode,
            SearchMode::Lexical,
            page.results,
            page.total,
            page.has_more,
            page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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
        let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
        return Ok(build_execution(
            options.mode,
            SearchMode::Lexical,
            page.results,
            page.total,
            page.has_more,
            page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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
    let semantic_hits = match state
        .semantic
        // Lexical search remains the exact/literal path. Dense retrieval only
        // broadens recall inside the source kinds implied by the parsed query.
        .search(&semantic_query, dense_window, &semantic_plan.source_kinds)
        .await
    {
        Ok(hits) => hits,
        Err(error) => {
            let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
            return Ok(build_execution(
                options.mode,
                SearchMode::Lexical,
                page.results,
                page.total,
                page.has_more,
                page.next_offset,
                ExecutionExplainInput {
                    include_explain: options.explain,
                    semantic_query: Some(semantic_query),
                    lexical_window,
                    dense_window: Some(dense_window),
                    lexical_results: &lexical_results,
                    dense_results: &[],
                    rrf_k: None,
                    notes: vec![format!(
                        "semantic retrieval failed ({error}); used lexical ranking"
                    )],
                },
            ));
        }
    };

    let dense_results =
        filter_dense_hits(state, ast, options.account_id.as_ref(), semantic_hits).await?;
    if options.mode == SearchMode::Semantic {
        if dense_results.is_empty() {
            let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
            return Ok(build_execution(
                options.mode,
                SearchMode::Lexical,
                page.results,
                page.total,
                page.has_more,
                page.next_offset,
                ExecutionExplainInput {
                    include_explain: options.explain,
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
        let dense_results = sort_results(state, dense_results, options.sort.clone()).await?;
        let page = paginate_results(dense_results.clone(), options.offset, options.limit);
        return Ok(build_execution(
            options.mode,
            SearchMode::Semantic,
            page.results,
            page.total,
            page.has_more,
            page.next_offset,
            ExecutionExplainInput {
                include_explain: options.explain,
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
    let fused_results = sort_results(state, fused_results, options.sort.clone()).await?;
    let page = paginate_results(fused_results.clone(), options.offset, options.limit);
    Ok(build_execution(
        options.mode,
        SearchMode::Hybrid,
        page.results,
        page.total,
        page.has_more,
        page.next_offset,
        ExecutionExplainInput {
            include_explain: options.explain,
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
    state: &AppState,
    ast: &QueryNode,
    account_id: Option<&AccountId>,
    limit: usize,
    offset: usize,
    sort: SortOrder,
) -> Result<SearchPage, String> {
    live_lexical_page(state, limit, offset, sort, |raw_limit, raw_offset, sort| {
        let schema = MxrSchema::build();
        let builder = QueryBuilder::new(&schema);
        let tantivy_query = if let Some(account_id) = account_id {
            let account_id = account_id.as_str();
            builder.build_in_account(ast, &account_id)
        } else {
            builder.build(ast)
        };
        async move {
            state
                .search
                .search_ast(tantivy_query, raw_limit, raw_offset, sort)
                .await
                .map_err(|e| e.to_string())
        }
    })
    .await
}

async fn lexical_text_search(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
    limit: usize,
    offset: usize,
    sort: SortOrder,
) -> Result<SearchPage, String> {
    live_lexical_page(
        state,
        limit,
        offset,
        sort,
        |raw_limit, raw_offset, sort| async move {
            let account_id = account_id.map(AccountId::as_str);
            state
                .search
                .search_in_account(query, account_id.as_deref(), raw_limit, raw_offset, sort)
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await
}

async fn live_lexical_page<F, Fut>(
    state: &AppState,
    limit: usize,
    offset: usize,
    sort: SortOrder,
    mut fetch_raw_page: F,
) -> Result<SearchPage, String>
where
    F: FnMut(usize, usize, SortOrder) -> Fut,
    Fut: std::future::Future<Output = Result<SearchPage, String>>,
{
    if limit == 0 {
        return Ok(SearchPage {
            results: Vec::new(),
            total: 0,
            has_more: false,
            next_offset: None,
        });
    }

    let target_live = offset.saturating_add(limit).saturating_add(1);
    let raw_chunk = limit.saturating_add(1).clamp(200, 2_000);
    let mut raw_offset = 0usize;
    let mut live_results = Vec::new();
    let mut raw_total: usize;
    let mut exhausted_raw = false;

    loop {
        let raw_page = fetch_raw_page(raw_chunk, raw_offset, sort.clone()).await?;
        raw_total = raw_page.total;
        let raw_len = raw_page.results.len();
        let live_page = filter_live_search_results(state, raw_page.results).await?;
        live_results.extend(live_page);

        if live_results.len() >= target_live {
            break;
        }
        if !raw_page.has_more || raw_len == 0 {
            exhausted_raw = true;
            break;
        }
        raw_offset = raw_offset.saturating_add(raw_len);
    }

    let has_more = live_results.len() > offset.saturating_add(limit);
    let total = if exhausted_raw {
        live_results.len()
    } else {
        raw_total
    };
    let results = live_results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(SearchPage {
        results,
        total,
        has_more,
        next_offset: has_more.then_some(offset.saturating_add(limit)),
    })
}

async fn filter_live_search_results(
    state: &AppState,
    results: Vec<SearchResult>,
) -> Result<Vec<SearchResult>, String> {
    if results.is_empty() {
        return Ok(results);
    }

    let enabled_accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|account| account.enabled)
        .map(|account| account.id.as_str())
        .collect::<HashSet<_>>();
    if enabled_accounts.is_empty() {
        return Ok(Vec::new());
    }

    let message_ids = results
        .iter()
        .filter_map(|result| parse_search_result_message_id(result))
        .collect::<Vec<_>>();
    let envelopes = state
        .store
        .list_envelopes_by_ids(&message_ids)
        .await
        .map_err(|e| e.to_string())?;
    let live_ids = envelopes
        .into_iter()
        .filter(|envelope| enabled_accounts.contains(&envelope.account_id.as_str()))
        .map(|envelope| envelope.id.as_str())
        .collect::<HashSet<_>>();

    Ok(results
        .into_iter()
        .filter(|result| live_ids.contains(&result.message_id))
        .collect())
}

fn parse_search_result_message_id(result: &SearchResult) -> Option<MessageId> {
    Some(MessageId::from_uuid(
        uuid::Uuid::parse_str(&result.message_id).ok()?,
    ))
}

pub(super) async fn filter_dense_hits(
    state: &AppState,
    ast: &QueryNode,
    account_id: Option<&AccountId>,
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
        if account_id.is_some_and(|account_id| &envelope.account_id != account_id) {
            continue;
        }
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

#[cfg(test)]
mod owed_reply_filter_tests {
    //! Slice 2's `is:owed-reply` search operator. Verifies that the
    //! search executor intersects Tantivy results with the dynamic
    //! owed-reply thread set, so the lens is reachable as a saved
    //! search and matches `list_owed_replies`.
    use super::*;
    use crate::state::AppState;
    use mxr_core::types::{Address, Envelope, MessageDirection, MessageFlags, UnsubscribeMethod};
    use mxr_core::{AccountId, MessageId, ThreadId};
    use mxr_protocol::ResponseData;
    use mxr_search::{SearchIndexEntry, SearchUpdateBatch};
    use std::sync::Arc;

    async fn index_envelope(state: &AppState, envelope: &Envelope) {
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: envelope.clone(),
                    body: None,
                    reply_later: false,
                }],
                removed_message_ids: vec![],
            })
            .await
            .unwrap();
        state.search.commit().await.unwrap();
    }

    fn envelope_inbound(
        account_id: &AccountId,
        thread_id: &ThreadId,
        from_email: &str,
        days_ago: i64,
    ) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: format!("p-{}", uuid::Uuid::now_v7()),
            thread_id: thread_id.clone(),
            message_id_header: Some(format!("<{}@example.com>", uuid::Uuid::now_v7())),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".into()),
                email: from_email.into(),
            },
            to: vec![Address {
                name: None,
                email: "user@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "are we still on for friday?".into(),
            date: chrono::Utc::now() - chrono::Duration::days(days_ago),
            flags: MessageFlags::empty(),
            snippet: "are we still on".into(),
            has_attachments: false,
            size_bytes: 512,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 16,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    /// The acceptance criterion from
    /// `docs/reference/ai-email.md`: "`is:owed-reply` matches
    /// daemon list." After seeding a thread where the user is the
    /// bottleneck, both `list_owed_replies` and `Search { query:
    /// "is:owed-reply" }` must return the same thread set.
    #[tokio::test]
    async fn is_owed_reply_search_matches_list_owed_replies() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let thread_id = ThreadId::new();
        let envelope = envelope_inbound(&account_id, &thread_id, "alice@example.com", 10);
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();
        index_envelope(&state, &envelope).await;
        state.store.refresh_contacts().await.unwrap();

        let listed: Vec<_> = state
            .store
            .list_owed_replies(&account_id, None, None, 100)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.thread_id.as_str().clone())
            .collect();
        assert!(
            listed.contains(&thread_id.as_str().clone()),
            "test setup: list_owed_replies should include the seeded thread, got {listed:?}"
        );

        let execution = execute_search(
            &state,
            "is:owed-reply",
            100,
            0,
            None,
            SearchMode::Lexical,
            SortOrder::DateDesc,
            false,
        )
        .await
        .unwrap();
        let searched_threads: Vec<_> = execution
            .results
            .iter()
            .map(|r| r.thread_id.clone())
            .collect();
        assert!(
            searched_threads.contains(&thread_id.as_str().clone()),
            "is:owed-reply search must include the same thread; got {searched_threads:?}"
        );
    }

    /// Sending a reply moves the thread out of owed. Models the spec
    /// "Sending reply removes row." After we upsert an outbound after
    /// the inbound, neither surface should list the thread.
    #[tokio::test]
    async fn is_owed_reply_excludes_thread_after_outbound_reply() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let thread_id = ThreadId::new();
        let inbound = envelope_inbound(&account_id, &thread_id, "bob@example.com", 5);
        state
            .store
            .upsert_envelope_with_direction(&inbound, MessageDirection::Inbound)
            .await
            .unwrap();
        index_envelope(&state, &inbound).await;

        // Now upsert an outbound reply newer than the inbound.
        let mut outbound = envelope_inbound(&account_id, &thread_id, "user@example.com", 1);
        outbound.from = Address {
            name: None,
            email: "user@example.com".into(),
        };
        outbound.to = vec![Address {
            name: None,
            email: "bob@example.com".into(),
        }];
        state
            .store
            .upsert_envelope_with_direction(&outbound, MessageDirection::Outbound)
            .await
            .unwrap();
        index_envelope(&state, &outbound).await;
        state.store.refresh_contacts().await.unwrap();

        let execution = execute_search(
            &state,
            "is:owed-reply",
            100,
            0,
            None,
            SearchMode::Lexical,
            SortOrder::DateDesc,
            false,
        )
        .await
        .unwrap();
        let searched_threads: Vec<_> = execution
            .results
            .iter()
            .map(|r| r.thread_id.clone())
            .collect();
        assert!(
            !searched_threads.contains(&thread_id.as_str().clone()),
            "thread with later outbound reply must not match is:owed-reply, got {searched_threads:?}"
        );
    }

    /// Regression: searching `label:<display name>` must hit the
    /// messages tagged with that label, not return zero results
    /// because Tantivy only indexes Gmail provider IDs. Before the
    /// label-name resolver was added, the daemon shipped a workaround
    /// where users had to query by provider_id (e.g.
    /// `label:Label_101`) instead of the human-readable name they
    /// see in Gmail and the TUI.
    #[tokio::test]
    async fn label_search_by_display_name_resolves_to_provider_id() {
        use mxr_core::types::{Label, LabelKind};
        use mxr_core::LabelId;

        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        // Persist a user-defined label so `collect_labels_for_resolution`
        // can find the name → provider_id mapping.
        let label = Label {
            id: LabelId::new(),
            account_id: account_id.clone(),
            name: "Notto".into(),
            kind: LabelKind::User,
            color: None,
            provider_id: "Label_101".into(),
            unread_count: 0,
            total_count: 1,
            role: None,
        };
        state.store.upsert_label(&label).await.unwrap();

        // Index an envelope that carries the provider_id (the shape
        // sync produces). The display name "Notto" is *not* indexed.
        let thread_id = ThreadId::new();
        let mut envelope = envelope_inbound(&account_id, &thread_id, "alice@example.com", 1);
        envelope.label_provider_ids = vec!["Label_101".into()];
        state.store.upsert_envelope(&envelope).await.unwrap();
        index_envelope(&state, &envelope).await;

        let execution = execute_search(
            &state,
            "label:Notto",
            100,
            0,
            None,
            SearchMode::Lexical,
            SortOrder::DateDesc,
            false,
        )
        .await
        .unwrap();
        let hit_ids: Vec<_> = execution
            .results
            .iter()
            .map(|r| r.message_id.clone())
            .collect();
        assert!(
            hit_ids.contains(&envelope.id.as_str()),
            "label:Notto should resolve to Label_101 and match the indexed envelope; got {hit_ids:?}"
        );
    }

    /// Spaces in label names must survive the parser's quoted-value
    /// path (`label:"Follow Up"`) and the case-insensitive lookup.
    /// Equally, raw provider IDs (`label:Label_42`) must continue to
    /// work — there's no display name "Label_42", so the resolver
    /// leaves it alone and the verbatim path still hits the index.
    #[tokio::test]
    async fn label_search_handles_quoted_names_and_raw_provider_ids() {
        use mxr_core::types::{Label, LabelKind};
        use mxr_core::LabelId;

        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let label = Label {
            id: LabelId::new(),
            account_id: account_id.clone(),
            name: "Follow Up".into(),
            kind: LabelKind::User,
            color: None,
            provider_id: "Label_42".into(),
            unread_count: 0,
            total_count: 1,
            role: None,
        };
        state.store.upsert_label(&label).await.unwrap();

        let thread_id = ThreadId::new();
        let mut envelope = envelope_inbound(&account_id, &thread_id, "alice@example.com", 1);
        envelope.label_provider_ids = vec!["Label_42".into()];
        state.store.upsert_envelope(&envelope).await.unwrap();
        index_envelope(&state, &envelope).await;

        for query in [
            "label:\"Follow Up\"",
            "label:\"follow up\"",
            "label:Label_42",
        ] {
            let execution = execute_search(
                &state,
                query,
                100,
                0,
                None,
                SearchMode::Lexical,
                SortOrder::DateDesc,
                false,
            )
            .await
            .unwrap();
            let hit_ids: Vec<_> = execution
                .results
                .iter()
                .map(|r| r.message_id.clone())
                .collect();
            assert!(
                hit_ids.contains(&envelope.id.as_str()),
                "query {query:?} should match Label_42 envelope; got {hit_ids:?}"
            );
        }
    }

    #[tokio::test]
    async fn lexical_search_skips_stale_index_rows_before_pagination() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let base_time = chrono::Utc::now();
        for i in 0..10 {
            let thread_id = ThreadId::new();
            let mut stale = envelope_inbound(&account_id, &thread_id, "stale@example.com", 0);
            stale.subject = format!("triage survey stale {i}");
            stale.date = base_time - chrono::Duration::seconds(i);
            index_envelope(&state, &stale).await;
        }

        let mut live_ids = Vec::new();
        for i in 0..25 {
            let thread_id = ThreadId::new();
            let mut envelope = envelope_inbound(&account_id, &thread_id, "live@example.com", 1);
            envelope.subject = format!("triage survey live {i}");
            envelope.date = base_time - chrono::Duration::hours(1) - chrono::Duration::seconds(i);
            state.store.upsert_envelope(&envelope).await.unwrap();
            index_envelope(&state, &envelope).await;
            live_ids.push(envelope.id.as_str());
        }

        let execution = execute_search(
            &state,
            "triage",
            25,
            0,
            None,
            SearchMode::Lexical,
            SortOrder::DateDesc,
            false,
        )
        .await
        .unwrap();
        let returned_ids = execution
            .results
            .iter()
            .map(|result| result.message_id.clone())
            .collect::<Vec<_>>();

        assert_eq!(returned_ids.len(), 25);
        assert_eq!(returned_ids, live_ids);
        assert!(!execution.has_more);
    }

    #[tokio::test]
    async fn account_scoped_search_filters_before_pagination() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let default_account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let other_account_id = AccountId::new();
        let other_account = crate::test_fixtures::test_account_with_id(other_account_id.clone());
        state.store.insert_account(&other_account).await.unwrap();

        let default_thread = ThreadId::new();
        let default_envelope = envelope_inbound(
            &default_account_id,
            &default_thread,
            "alice@example.com",
            10,
        );
        state
            .store
            .upsert_envelope(&default_envelope)
            .await
            .unwrap();
        index_envelope(&state, &default_envelope).await;

        let other_thread = ThreadId::new();
        let other_envelope =
            envelope_inbound(&other_account_id, &other_thread, "alice@example.com", 0);
        state.store.upsert_envelope(&other_envelope).await.unwrap();
        index_envelope(&state, &other_envelope).await;

        let scoped = execute_search(
            &state,
            "friday",
            1,
            0,
            Some(&default_account_id),
            SearchMode::Lexical,
            SortOrder::DateDesc,
            true,
        )
        .await
        .unwrap();

        assert_eq!(scoped.results.len(), 1);
        assert_eq!(scoped.results[0].message_id, default_envelope.id.as_str());
        let explain = scoped.explain.as_ref().unwrap();
        assert_eq!(explain.final_results, 1);
        assert_eq!(explain.results.len(), 1);
        assert_eq!(
            explain.results[0].message_id.as_str(),
            default_envelope.id.as_str()
        );
    }

    #[tokio::test]
    async fn saved_search_account_id_scopes_results() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let default_account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let other_account_id = AccountId::new();
        let other_account = crate::test_fixtures::test_account_with_id(other_account_id.clone());
        state.store.insert_account(&other_account).await.unwrap();

        let default_thread = ThreadId::new();
        let default_envelope =
            envelope_inbound(&default_account_id, &default_thread, "alice@example.com", 1);
        state
            .store
            .upsert_envelope(&default_envelope)
            .await
            .unwrap();
        index_envelope(&state, &default_envelope).await;

        let other_thread = ThreadId::new();
        let other_envelope =
            envelope_inbound(&other_account_id, &other_thread, "alice@example.com", 1);
        state.store.upsert_envelope(&other_envelope).await.unwrap();
        index_envelope(&state, &other_envelope).await;

        let saved = mxr_core::types::SavedSearch {
            id: mxr_core::SavedSearchId::new(),
            account_id: Some(other_account_id.clone()),
            name: "Other Friday".into(),
            query: "friday".into(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        };
        state.store.insert_saved_search(&saved).await.unwrap();

        let response = super::super::run_saved_search(&state, "Other Friday", 10, None)
            .await
            .unwrap();
        match response {
            ResponseData::SearchResults { results, .. } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].message_id, other_envelope.id);
            }
            other => panic!("expected search results, got {other:?}"),
        }
    }
}

#[cfg(all(test, feature = "semantic-local"))]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use mxr_core::types::{AttachmentDisposition, AttachmentMeta, MessageBody, MessageMetadata};
    use std::sync::Arc;
    use tempfile::tempdir;

    fn keyword_embedder(
        _profile: mxr_core::SemanticProfile,
        texts: &[String],
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let contains = |needle: &str| text.contains(needle) as u8 as f32;
                vec![
                    contains("deployment"),
                    contains("roadmap"),
                    contains("attachment"),
                    contains("notes"),
                    1.0,
                ]
            })
            .collect())
    }

    fn failing_embedder(
        _profile: mxr_core::SemanticProfile,
        _texts: &[String],
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        Err(anyhow::anyhow!("embedder offline"))
    }

    fn text_body(
        message_id: &mxr_core::MessageId,
        text_plain: &str,
        attachments: Vec<AttachmentMeta>,
    ) -> MessageBody {
        MessageBody {
            message_id: message_id.clone(),
            text_plain: Some(text_plain.into()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    async fn enable_semantic_for_test(state: &AppState) {
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = true;
        state.set_config_for_test(config).await;
        state
            .semantic
            .set_test_embedder(keyword_embedder)
            .await
            .unwrap();
        state
            .semantic
            .use_profile(mxr_core::SemanticProfile::BgeSmallEnV15)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn execute_search_uses_dense_source_kinds_for_fielded_queries() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();
        let attachment_dir = tempdir().unwrap();
        let attachment_path = attachment_dir.path().join("deployment-notes.txt");
        std::fs::write(&attachment_path, "Attachment deployment notes").unwrap();

        let subject_message = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("semantic-subject")
            .subject("Deployment update")
            .snippet("header match")
            .build();
        let body_message = TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("semantic-body")
            .subject("Weekly update")
            .snippet("body match")
            .build();
        let attachment_message = TestEnvelopeBuilder::new()
            .account_id(account_id)
            .provider_id("semantic-attachment")
            .subject("Weekly update")
            .snippet("attachment match")
            .has_attachments(true)
            .build();

        for envelope in [&subject_message, &body_message, &attachment_message] {
            state.store.upsert_envelope(envelope).await.unwrap();
        }

        state
            .store
            .insert_body(&text_body(
                &subject_message.id,
                "General notes only",
                Vec::new(),
            ))
            .await
            .unwrap();
        state
            .store
            .insert_body(&text_body(
                &body_message.id,
                "Deployment checklist lives in the message body",
                Vec::new(),
            ))
            .await
            .unwrap();
        state
            .store
            .insert_body(&text_body(
                &attachment_message.id,
                "General notes only",
                vec![AttachmentMeta {
                    id: mxr_core::AttachmentId::new(),
                    message_id: attachment_message.id.clone(),
                    filename: "deployment-notes.txt".into(),
                    mime_type: "text/plain".into(),
                    disposition: AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: std::fs::metadata(&attachment_path).unwrap().len(),
                    local_path: Some(attachment_path.clone()),
                    provider_id: "att-1".into(),
                }],
            ))
            .await
            .unwrap();

        enable_semantic_for_test(&state).await;

        let subject_execution = execute_search(
            &state,
            "subject:deployment",
            1,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            true,
        )
        .await
        .unwrap();
        assert_eq!(subject_execution.executed_mode, SearchMode::Hybrid);
        assert_eq!(subject_execution.results.len(), 1);
        assert_eq!(
            subject_execution.results[0].message_id,
            subject_message.id.as_str()
        );
        assert_eq!(
            subject_execution
                .explain
                .as_ref()
                .and_then(|explain| explain.semantic_query.as_deref()),
            Some("deployment")
        );

        let body_execution = execute_search(
            &state,
            "body:deployment",
            1,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            false,
        )
        .await
        .unwrap();
        assert_eq!(body_execution.executed_mode, SearchMode::Hybrid);
        assert_eq!(body_execution.results.len(), 1);
        assert_eq!(
            body_execution.results[0].message_id,
            body_message.id.as_str()
        );

        let filename_execution = execute_search(
            &state,
            "filename:deployment",
            1,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            false,
        )
        .await
        .unwrap();
        assert_eq!(filename_execution.executed_mode, SearchMode::Hybrid);
        assert_eq!(filename_execution.results.len(), 1);
        assert_eq!(
            filename_execution.results[0].message_id,
            attachment_message.id.as_str()
        );
    }

    #[tokio::test]
    async fn execute_search_explains_negated_semantic_fallback() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = true;
        state.set_config_for_test(config).await;

        let execution = execute_search(
            &state,
            "body:deployment -filename:report",
            10,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            true,
        )
        .await
        .unwrap();

        assert_eq!(execution.executed_mode, SearchMode::Lexical);
        assert_eq!(
            execution
                .explain
                .as_ref()
                .and_then(|explain| explain.semantic_query.as_deref()),
            Some("deployment")
        );
        assert!(execution
            .explain
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .any(|note| note.contains("negated semantic terms")));
    }

    #[tokio::test]
    async fn execute_search_hybrid_falls_back_to_lexical_when_semantic_is_disabled() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = false;
        state.set_config_for_test(config).await;

        let account_id = state.default_account_id();
        let message = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id)
            .provider_id("lexical-body")
            .subject("Weekly update")
            .snippet("body match")
            .build();

        state.store.upsert_envelope(&message).await.unwrap();
        let body = text_body(
            &message.id,
            "Deployment checklist lives in the message body",
            Vec::new(),
        );
        state.store.insert_body(&body).await.unwrap();
        state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: vec![mxr_search::SearchIndexEntry {
                    envelope: message.clone(),
                    body: Some(body.clone()),
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await
            .unwrap();

        let execution = execute_search(
            &state,
            "body:deployment",
            10,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            true,
        )
        .await
        .unwrap();

        assert_eq!(execution.executed_mode, SearchMode::Lexical);
        assert_eq!(execution.results.len(), 1);
        assert_eq!(execution.results[0].message_id, message.id.as_str());
        assert!(execution
            .explain
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .any(|note| note.contains("semantic search disabled in config")));
    }

    #[tokio::test]
    async fn execute_search_hybrid_falls_back_to_lexical_when_semantic_backend_errors() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();
        let message = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id)
            .provider_id("lexical-body")
            .subject("Weekly update")
            .snippet("body match")
            .build();

        state.store.upsert_envelope(&message).await.unwrap();
        let body = text_body(
            &message.id,
            "Deployment checklist lives in the message body",
            Vec::new(),
        );
        state.store.insert_body(&body).await.unwrap();
        state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: vec![mxr_search::SearchIndexEntry {
                    envelope: message.clone(),
                    body: Some(body),
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await
            .unwrap();
        let mut config = state.config_snapshot();
        config.search.semantic.enabled = true;
        state.set_config_for_test(config).await;
        state
            .semantic
            .set_test_embedder(failing_embedder)
            .await
            .unwrap();

        let execution = execute_search(
            &state,
            "body:deployment",
            10,
            0,
            None,
            SearchMode::Hybrid,
            SortOrder::Relevance,
            true,
        )
        .await
        .unwrap();

        assert_eq!(execution.executed_mode, SearchMode::Lexical);
        assert_eq!(execution.results.len(), 1);
        assert_eq!(execution.results[0].message_id, message.id.as_str());
        assert!(execution
            .explain
            .as_ref()
            .unwrap()
            .notes
            .iter()
            .any(|note| note.contains("semantic retrieval failed")));
    }
}
