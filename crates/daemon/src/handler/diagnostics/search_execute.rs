use super::search_filter::{
    has_negated_semantic_terms, matches_structured_filters, semantic_query_plan,
};
use super::{build_execution, ExecutionExplainInput, SearchExecution};
use crate::state::AppState;
use mxr_core::types::{SearchMode, SortOrder};
use mxr_search::{ast::QueryNode, parse_query, MxrSchema, QueryBuilder, SearchPage, SearchResult};
use mxr_semantic::{should_use_semantic, SemanticHit};
use std::collections::{HashMap, HashSet};

use super::{paginate_results, sort_results};
use crate::handler::should_fallback_to_tantivy;
use mxr_protocol::SearchExplain;

#[derive(Clone)]
struct SearchExecutionOptions {
    limit: usize,
    offset: usize,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
}

pub(super) async fn execute_search(
    state: &AppState,
    query: &str,
    limit: usize,
    offset: usize,
    mode: SearchMode,
    sort: SortOrder,
    explain: bool,
) -> Result<SearchExecution, String> {
    let options = SearchExecutionOptions {
        limit,
        offset,
        mode,
        sort,
        explain,
    };
    let mut execution = match parse_query(query) {
        Ok(ast) => execute_search_ast(state, query, &ast, &options).await?,
        Err(error) => {
            if should_fallback_to_tantivy(query, &error) {
                let page = state
                    .search
                    .search(query, options.limit, options.offset, options.sort)
                    .await
                    .map_err(|e| e.to_string())?;
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
                SearchExecution {
                    results: page.results,
                    has_more: page.has_more,
                    executed_mode: SearchMode::Lexical,
                    explain,
                }
            } else {
                return Err(format!("Invalid search query: {error}"));
            }
        }
    };
    filter_disabled_accounts(state, &mut execution).await?;
    Ok(execution)
}

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
            lexical_page.has_more,
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
            page.has_more,
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
            page.has_more,
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
            page.has_more,
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
            page.has_more,
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
    let semantic_hits = state
        .semantic
        // Lexical search remains the exact/literal path. Dense retrieval only
        // broadens recall inside the source kinds implied by the parsed query.
        .search(&semantic_query, dense_window, &semantic_plan.source_kinds)
        .await
        .map_err(|e| e.to_string())?;

    let dense_results = filter_dense_hits(state, ast, semantic_hits).await?;
    if options.mode == SearchMode::Semantic {
        if dense_results.is_empty() {
            let page = paginate_results(lexical_results.clone(), options.offset, options.limit);
            return Ok(build_execution(
                options.mode,
                SearchMode::Lexical,
                page.results,
                page.has_more,
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
            page.has_more,
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
        page.has_more,
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
    limit: usize,
    offset: usize,
    sort: SortOrder,
) -> Result<SearchPage, String> {
    let schema = MxrSchema::build();
    let builder = QueryBuilder::new(&schema);
    let tantivy_query = builder.build(ast);
    state
        .search
        .search_ast(tantivy_query, limit, offset, sort)
        .await
        .map_err(|e| e.to_string())
}

pub(super) async fn filter_dense_hits(
    state: &AppState,
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

        state
            .semantic
            .ingest_messages(&[
                subject_message.id.clone(),
                body_message.id.clone(),
                attachment_message.id.clone(),
            ])
            .await
            .unwrap();
        enable_semantic_for_test(&state).await;

        let subject_execution = execute_search(
            &state,
            "subject:deployment",
            1,
            0,
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
}
