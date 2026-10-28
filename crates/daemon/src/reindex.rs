#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use mxr_core::id::MessageId;
use mxr_core::MxrError;
use mxr_search::{SearchIndexEntry, SearchServiceHandle, SearchUpdateBatch};
use mxr_store::Store;

/// Progress callback data for reindexing.
#[derive(Debug, Clone)]
pub enum ReindexProgress {
    Starting { total: u32 },
    Indexing { indexed: u32, total: u32 },
    Complete { indexed: u32 },
}

const REINDEX_BATCH_SIZE: u32 = 500;

/// Why a rebuild ran. Recorded on the "Lexical index rebuilt" event, which
/// is the only place a user can see it.
pub(crate) const REASON_FULL_REINDEX: &str = "full_reindex";
/// The indexed text changed since the index was written.
pub(crate) const REASON_CONTENT_VERSION: &str = "content_version";

/// Drop and rebuild the Tantivy index from all messages in SQLite.
pub async fn reindex(
    search: &SearchServiceHandle,
    store: &Store,
    progress: impl FnMut(ReindexProgress),
) -> Result<u32, MxrError> {
    reindex_for(search, store, REASON_FULL_REINDEX, progress).await
}

/// [`reindex`], recording `reason` on the rebuilt event.
pub(crate) async fn reindex_for(
    search: &SearchServiceHandle,
    store: &Store,
    reason: &str,
    progress: impl FnMut(ReindexProgress),
) -> Result<u32, MxrError> {
    reindex_in_batches(search, store, REINDEX_BATCH_SIZE, reason, progress).await
}

/// Whether startup must rebuild the index: it was written from older text, or
/// it does not hold every message. An empty mailbox with a current index needs
/// nothing, even if the index still has documents.
pub(crate) fn startup_rebuild_needed(
    stale_content: bool,
    total_messages: u32,
    indexed: u64,
) -> bool {
    stale_content || (total_messages != 0 && indexed != u64::from(total_messages))
}

