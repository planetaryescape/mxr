//! Resolving a search to every message it matches, for bulk actions.
//!
//! `Request::Search` answers a page at a time, and each page rescans from the
//! first hit. A bulk action needs all of them, so this scans once. It
//! compiles the query through the same helper `execute_search_indexed` uses
//! and applies the same filters, so the set it returns is the set the paged
//! search would list end to end:
//!
//! - lexical, newest first, trash and spam excluded unless the query asks;
//! - label display names resolved, the account scope applied in the query;
//! - only messages in a readable account that are still in the store;
//! - `is:owed-reply` applied once to the complete set.
//!
//! Semantic and hybrid modes are out: they rank by relevance, which is not a
//! stable meaning for "every match".

use super::search_execute::{compile_query, owed_thread_ids, CompiledQuery, INCOMPLETE_NOTICE};
use super::search_fallback::{needs_full_index, SearchError};
use crate::handler::HandlerError;
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::SearchDegradedReason;
use mxr_search::{IndexAvailability, MxrSchema, QueryBuilder, ResolveTarget};
use std::collections::{HashMap, HashSet};

const WHAT: &str = "Acting on every search match";

/// Everything a search matches, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedSelection {
    pub ids: Vec<MessageId>,
    /// Distinct threads among `ids`.
    pub threads: u32,
    /// Messages per account, in the order each account first appears.
    pub accounts: Vec<(AccountId, u32)>,
}

pub(crate) async fn resolve_search_selection(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
) -> Result<ResolvedSelection, HandlerError> {
    Ok(resolve(state, query, account_id).await?)
}

async fn resolve(
    state: &AppState,
    query: &str,
    account_id: Option<&AccountId>,
) -> Result<ResolvedSelection, SearchError> {
    let epoch = match state.search.availability() {
        IndexAvailability::Ready { epoch } => epoch,
        IndexAvailability::Stopped => {
            return Err(needs_full_index(SearchDegradedReason::IndexStopped, WHAT))
        }
        IndexAvailability::Rebuilding => {
            return Err(needs_full_index(
                SearchDegradedReason::IndexRebuilding,
                WHAT,
            ))
        }
        IndexAvailability::Incomplete => {
            return Err(SearchError::IndexUnavailable(format!(
                "{WHAT} needs the full index. {INCOMPLETE_NOTICE}"
            )))
        }
    };

    let (target, needs_owed_filter) = match compile_query(state, query).await? {
        CompiledQuery::Structured {
            ast,
            needs_owed_filter,
        } => {
            let schema = MxrSchema::build();
            let builder = QueryBuilder::new(&schema);
            let compiled = match account_id {
                Some(account_id) => builder.build_in_account(&ast, &account_id.as_str()),
                None => builder.build(&ast),
            };
            (ResolveTarget::Ast(compiled), needs_owed_filter)
        }
        CompiledQuery::TextFallback { .. } => (
            ResolveTarget::Text {
                query: query.to_string(),
                account_id: account_id.map(AccountId::as_str),
            },
            false,
        ),
    };

    let hits = state.search.resolve_date_desc(target).await.map_err(|e| {
        SearchError::Other(format!("search failed while selecting every match: {e}"))
    })?;
    // A rebuild that started or ended under the scan may have shown it a
    // half-filled index. Throw the answer away rather than act on it.
    if !state.search.still_ready(epoch) {
        return Err(SearchError::Other(
            "the search index changed while selecting matches; try again".to_string(),
        ));
    }

    let visible: HashSet<AccountId> = state
        .store
        .list_cached_mail_accounts()
        .await
        .map_err(|e| SearchError::Other(e.to_string()))?
        .into_iter()
        .map(|account| account.id)
        .collect();
    if visible.is_empty() {
        return Ok(ResolvedSelection {
            ids: Vec::new(),
            threads: 0,
            accounts: Vec::new(),
        });
    }

    // Live: still in the store, in a readable account. Checked in chunks so
    // 500k ids never become one statement.
    let mut live: Vec<(MessageId, AccountId, ThreadId, ThreadId)> = Vec::with_capacity(hits.len());
    for chunk in hits.chunks(2_000) {
        let ids: Vec<MessageId> = chunk.iter().map(|hit| hit.message_id.clone()).collect();
        let homes: HashMap<MessageId, (AccountId, ThreadId)> = state
            .store
            .stored_message_homes(&ids)
            .await
            .map_err(|e| SearchError::Other(e.to_string()))?
            .into_iter()
            .map(|(id, account, thread)| (id, (account, thread)))
            .collect();
        for hit in chunk {
            let Some((account, thread)) = homes.get(&hit.message_id) else {
                continue;
            };
            if visible.contains(account) {
                live.push((
                    hit.message_id.clone(),
                    account.clone(),
                    thread.clone(),
                    hit.thread_id.clone(),
                ));
            }
        }
    }

    if needs_owed_filter {
        // The whole set at once, with no per-account cap: a cap would drop
        // owed threads without a word, and a selection must be exact.
        let accounts: Vec<String> = live
            .iter()
            .map(|(_, account, _, _)| account.as_str())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let (owed, failures) = owed_thread_ids(state, &accounts, u32::MAX).await;
        if let Some((account, error)) = failures.into_iter().next() {
            return Err(SearchError::Other(format!(
                "could not work out which threads are owed a reply in account {account}: {error}"
            )));
        }
        // `is:owed-reply` compares the thread the index holds, as the paged
        // search does.
        live.retain(|(_, _, _, indexed_thread)| owed.contains(&indexed_thread.as_str()));
    }

    let mut seen = HashSet::with_capacity(live.len());
    let mut threads = HashSet::new();
    let mut per_account: Vec<(AccountId, u32)> = Vec::new();
    let mut ids = Vec::with_capacity(live.len());
    for (message_id, account, thread, _) in live {
        if !seen.insert(message_id.clone()) {
            continue;
        }
        threads.insert(thread);
        match per_account.iter_mut().find(|(known, _)| *known == account) {
            Some((_, count)) => *count += 1,
            None => per_account.push((account, 1)),
        }
        ids.push(message_id);
    }
    Ok(ResolvedSelection {
        threads: threads.len() as u32,
        accounts: per_account,
        ids,
    })
}
