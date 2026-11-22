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
pub(crate) const REASON_CONTENT_VERSION: &str = "content_version";

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
    let rebuilding = search.begin_rebuild();
    search.take_clean_fallbacks();
    let total = store
        .count_all_messages()
        .await
        .map_err(|e| MxrError::Store(e.to_string()))?;
    progress(ReindexProgress::Starting { total });

    search.clear().await?;

    let mut indexed: u32 = 0;
    let mut after: Option<MessageId> = None;

    loop {
        let read_seq = search.read_seq();
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
        search.apply_rebuild_batch(batch, read_seq).await?;
    }

    progress(ReindexProgress::Complete { indexed });

    let clean_fallbacks = search.take_clean_fallbacks();
    if clean_fallbacks > 0 {
        tracing::warn!(
            count = clean_fallbacks,
            "Indexed message bodies from fallback text because cleaning failed"
        );
    }

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
    async fn reindex_across_many_batches_indexes_every_message_once() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let total = state.store.count_all_messages().await.unwrap();
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

    fn page_of(envelope: &mxr_core::Envelope) -> SearchUpdateBatch {
        SearchUpdateBatch {
            entries: vec![SearchIndexEntry {
                envelope: envelope.clone(),
                body: None,
                reply_later: false,
            }],
            removed_message_ids: Vec::new(),
        }
    }

    async fn subject_hits(state: &AppState, word: &str) -> Vec<String> {
        state
            .search
            .search(word, 10, 0, mxr_core::types::SortOrder::Relevance)
            .await
            .unwrap()
            .results
            .into_iter()
            .map(|result| result.message_id)
            .collect()
    }

    async fn stored_with_subject(state: &AppState, subject: &str) -> mxr_core::Envelope {
        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(state.default_account_id())
            .subject(subject)
            .build();
        state.store.upsert_envelope(&envelope).await.unwrap();
        envelope
    }

    #[tokio::test]
    async fn a_live_update_after_a_page_read_is_not_overwritten_by_that_page() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let stored = stored_with_subject(&state, "stalepageword").await;
        let rebuild = state.search.begin_rebuild();

        let read_seq = state.search.read_seq();
        let old_page = page_of(&stored);
        let mut newer = stored.clone();
        newer.subject = "freshpageword".to_string();
        state.store.upsert_envelope(&newer).await.unwrap();
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: newer.clone(),
                    body: None,
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await
            .unwrap();
        assert_eq!(state.search.live_record_len().await.unwrap(), 1);

        state
            .search
            .apply_rebuild_batch(old_page, read_seq)
            .await
            .unwrap();
        drop(rebuild);

        assert_eq!(
            subject_hits(&state, "freshpageword").await,
            [stored.id.as_str()]
        );
        assert!(subject_hits(&state, "stalepageword").await.is_empty());
        assert_eq!(state.search.live_record_len().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_live_delete_after_a_page_read_keeps_the_message_out_of_the_index() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let stored = stored_with_subject(&state, "deletedpageword").await;
        let rebuild = state.search.begin_rebuild();

        let read_seq = state.search.read_seq();
        let old_page = page_of(&stored);
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: vec![stored.id.clone()],
            })
            .await
            .unwrap();
        state
            .search
            .apply_rebuild_batch(old_page, read_seq)
            .await
            .unwrap();
        drop(rebuild);

        assert!(subject_hits(&state, "deletedpageword").await.is_empty());
    }

    #[tokio::test]
    async fn an_overlapping_guard_ending_keeps_the_record_a_running_rebuild_needs() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let stored = stored_with_subject(&state, "deletedduringword").await;

        let a = state.search.begin_rebuild();
        let b = state.search.begin_rebuild();
        let read_seq = state.search.read_seq();
        let a_page = page_of(&stored);
        state
            .search
            .apply_batch(SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: vec![stored.id.clone()],
            })
            .await
            .unwrap();
        drop(b);
        assert_eq!(state.search.live_record_len().await.unwrap(), 1);

        state
            .search
            .apply_rebuild_batch(a_page, read_seq)
            .await
            .unwrap();
        assert!(subject_hits(&state, "deletedduringword").await.is_empty());

        drop(a);
        assert_eq!(state.search.live_record_len().await.unwrap(), 0);
    }
}