/// Pages by primary key, not by offset. An offset page re-sorts the table and
/// walks past every earlier row, so the rebuild got slower as it went. A key
/// seek costs the same at the start and the end of the mailbox.
async fn reindex_in_batches(
    search: &SearchServiceHandle,
    store: &Store,
    batch_size: u32,
    reason: &str,
    mut progress: impl FnMut(ReindexProgress),
) -> Result<u32, MxrError> {
    // Searches answer from SQLite while the index is wiped and refilled,
    // instead of from the part of it that exists so far.
    // Only a complete rebuild clears the incomplete state. A rebuild that fails
    // part way drops this guard unfinished, so search keeps answering from SQLite.
    let rebuilding = search.begin_rebuild();
    // Count only the bodies this rebuild falls back on, not earlier syncs.
    search.take_clean_fallbacks();
    let total = store
        .count_all_messages()
        .await
        .map_err(|e| MxrError::Store(e.to_string()))?;
    progress(ReindexProgress::Starting { total });

    // Clear existing index
    search.clear().await?;

    let mut indexed: u32 = 0;
    let mut after: Option<MessageId> = None;

    loop {
        let envelopes = store
            .list_envelopes_for_reindex(after.as_ref(), batch_size)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;
        let Some(last) = envelopes.last() else {
            break;
        };
        after = Some(last.id.clone());

        let ids: Vec<MessageId> = envelopes.iter().map(|env| env.id.clone()).collect();
        let mut bodies = store
            .bodies_for_messages(&ids)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;
        let reply_later = store
            .reply_later_message_ids(&ids)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        let mut batch = SearchUpdateBatch::default();
        for env in envelopes {
            batch.entries.push(SearchIndexEntry {
                body: bodies.remove(&env.id),
                reply_later: reply_later.contains(&env.id),
                envelope: env,
            });

            indexed += 1;
            if indexed.is_multiple_of(100) {
                progress(ReindexProgress::Indexing { indexed, total });
            }
        }
        search.apply_batch(batch).await?;
    }

    progress(ReindexProgress::Complete { indexed });

    let clean_fallbacks = search.take_clean_fallbacks();
    if clean_fallbacks > 0 {
        tracing::warn!(
            count = clean_fallbacks,
            "Indexed message bodies from fallback text because cleaning failed"
        );
    }

    // Verify
    let doc_count = search.num_docs().await?;
    if doc_count != indexed as u64 {
        tracing::warn!(
            expected = indexed,
            actual = doc_count,
            "Index document count mismatch after reindex"
        );
    }

    store
        .insert_event(
            "info",
            "search",
            "Lexical index rebuilt",
            None,
            Some(&format!("reason={reason} indexed={indexed}")),
        )
        .await
        .map_err(|e| MxrError::Store(e.to_string()))?;

    rebuilding.finish();
    Ok(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::sync::Arc;

    #[tokio::test]
    async fn reindex_empty_store_produces_empty_index() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let mut progress_calls = Vec::new();
        let result = reindex(&state.search, &state.store, |p| {
            progress_calls.push(p);
        })
        .await
        .unwrap();

        assert_eq!(result, 0);
        assert!(progress_calls.len() >= 2); // Starting + Complete
    }

    #[tokio::test]
    async fn reindex_across_many_batches_indexes_every_message_once() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let total = state.store.count_all_messages().await.unwrap();
        // 55 fake messages in batches of 7: eight full-or-partial pages, with
        // the last page short.
        assert!(total > 7 * 7, "need enough messages for several batches");

        state.search.clear().await.unwrap();
        let indexed =
            reindex_in_batches(&state.search, &state.store, 7, REASON_FULL_REINDEX, |_| {})
                .await
                .unwrap();

        assert_eq!(indexed, total);
        assert_eq!(state.search.num_docs().await.unwrap(), total as u64);
    }

    #[tokio::test]
    async fn reindex_after_sync_indexes_all_messages() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync to populate store (FakeProvider creates 55 messages)
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let total = state.store.count_all_messages().await.unwrap();
        assert!(total > 0, "Store should have messages after sync");

        state.search.clear().await.unwrap();
        assert_eq!(state.search.num_docs().await.unwrap(), 0);

        // Reindex
        let indexed = reindex(&state.search, &state.store, |_| {}).await.unwrap();

        assert_eq!(indexed, total);

        // Verify search works after reindex
        assert_eq!(state.search.num_docs().await.unwrap(), total as u64);

        // Should be able to find messages
        let results = state
            .search
            .search("deployment", 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await;
        // FakeProvider messages may or may not contain "deployment",
        // but search itself should not error
        drop(results);
    }

    #[tokio::test]
    async fn reindex_replaces_existing_index() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync and index normally
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let before = state.search.num_docs().await.unwrap();

        // Reindex should produce same count
        let indexed = reindex(&state.search, &state.store, |_| {}).await.unwrap();

        let after = state.search.num_docs().await.unwrap();

        assert_eq!(indexed as u64, after);
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn reindex_backfills_reply_later_search_marker() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let env = state
            .store
            .list_all_envelopes_paginated(1, 0)
            .await
            .unwrap()
            .pop()
            .expect("seeded message");
        state
            .store
            .set_reply_later(&env.id, chrono::Utc::now())
            .await
            .unwrap();

        state.search.clear().await.unwrap();
        reindex(&state.search, &state.store, |_| {}).await.unwrap();

        let ast = mxr_search::parse_query("is:reply-later").unwrap();
        let schema = mxr_search::MxrSchema::build();
        let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
        let results = state
            .search
            .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .unwrap();

        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].message_id, env.id.as_str());
    }

    #[tokio::test]
    async fn reindex_progress_reports_correctly() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let total = state.store.count_all_messages().await.unwrap();

        let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let progress_clone = progress.clone();

        reindex(&state.search, &state.store, move |p| {
            progress_clone.lock().unwrap().push(p);
        })
        .await
        .unwrap();

        let calls = progress.lock().unwrap();
        // First call should be Starting
        assert!(matches!(
            calls.first(),
            Some(ReindexProgress::Starting { total: t }) if *t == total
        ));
        // Last call should be Complete
        assert!(matches!(
            calls.last(),
            Some(ReindexProgress::Complete { indexed: i }) if *i == total
        ));
    }

    #[test]
    fn startup_rebuilds_a_stale_index_or_a_short_one_but_not_an_empty_mailbox() {
        assert!(startup_rebuild_needed(true, 0, 0));
        assert!(startup_rebuild_needed(true, 5, 5));
        assert!(startup_rebuild_needed(false, 5, 4));
        assert!(startup_rebuild_needed(false, 5, 6));
        assert!(!startup_rebuild_needed(false, 5, 5));
        assert!(!startup_rebuild_needed(false, 0, 3));
    }

    #[tokio::test]
    async fn a_content_version_rebuild_is_recorded_as_a_rebuilt_event_with_its_reason() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state.search.clear().await.unwrap();

        reindex_for(&state.search, &state.store, REASON_CONTENT_VERSION, |_| {})
            .await
            .unwrap();

        let rebuilt: Vec<_> = state
            .store
            .list_events(50, None, Some("search"))
            .await
            .unwrap()
            .into_iter()
            .filter(|event| event.summary.starts_with("Lexical index rebuilt"))
            .collect();
        assert_eq!(rebuilt.len(), 1);
        let details = rebuilt[0].details.as_deref().unwrap_or_default();
        assert!(
            details.starts_with("reason=content_version "),
            "got {details:?}"
        );
        // The retention predicate keeps the newest rebuilt row, so this reason
        // survives pruning for doctor and status to read.
        assert_eq!(rebuilt[0].summary, "Lexical index rebuilt");
    }
}
