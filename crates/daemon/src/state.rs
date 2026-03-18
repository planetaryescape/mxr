use mxr_core::*;
use mxr_protocol::IpcMessage;
use mxr_search::SearchIndex;
use mxr_store::Store;
use mxr_sync::SyncEngine;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct AppState {
    pub store: Arc<Store>,
    pub search: Arc<Mutex<SearchIndex>>,
    pub sync_engine: Arc<SyncEngine>,
    pub provider: Arc<dyn MailSyncProvider>,
    pub event_tx: broadcast::Sender<IpcMessage>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        let data_dir = Self::data_dir();
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("mxr.db");
        let index_path = data_dir.join("search_index");
        std::fs::create_dir_all(&index_path)?;

        let store = Arc::new(Store::new(&db_path).await?);
        let search = Arc::new(Mutex::new(SearchIndex::open(&index_path)?));
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        // Phase 0: use FakeProvider
        let account_id = AccountId::new();
        let account = Account {
            id: account_id.clone(),
            name: "Fake Account".to_string(),
            email: "user@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await?;

        let provider: Arc<dyn MailSyncProvider> =
            Arc::new(mxr_provider_fake::FakeProvider::new(account_id));

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            store,
            search,
            sync_engine,
            provider,
            event_tx,
        })
    }

    pub fn data_dir() -> std::path::PathBuf {
        if cfg!(target_os = "macos") {
            dirs::home_dir()
                .unwrap()
                .join("Library/Application Support/mxr")
        } else {
            dirs::data_dir().unwrap().join("mxr")
        }
    }

    /// Create an in-memory AppState for tests.
    #[cfg(test)]
    pub async fn in_memory() -> anyhow::Result<Self> {
        let store = Arc::new(Store::in_memory().await?);
        let search = Arc::new(Mutex::new(SearchIndex::in_memory()?));
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        let account_id = AccountId::new();
        let account = Account {
            id: account_id.clone(),
            name: "Fake Account".to_string(),
            email: "user@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await?;

        let provider: Arc<dyn MailSyncProvider> =
            Arc::new(mxr_provider_fake::FakeProvider::new(account_id));

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            store,
            search,
            sync_engine,
            provider,
            event_tx,
        })
    }

    pub fn socket_path() -> std::path::PathBuf {
        if cfg!(target_os = "macos") {
            dirs::home_dir()
                .unwrap()
                .join("Library/Application Support/mxr/mxr.sock")
        } else {
            dirs::runtime_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                .join("mxr/mxr.sock")
        }
    }
}
