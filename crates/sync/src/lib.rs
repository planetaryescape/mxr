mod engine;
pub use engine::SyncEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::*;
    use mxr_core::types::*;
    use mxr_search::SearchIndex;
    use mxr_store::Store;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn test_account(account_id: AccountId) -> mxr_core::Account {
        mxr_core::Account {
            id: account_id,
            name: "Fake Account".to_string(),
            email: "user@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn sync_populates_store_and_search() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = Arc::new(Mutex::new(SearchIndex::in_memory().unwrap()));
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());
        let count = engine.sync_account(&provider).await.unwrap();
        assert_eq!(count, 55);

        // Verify store
        let envelopes = store
            .list_envelopes_by_account(&account_id, 100, 0)
            .await
            .unwrap();
        assert_eq!(envelopes.len(), 55);

        // Verify search
        let results = search.lock().await.search("deployment", 10).unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn body_caching() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = Arc::new(Mutex::new(SearchIndex::in_memory().unwrap()));
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());
        engine.sync_account(&provider).await.unwrap();

        // Get first message
        let envelopes = store
            .list_envelopes_by_account(&account_id, 1, 0)
            .await
            .unwrap();
        let msg_id = &envelopes[0].id;

        // First fetch — from provider
        let body = engine.fetch_body(&provider, msg_id).await.unwrap();
        assert!(body.text_plain.is_some());

        // Second fetch — from cache (should still work)
        let body2 = engine.fetch_body(&provider, msg_id).await.unwrap();
        assert_eq!(body.text_plain, body2.text_plain);
    }

    #[tokio::test]
    async fn snooze_wake() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let search = Arc::new(Mutex::new(SearchIndex::in_memory().unwrap()));
        let engine = SyncEngine::new(store.clone(), search.clone());

        let account_id = AccountId::new();
        store
            .insert_account(&test_account(account_id.clone()))
            .await
            .unwrap();

        let provider = mxr_provider_fake::FakeProvider::new(account_id.clone());
        engine.sync_account(&provider).await.unwrap();

        // Get a message to snooze
        let envelopes = store
            .list_envelopes_by_account(&account_id, 1, 0)
            .await
            .unwrap();

        let snoozed = Snoozed {
            message_id: envelopes[0].id.clone(),
            account_id: account_id.clone(),
            snoozed_at: chrono::Utc::now(),
            wake_at: chrono::Utc::now() - chrono::Duration::hours(1),
            original_labels: vec![],
        };
        store.insert_snooze(&snoozed).await.unwrap();

        let woken = engine.check_snoozes().await.unwrap();
        assert_eq!(woken.len(), 1);

        // Should be gone now
        let woken2 = engine.check_snoozes().await.unwrap();
        assert_eq!(woken2.len(), 0);
    }
}
