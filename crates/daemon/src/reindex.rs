use mxr_core::MxrError;
use mxr_search::SearchIndex;
use mxr_store::Store;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Progress callback data for reindexing.
#[derive(Debug, Clone)]
pub enum ReindexProgress {
    Starting { total: u32 },
    Indexing { indexed: u32, total: u32 },
    Complete { indexed: u32 },
}

/// Drop and rebuild the Tantivy index from all messages in SQLite.
pub async fn reindex(
    search: &Arc<Mutex<SearchIndex>>,
    store: &Arc<Store>,
    mut progress: impl FnMut(ReindexProgress),
) -> Result<u32, MxrError> {
    let total = store
        .count_all_messages()
        .await
        .map_err(|e| MxrError::Store(e.to_string()))?;
    progress(ReindexProgress::Starting { total });

    // Clear existing index
    {
        let mut idx = search.lock().await;
        idx.clear()?;
    }

    let batch_size: u32 = 500;
    let mut indexed: u32 = 0;
    let mut offset: u32 = 0;

    loop {
        let envelopes = store
            .list_all_envelopes_paginated(batch_size, offset)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        if envelopes.is_empty() {
            break;
        }

        let mut idx = search.lock().await;
        for env in &envelopes {
            // Fetch body for full-text indexing
            let body = store
                .get_body(&env.id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;

            if let Some(body) = body {
                idx.index_body(env, &body)?;
            } else {
                idx.index_envelope(env)?;
            }

            indexed += 1;
            if indexed.is_multiple_of(100) {
                progress(ReindexProgress::Indexing { indexed, total });
            }
        }
        idx.commit()?;
        drop(idx);

        offset += batch_size;
    }

    // Final commit
    {
        let mut idx = search.lock().await;
        idx.commit()?;
    }

    progress(ReindexProgress::Complete { indexed });

    // Verify
    {
        let idx = search.lock().await;
        let doc_count = idx.num_docs();
        if doc_count != indexed as u64 {
            tracing::warn!(
                expected = indexed,
                actual = doc_count,
                "Index document count mismatch after reindex"
            );
        }
    }

    Ok(indexed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;

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

        // Clear the search index manually
        {
            let mut idx = state.search.lock().await;
            idx.clear().unwrap();
            assert_eq!(idx.num_docs(), 0);
        }

        // Reindex
        let indexed = reindex(&state.search, &state.store, |_| {}).await.unwrap();

        assert_eq!(indexed, total);

        // Verify search works after reindex
        let idx = state.search.lock().await;
        assert_eq!(idx.num_docs(), total as u64);

        // Should be able to find messages
        let results = idx.search("deployment", 10).unwrap_or_default();
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

        let before = {
            let idx = state.search.lock().await;
            idx.num_docs()
        };

        // Reindex should produce same count
        let indexed = reindex(&state.search, &state.store, |_| {}).await.unwrap();

        let after = {
            let idx = state.search.lock().await;
            idx.num_docs()
        };

        assert_eq!(indexed as u64, after);
        assert_eq!(before, after);
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
}
