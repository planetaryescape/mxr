use mxr_core::*;
use mxr_protocol::IpcMessage;
use mxr_search::SearchIndex;
use mxr_store::Store;
use mxr_sync::SyncEngine;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, Mutex};

pub struct AppState {
    pub store: Arc<Store>,
    pub search: Arc<Mutex<SearchIndex>>,
    pub sync_engine: Arc<SyncEngine>,
    pub provider: Arc<dyn MailSyncProvider>,
    pub send_provider: Option<Arc<dyn MailSendProvider>>,
    pub event_tx: broadcast::Sender<IpcMessage>,
    pub start_time: Instant,
    #[allow(dead_code)] // Used by CLI commands, sync loop config
    pub config: mxr_config::MxrConfig,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        let config = mxr_config::load_config().unwrap_or_default();
        let data_dir = mxr_config::data_dir();
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("mxr.db");
        let index_path = data_dir.join("search_index");
        std::fs::create_dir_all(&index_path)?;

        let store = Arc::new(Store::new(&db_path).await?);
        let search = Arc::new(Mutex::new(SearchIndex::open(&index_path)?));
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        let (provider, send_provider) = match Self::create_provider_from_config(&config, &store)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                anyhow::bail!(
                        "No configured account: {e}\nRun `mxr accounts add gmail` to set up your account."
                    );
            }
        };

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            store,
            search,
            sync_engine,
            provider,
            send_provider,
            event_tx,
            start_time: Instant::now(),
            config,
        })
    }

    async fn create_provider_from_config(
        config: &mxr_config::MxrConfig,
        store: &Arc<Store>,
    ) -> anyhow::Result<(Arc<dyn MailSyncProvider>, Option<Arc<dyn MailSendProvider>>)> {
        // Find the first Gmail account in config
        for (key, acct_config) in &config.accounts {
            if let Some(mxr_config::SyncProviderConfig::Gmail {
                client_id,
                client_secret,
                token_ref,
            }) = &acct_config.sync
            {
                // Use config credentials, or fall back to bundled
                let cid = client_id.clone();
                let csecret = client_secret
                    .clone()
                    .or_else(|| mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET.map(String::from))
                    .unwrap_or_default();

                let mut auth =
                    mxr_provider_gmail::auth::GmailAuth::new(cid, csecret, token_ref.clone());
                auth.load_existing().await?;

                let client = mxr_provider_gmail::client::GmailClient::new(auth);
                let account_id = AccountId::from_provider_id("gmail", &acct_config.email);

                // Ensure account exists in store
                if store.get_account(&account_id).await?.is_none() {
                    let account = Account {
                        id: account_id.clone(),
                        name: acct_config.name.clone(),
                        email: acct_config.email.clone(),
                        sync_backend: Some(BackendRef {
                            provider_kind: ProviderKind::Gmail,
                            config_key: key.clone(),
                        }),
                        send_backend: None,
                        enabled: true,
                    };
                    store.insert_account(&account).await?;
                }

                tracing::info!("Using Gmail provider for account '{key}'");
                let provider = Arc::new(mxr_provider_gmail::GmailProvider::new(account_id, client));
                // GmailProvider implements both MailSyncProvider and MailSendProvider
                let send_provider: Arc<dyn MailSendProvider> = provider.clone();
                return Ok((provider, Some(send_provider)));
            }
        }
        anyhow::bail!("no Gmail accounts in config")
    }

    pub fn data_dir() -> std::path::PathBuf {
        mxr_config::data_dir()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
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

        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id));
        let provider: Arc<dyn MailSyncProvider> = fake.clone();
        let send_provider: Option<Arc<dyn MailSendProvider>> = Some(fake);

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            store,
            search,
            sync_engine,
            provider,
            send_provider,
            event_tx,
            start_time: Instant::now(),
            config: mxr_config::MxrConfig::default(),
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
