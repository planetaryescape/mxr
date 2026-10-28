//! Answering a search from SQLite when the lexical index is unavailable.
//!
//! The fallback reads `messages_fts`, which holds subject, sender and snippet
//! only. It runs a query only when every part of it is something that table
//! can answer the same way the index would; anything else is refused by name,
//! because a quietly loosened query returns wrong mail without saying so.

use super::search_execute::SearchExecutionOptions;
use super::search_filter::{default_exclusion_of, exclude_trash_and_spam_by_default, Hidden};
use super::{build_execution, ExecutionExplainInput, SearchExecution};
use crate::handler::{should_fallback_to_tantivy, HandlerError};
use crate::state::AppState;
use mxr_core::types::{SearchMode, SortOrder};
use mxr_protocol::{SearchDegraded, SearchDegradedReason};
use mxr_search::ast::{FilterKind, QueryField, QueryNode};
use mxr_search::{parse_query, SearchResult};
use mxr_store::{FtsClause, FtsColumns, FtsFallbackQuery};

/// Why a search failed, split so a client can tell "the index is down and this
/// cannot be served" from every other failure.
#[derive(Debug)]
pub(crate) enum SearchError {
    /// The lexical index is unavailable and the request has no limited form.
    /// The message already says what to do.
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

/// What a query asks of `messages_fts`, once every part of it is known to be
/// one the table can answer.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FallbackPlan {
    pub(super) clauses: Vec<FtsClause>,
    pub(super) hide_trash: bool,
    pub(super) hide_spam: bool,
}

/// Translates a query into a [`FallbackPlan`], or names every part of it the
/// fallback cannot honour.
///
/// Supported: words, `+words`, "phrases", `subject:`, `from:`, AND of those,
/// `in:anywhere`, and the Trash/Spam exclusion `execute_search` adds by
/// default. Everything else is refused, including every kind of NOT and OR:
/// FTS5 can express them, but not with the index's meaning of a missing field
/// or a stemmed word, and a wrong answer is worse than an explicit refusal.
pub(super) fn plan_fallback(ast: &QueryNode) -> Result<FallbackPlan, Vec<String>> {
    let mut plan = FallbackPlan {
        clauses: Vec::new(),
        hide_trash: false,
        hide_spam: false,
    };
    let mut unsupported = Vec::new();
    collect(ast, &mut plan, &mut unsupported);
    if unsupported.is_empty() && plan.clauses.is_empty() {
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
        QueryNode::Text(text) | QueryNode::Exact(text) | QueryNode::Phrase(text) => {
            push_clause(FtsColumns::Any, text, node, plan, unsupported);
        }
        QueryNode::Field {
            field: QueryField::Subject,
            value,
        } => push_clause(FtsColumns::Subject, value, node, plan, unsupported),
        QueryNode::Field {
            field: QueryField::From,
            value,
        } => push_clause(FtsColumns::Sender, value, node, plan, unsupported),
        // `in:anywhere` only lifts the Trash/Spam exclusion, which is not
        // added when it is present; there is nothing left for it to do here.
        QueryNode::Filter(FilterKind::Anywhere) => {}
        QueryNode::Or(..) => unsupported.push("OR".to_string()),
        other => unsupported.push(other.to_string()),
    }
}

fn push_clause(
    columns: FtsColumns,
    text: &str,
    node: &QueryNode,
    plan: &mut FallbackPlan,
    unsupported: &mut Vec<String>,
) {
    // A term with no letters or digits has no words in it, so the table has
    // nothing to match it against and the index would not either.
    if text.chars().any(char::is_alphanumeric) {
        plan.clauses.push(FtsClause {
            columns,
            text: text.to_string(),
        });
    } else {
        unsupported.push(format!("{node} (no letters or digits)"));
    }
}

/// The error for a request that has no limited form (counts, grouping).
pub(super) fn needs_full_index(reason: SearchDegradedReason, what: &str) -> SearchError {
    SearchError::IndexUnavailable(SearchDegraded::needs_full_index(reason, what))
}

/// Answers `query` from `messages_fts`, or refuses it with the parts of the
/// query the fallback cannot run.
pub(super) async fn limited_search(
    state: &AppState,
    query: &str,
    options: &SearchExecutionOptions,
    reason: SearchDegradedReason,
) -> Result<SearchExecution, SearchError> {
    let ast = match parse_query(query) {
        Ok(ast) => ast,
        // Free text the structured parser rejects is searched as raw text by
        // the index; the fallback has no equivalent.
        Err(error) if should_fallback_to_tantivy(query, &error) => {
            return Err(SearchError::IndexUnavailable(SearchDegraded::refusal(
                reason,
                &[format!("free-form text {query:?}")],
            )));
        }
        Err(error) => return Err(SearchError::Other(format!("Invalid search query: {error}"))),
    };
    // Plan the query as typed plus the default exclusion, not the label-
    // resolved one the index gets: a refusal should name `label:Work`, not an
    // internal provider id.
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
            // SQLite ranks nothing; results are by date.
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
    fn exact_terms_and_phrases_are_supported_as_words() {
        let plan = plan("+invoice \"quarterly report\"").unwrap();
        assert_eq!(
            plan.clauses,
            vec![
                clause(FtsColumns::Any, "invoice"),
                clause(FtsColumns::Any, "quarterly report")
            ]
        );
    }

    #[test]
    fn subject_and_from_fields_pick_their_columns() {
        let plan = plan("subject:plan from:alice@example.com").unwrap();
        assert_eq!(
            plan.clauses,
            vec![
                clause(FtsColumns::Subject, "plan"),
                clause(FtsColumns::Sender, "alice@example.com")
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
    fn a_term_without_letters_or_digits_is_refused() {
        assert!(plan("***").is_err());
    }

    #[test]
    fn a_query_with_only_a_lifting_filter_has_nothing_to_search_for() {
        let refused = plan("in:anywhere").unwrap_err();
        assert_eq!(
            refused,
            vec!["a query with no words to search for".to_string()]
        );
    }

    #[test]
    fn terms_with_fts5_syntax_are_kept_as_words_for_the_store_to_quote() {
        let plan = plan("\"alpha NEAR beta\" \"x OR y\"").unwrap();
        assert_eq!(plan.clauses.len(), 2);
    }
}
