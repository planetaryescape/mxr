use super::search_execute::SearchExecutionOptions;
use super::search_filter::{default_exclusion_of, exclude_trash_and_spam_by_default, Hidden};
use super::{build_execution, ExecutionExplainInput, SearchExecution};
use crate::handler::{should_fallback_to_tantivy, HandlerError};
use crate::state::AppState;
use mxr_core::types::{SearchMode, SortOrder};
use mxr_protocol::{SearchDegraded, SearchDegradedReason};
use mxr_search::ast::{FilterKind, QueryField, QueryNode};
use mxr_search::query_builder::tokenize_text_value;
use mxr_search::{parse_query, SearchResult};
use mxr_store::{AddressOrWords, FtsClause, FtsColumns, FtsFallbackQuery};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug)]
pub(crate) enum SearchError {
    IndexUnavailable(String),
    Other(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IndexUnavailable(message) | Self::Other(message) => f.write_str(message),
        }
    }
}

impl From<String> for SearchError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

impl From<SearchError> for String {
    fn from(error: SearchError) -> Self {
        error.to_string()
    }
}

impl From<SearchError> for HandlerError {
    fn from(error: SearchError) -> Self {
        match error {
            SearchError::IndexUnavailable(message) => Self::SearchIndexUnavailable(message),
            SearchError::Other(message) => Self::Message(message),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct FallbackPlan {
    pub(super) clauses: Vec<FtsClause>,
    pub(super) sender_addresses: Vec<String>,
    pub(super) address_words: Vec<AddressOrWords>,
    pub(super) hide_trash: bool,
    pub(super) hide_spam: bool,
}

pub(super) fn plan_fallback(ast: &QueryNode) -> Result<FallbackPlan, Vec<String>> {
    let mut plan = FallbackPlan {
        clauses: Vec::new(),
        sender_addresses: Vec::new(),
        address_words: Vec::new(),
        hide_trash: false,
        hide_spam: false,
    };
    let mut unsupported = Vec::new();
    collect(ast, &mut plan, &mut unsupported);
    if unsupported.is_empty()
        && plan.clauses.is_empty()
        && plan.sender_addresses.is_empty()
        && plan.address_words.is_empty()
    {
        unsupported.push("a query with no words to search for".to_string());
    }
    unsupported.dedup();
    if unsupported.is_empty() {
        Ok(plan)
    } else {
        Err(unsupported)
    }
}

fn collect(node: &QueryNode, plan: &mut FallbackPlan, unsupported: &mut Vec<String>) {
    if let Some(place) = default_exclusion_of(node) {
        match place {
            Hidden::Trash => plan.hide_trash = true,
            Hidden::Spam => plan.hide_spam = true,
        }
        return;
    }
    match node {
        QueryNode::And(left, right) => {
            collect(left, plan, unsupported);
            collect(right, plan, unsupported);
        }
        QueryNode::Text(text) if text.contains('@') => {
            let words = words_of(text, node, unsupported);
            if !words.is_empty() {
                plan.address_words.push(AddressOrWords {
                    address: text.clone(),
                    words: words
                        .into_iter()
                        .map(|word| FtsClause {
                            columns: FtsColumns::Any,
                            text: word,
                        })
                        .collect(),
                });
            }
        }
        QueryNode::Text(text) | QueryNode::Exact(text) => {
            let words = words_of(text, node, unsupported);
            plan.clauses.extend(words.into_iter().map(|word| FtsClause {
                columns: FtsColumns::Any,
                text: word,
            }));
        }
        QueryNode::Phrase(text) => {
            let words = words_of(text, node, unsupported);
            if !words.is_empty() {
                plan.clauses.push(FtsClause {
                    columns: FtsColumns::Any,
                    text: words.join(" "),
                });
            }
        }
        QueryNode::Field {
            field: QueryField::Subject,
            value,
        } => {
            let words = words_of(value, node, unsupported);
            if !words.is_empty() {
                plan.clauses.push(FtsClause {
                    columns: FtsColumns::Subject,
                    text: words.join(" "),
                });
            }
        }
        QueryNode::Field {
            field: QueryField::From,
            value,
        } => plan.sender_addresses.push(value.clone()),
        QueryNode::Filter(FilterKind::Anywhere) => {}
        QueryNode::Or(..) => unsupported.push("OR".to_string()),
        other => unsupported.push(other.to_string()),
    }
}

fn words_of(text: &str, node: &QueryNode, unsupported: &mut Vec<String>) -> Vec<String> {
    let composed: String = text.nfc().collect();
    let words = tokenize_text_value(&composed);
    if words.is_empty() {
        unsupported.push(format!("{node} (no letters or digits)"));
    }
    words
}

pub(super) fn needs_full_index(reason: SearchDegradedReason, what: &str) -> SearchError {
    SearchError::IndexUnavailable(SearchDegraded::needs_full_index(reason, what))
}

pub(super) async fn limited_search(
    state: &AppState,
    query: &str,
    options: &SearchExecutionOptions,
    reason: SearchDegradedReason,
) -> Result<SearchExecution, SearchError> {
    let ast = match parse_query(query) {
        Ok(ast) => ast,
        Err(error) if should_fallback_to_tantivy(query, &error) => {
            return Err(SearchError::IndexUnavailable(SearchDegraded::refusal(
                reason,
                &[format!("free-form text {query:?}")],
            )));
        }
        Err(error) => return Err(SearchError::Other(format!("Invalid search query: {error}"))),
    };
    let plan = plan_fallback(&exclude_trash_and_spam_by_default(ast)).map_err(|unsupported| {
        SearchError::IndexUnavailable(SearchDegraded::refusal(reason, &unsupported))
    })?;

    let visible_accounts = state
        .store
        .list_cached_mail_accounts()
        .await
        .map_err(|error| SearchError::Other(error.to_string()))?
        .into_iter()
        .map(|account| account.id)
        .collect();
    let page = state
        .store
        .search_fts_fallback(
            &FtsFallbackQuery {
                clauses: plan.clauses,
                sender_addresses: plan.sender_addresses,
                address_words: plan.address_words,
                visible_accounts,
                account_id: options.account_id.clone(),
                hide_trash: plan.hide_trash,
                hide_spam: plan.hide_spam,
                oldest_first: options.sort == SortOrder::DateAsc,
            },
            options.limit,
            options.offset,
        )
        .await
        .map_err(|error| SearchError::Other(error.to_string()))?;

    let results = page
        .hits
        .into_iter()
        .map(|hit| SearchResult {
            message_id: hit.message_id.as_str(),
            account_id: hit.account_id.as_str(),
            thread_id: hit.thread_id.as_str(),
            score: 0.0,
        })
        .collect::<Vec<_>>();
    let next_offset = page
        .has_more
        .then_some(options.offset.saturating_add(options.limit));
    let mut notes = vec![format!(
        "lexical index unavailable ({}); answered from SQLite over {} only",
        match reason {
            SearchDegradedReason::IndexStopped => "stopped",
            SearchDegradedReason::IndexRebuilding => "rebuilding",
        },
        SearchDegraded::new(reason).searched_fields.join(", "),
    )];
    if options.mode != SearchMode::Lexical {
        notes.push("semantic ranking skipped; results are newest first".to_string());
    }
    let mut execution = build_execution(
        options.mode,
        SearchMode::Lexical,
        results.clone(),
        page.total,
        page.has_more,
        next_offset,
        ExecutionExplainInput {
            include_explain: options.explain,
            semantic_query: None,
            lexical_window: options.limit,
            dense_window: None,
            lexical_results: &results,
            dense_results: &[],
            rrf_k: None,
            notes,
        },
    );
    execution.degraded = Some(SearchDegraded::new(reason));
    Ok(execution)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;

    fn plan(query: &str) -> Result<FallbackPlan, Vec<String>> {
        plan_fallback(&exclude_trash_and_spam_by_default(
            parse_query(query).unwrap(),
        ))
    }

    fn clause(columns: FtsColumns, text: &str) -> FtsClause {
        FtsClause {
            columns,
            text: text.to_string(),
        }
    }

    #[test]
    fn plain_words_search_subject_sender_and_preview() {
        let plan = plan("invoice paid").unwrap();
        assert_eq!(
            plan.clauses,
            vec![
                clause(FtsColumns::Any, "invoice"),
                clause(FtsColumns::Any, "paid")
            ]
        );
    }

    #[test]
    fn trash_and_spam_are_hidden_unless_the_query_lifts_them() {
        let default = plan("invoice").unwrap();
        assert!(default.hide_trash && default.hide_spam);

        let anywhere = plan("invoice in:anywhere").unwrap();
        assert!(!anywhere.hide_trash && !anywhere.hide_spam);
    }

    #[test]
    fn every_unsupported_kind_of_node_is_refused_by_name() {
        for (query, named) in [
            ("to:bob", "to:bob"),
            ("cc:bob", "cc:bob"),
            ("body:invoice", "body:invoice"),
            ("filename:report", "filename:report"),
            ("list:news", "list:news"),
            ("label:work", "label:work"),
            ("category:promotions", "label:CATEGORY_PROMOTIONS"),
            ("is:unread", "is:unread"),
            ("is:starred", "is:starred"),
            ("has:attachment", "has:attachment"),
            ("is:owed-reply", "is:owed-reply"),
            ("in:trash", "is:trash"),
            ("after:2024-01-01", "after:2024-01-01"),
            ("older_than:5d", "older_than:5d"),
            ("size:>5M", "size:>5242880"),
            ("foo AROUND 3 bar", "foo AROUND 3 bar"),
            ("-spam", "-spam"),
            ("invoice -spam", "-spam"),
            ("alice OR bob", "OR"),
        ] {
            let refused = plan(query).expect_err(query);
            assert!(
                refused.iter().any(|part| part.contains(named)),
                "{query:?} should be refused as {named:?}, got {refused:?}"
            );
        }
    }

    #[test]
    fn a_refusal_names_every_unsupported_part_and_keeps_none_of_the_rest() {
        let refused = plan("invoice is:unread after:2024-01-01").unwrap_err();
        assert_eq!(
            refused,
            vec!["is:unread".to_string(), "after:2024-01-01".to_string()]
        );
    }

    #[test]
    fn an_or_never_runs_even_when_each_side_is_supported() {
        assert!(plan("alice OR bob").is_err());
        assert!(plan("subject:alice OR from:bob").is_err());
    }

    #[test]
    fn a_bare_address_is_one_group_matching_the_address_or_its_words() {
        let plan = plan("alice@example.com").unwrap();
        assert!(plan.clauses.is_empty());
        assert_eq!(
            plan.address_words,
            vec![AddressOrWords {
                address: "alice@example.com".to_string(),
                words: ["alice", "example", "com"]
                    .into_iter()
                    .map(|word| clause(FtsColumns::Any, word))
                    .collect(),
            }]
        );
    }

    #[test]
    fn a_decomposed_accent_splits_like_the_precomposed_word() {
        let decomposed = plan("nai\u{308}ve-beta").unwrap();
        let precomposed = plan("na\u{ef}ve-beta").unwrap();
        assert_eq!(decomposed.clauses, precomposed.clauses);
        assert!(
            !decomposed
                .clauses
                .iter()
                .any(|clause| clause.text == "nai" || clause.text == "ve"),
            "got {:?}",
            decomposed.clauses
        );
    }
}
