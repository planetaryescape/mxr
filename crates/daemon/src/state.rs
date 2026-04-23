use mxr_core::id::AccountId;
use mxr_core::*;
use mxr_protocol::IpcMessage;
use mxr_search::{SearchIndex, SearchServiceHandle};
use mxr_semantic::{SemanticEngine, SemanticServiceHandle};
use mxr_store::Store;
use mxr_sync::SyncEngine;
use parking_lot::{Mutex as ParkingMutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;

pub(crate) struct ProviderSetup {
    pub providers: HashMap<AccountId, Arc<dyn MailSyncProvider>>,
    pub send_providers: HashMap<AccountId, Arc<dyn MailSendProvider>>,
    pub default_provider: Option<Arc<dyn MailSyncProvider>>,
    pub default_send_provider: Option<Arc<dyn MailSendProvider>>,
}

pub(crate) struct ProviderRuntime {
    pub providers: HashMap<AccountId, Arc<dyn MailSyncProvider>>,
    pub send_providers: HashMap<AccountId, Arc<dyn MailSendProvider>>,
    pub default_provider: Option<Arc<dyn MailSyncProvider>>,
    pub default_send_provider: Option<Arc<dyn MailSendProvider>>,
}

struct NamedTaskHandle {
    name: String,
    handle: JoinHandle<()>,
}

#[derive(Default)]
struct RuntimeTasks {
    search_worker: ParkingMutex<Option<JoinHandle<()>>>,
    semantic_worker: ParkingMutex<Option<JoinHandle<()>>>,
    sync_loops: ParkingMutex<HashMap<AccountId, JoinHandle<()>>>,
    snooze_loop: ParkingMutex<Option<JoinHandle<()>>>,
    startup_maintenance: ParkingMutex<Option<JoinHandle<()>>>,
}

impl RuntimeTasks {
    fn set_search_worker(&self, handle: JoinHandle<()>) {
        *self.search_worker.lock() = Some(handle);
    }

    fn set_semantic_worker(&self, handle: JoinHandle<()>) {
        *self.semantic_worker.lock() = Some(handle);
    }

    fn register_sync_loop(&self, account_id: AccountId, handle: JoinHandle<()>) {
        self.sync_loops.lock().insert(account_id, handle);
    }

    fn finish_sync_loop(&self, account_id: &AccountId) {
        self.sync_loops.lock().remove(account_id);
    }

    fn set_snooze_loop(&self, handle: JoinHandle<()>) {
        *self.snooze_loop.lock() = Some(handle);
    }

    fn set_startup_maintenance(&self, handle: JoinHandle<()>) {
        *self.startup_maintenance.lock() = Some(handle);
    }

    fn take_all(&self) -> Vec<NamedTaskHandle> {
        let mut handles = Vec::new();

        if let Some(handle) = self.search_worker.lock().take() {
            handles.push(NamedTaskHandle {
                name: "search_worker".to_string(),
                handle,
            });
        }
        if let Some(handle) = self.semantic_worker.lock().take() {
            handles.push(NamedTaskHandle {
                name: "semantic_worker".to_string(),
                handle,
            });
        }
        if let Some(handle) = self.snooze_loop.lock().take() {
            handles.push(NamedTaskHandle {
                name: "snooze_loop".to_string(),
                handle,
            });
        }
        if let Some(handle) = self.startup_maintenance.lock().take() {
            handles.push(NamedTaskHandle {
                name: "startup_maintenance".to_string(),
                handle,
            });
        }

        for (account_id, handle) in self.sync_loops.lock().drain() {
            handles.push(NamedTaskHandle {
                name: format!("sync_loop:{account_id}"),
                handle,
            });
        }

        handles
    }
}

pub struct AppState {
    pub store: Arc<Store>,
    pub search: SearchServiceHandle,
    pub semantic: SemanticServiceHandle,
    pub sync_engine: Arc<SyncEngine>,
    runtime: RwLock<ProviderRuntime>,
    sync_loop_accounts: ParkingMutex<HashSet<AccountId>>,
    pub event_tx: broadcast::Sender<IpcMessage>,
    pub start_time: Instant,
    config: RwLock<mxr_config::MxrConfig>,
    shutdown_tx: watch::Sender<bool>,
    runtime_tasks: RuntimeTasks,
    admin_blocking: Arc<Semaphore>,
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
        let runtime_tasks = RuntimeTasks::default();
        let (search, search_worker) =
            SearchServiceHandle::start(open_search_index(&index_path, &store).await?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(SemanticEngine::new(
            store.clone(),
            &data_dir,
            config.search.semantic.clone(),
        ));
        runtime_tasks.set_semantic_worker(semantic_worker);
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        let provider_setup = Self::create_providers_from_config(&config, &store).await?;

        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        Ok(Self {
            store,
            search,
            semantic,
            sync_engine,
            runtime: RwLock::new(ProviderRuntime {
                providers: provider_setup.providers,
                send_providers: provider_setup.send_providers,
                default_provider: provider_setup.default_provider,
                default_send_provider: provider_setup.default_send_provider,
            }),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            event_tx,
            start_time: Instant::now(),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
        })
    }

    async fn create_providers_from_config(
        config: &mxr_config::MxrConfig,
        store: &Store,
    ) -> anyhow::Result<ProviderSetup> {
        let mut providers = HashMap::new();
        let mut send_providers = HashMap::new();
        let mut default_provider = None;
        let mut default_send_provider = None;
        let requested_default = config.general.default_account.as_deref();

        for (key, acct_config) in &config.accounts {
            let provider_kind = sync_provider_kind(acct_config.sync.as_ref());
            let send_kind = send_provider_kind(acct_config.send.as_ref());
            let account_id = AccountId::from_provider_id(
                provider_kind
                    .clone()
                    .or(send_kind.clone())
                    .map(provider_kind_name)
                    .unwrap_or("account"),
                &acct_config.email,
            );

            let account = Account {
                id: account_id.clone(),
                name: acct_config.name.clone(),
                email: acct_config.email.clone(),
                sync_backend: provider_kind.map(|provider_kind| BackendRef {
                    provider_kind,
                    config_key: key.clone(),
                }),
                send_backend: send_kind.map(|provider_kind| BackendRef {
                    provider_kind,
                    config_key: key.clone(),
                }),
                enabled: true,
            };
            store.insert_account(&account).await?;

            let sync_provider = match &acct_config.sync {
                Some(mxr_config::SyncProviderConfig::Gmail {
                    credential_source: _,
                    client_id,
                    client_secret,
                    token_ref,
                }) => {
                    let cid = client_id.clone();
                    let csecret = client_secret
                        .clone()
                        .or_else(|| {
                            mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET.map(String::from)
                        })
                        .unwrap_or_default();
                    let mut auth =
                        mxr_provider_gmail::auth::GmailAuth::new(cid, csecret, token_ref.clone());
                    match auth.load_existing().await {
                        Ok(()) => {
                            let client = mxr_provider_gmail::client::GmailClient::new(auth);
                            let provider = Arc::new(mxr_provider_gmail::GmailProvider::new(
                                account_id.clone(),
                                client,
                            ));
                            let sync_provider: Arc<dyn MailSyncProvider> = provider.clone();
                            if matches!(
                                acct_config.send,
                                Some(mxr_config::SendProviderConfig::Gmail)
                            ) {
                                let send_provider: Arc<dyn MailSendProvider> = provider.clone();
                                send_providers.insert(account_id.clone(), send_provider.clone());
                                if requested_default == Some(key.as_str())
                                    || default_send_provider.is_none()
                                {
                                    default_send_provider = Some(send_provider);
                                }
                            }
                            Some(sync_provider)
                        }
                        Err(e) => {
                            tracing::warn!(
                                account = %key,
                                "Gmail auth not ready, skipping provider: {e}"
                            );
                            None
                        }
                    }
                }
                Some(mxr_config::SyncProviderConfig::Imap {
                    host,
                    port,
                    username,
                    password_ref,
                    auth_required,
                    use_tls,
                }) => Some(Arc::new(mxr_provider_imap::ImapProvider::new(
                    account_id.clone(),
                    mxr_provider_imap::config::ImapConfig::new(
                        host.clone(),
                        *port,
                        username.clone(),
                        password_ref.clone(),
                        *auth_required,
                        *use_tls,
                    ),
                )) as Arc<dyn MailSyncProvider>),
                Some(mxr_config::SyncProviderConfig::OutlookPersonal {
                    client_id,
                    token_ref,
                }) => build_outlook_sync_provider(
                    client_id,
                    token_ref,
                    mxr_provider_outlook::OutlookTenant::Personal,
                    &account_id,
                    &acct_config.email,
                    key,
                )?,
                Some(mxr_config::SyncProviderConfig::OutlookWork {
                    client_id,
                    token_ref,
                }) => build_outlook_sync_provider(
                    client_id,
                    token_ref,
                    mxr_provider_outlook::OutlookTenant::Work,
                    &account_id,
                    &acct_config.email,
                    key,
                )?,
                None => None,
            };

            if let Some(sync_provider) = sync_provider {
                if requested_default == Some(key.as_str()) || default_provider.is_none() {
                    default_provider = Some(sync_provider.clone());
                }
                providers.insert(account_id.clone(), sync_provider);
            }

            if matches!(
                acct_config.send,
                Some(mxr_config::SendProviderConfig::Gmail)
            ) && !send_providers.contains_key(&account_id)
            {
                if providers.contains_key(&account_id) {
                    anyhow::bail!("Account '{key}' uses gmail send without gmail sync");
                }
                // Gmail sync provider was skipped (e.g. auth not ready), skip send too
                tracing::warn!(
                    account = %key,
                    "Skipping Gmail send provider: sync provider not available"
                );
            }

            if let Some(mxr_config::SendProviderConfig::Smtp {
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
            }) = &acct_config.send
            {
                let send_provider = Arc::new(mxr_provider_smtp::SmtpSendProvider::new(
                    mxr_provider_smtp::config::SmtpConfig::new(
                        host.clone(),
                        *port,
                        username.clone(),
                        password_ref.clone(),
                        *auth_required,
                        *use_tls,
                    ),
                )) as Arc<dyn MailSendProvider>;
                if requested_default == Some(key.as_str()) || default_send_provider.is_none() {
                    default_send_provider = Some(send_provider.clone());
                }
                send_providers.insert(account_id.clone(), send_provider);
            }

            if let Some(
                mxr_config::SendProviderConfig::OutlookPersonal { token_ref }
                | mxr_config::SendProviderConfig::OutlookWork { token_ref },
            ) = &acct_config.send
            {
                let tenant = match &acct_config.send {
                    Some(mxr_config::SendProviderConfig::OutlookWork { .. }) => {
                        mxr_provider_outlook::OutlookTenant::Work
                    }
                    _ => mxr_provider_outlook::OutlookTenant::Personal,
                };
                // Resolve client_id from sync config (shared token) or fall back to bundled.
                let cid = match &acct_config.sync {
                    Some(
                        mxr_config::SyncProviderConfig::OutlookPersonal {
                            client_id: Some(id),
                            ..
                        }
                        | mxr_config::SyncProviderConfig::OutlookWork {
                            client_id: Some(id),
                            ..
                        },
                    ) => id.clone(),
                    _ => mxr_provider_outlook::BUNDLED_CLIENT_ID
                        .map(String::from)
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Outlook send for '{key}' has no client_id and no bundled OUTLOOK_CLIENT_ID"
                            )
                        })?,
                };
                let auth = std::sync::Arc::new(mxr_provider_outlook::OutlookAuth::new(
                    cid,
                    token_ref.clone(),
                    tenant,
                ));
                let token_fn: std::sync::Arc<
                    dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                        + Send
                        + Sync,
                > = std::sync::Arc::new(move || {
                    let auth = auth.clone();
                    Box::pin(async move {
                        auth.get_valid_access_token()
                            .await
                            .map_err(|e| anyhow::anyhow!(e))
                    })
                });
                let smtp_host = match tenant {
                    mxr_provider_outlook::OutlookTenant::Personal => {
                        "smtp-mail.outlook.com"
                    }
                    mxr_provider_outlook::OutlookTenant::Work => "smtp.office365.com",
                };
                let send_provider = Arc::new(
                    mxr_provider_outlook::OutlookSmtpSendProvider::new(
                        smtp_host.to_string(),
                        587,
                        acct_config.email.clone(),
                        token_fn,
                    ),
                ) as Arc<dyn MailSendProvider>;
                if requested_default == Some(key.as_str()) || default_send_provider.is_none() {
                    default_send_provider = Some(send_provider.clone());
                }
                send_providers.insert(account_id.clone(), send_provider);
            }
        }

        Ok(ProviderSetup {
            providers,
            send_providers,
            default_provider,
            default_send_provider,
        })
    }

    pub fn data_dir() -> std::path::PathBuf {
        mxr_config::data_dir()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn config_snapshot(&self) -> mxr_config::MxrConfig {
        self.config.read().clone()
    }

    pub fn attachment_dir(&self) -> std::path::PathBuf {
        self.config_snapshot().general.attachment_dir
    }

    pub fn hook_timeout_secs(&self) -> u64 {
        self.config_snapshot().general.hook_timeout
    }

    pub fn sync_interval_secs(&self) -> u64 {
        self.config_snapshot().general.sync_interval
    }

    pub fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.shutdown_tx.subscribe()
    }

    pub fn shutdown_requested(&self) -> bool {
        *self.shutdown_tx.borrow()
    }

    pub fn request_shutdown(&self) {
        self.shutdown_tx.send_replace(true);
    }

    pub fn register_sync_loop_handle(&self, account_id: AccountId, handle: JoinHandle<()>) {
        self.runtime_tasks.register_sync_loop(account_id, handle);
    }

    pub fn finish_sync_loop(&self, account_id: &AccountId) {
        self.sync_loop_accounts.lock().remove(account_id);
        self.runtime_tasks.finish_sync_loop(account_id);
    }

    pub fn register_snooze_loop(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_snooze_loop(handle);
    }

    pub fn register_startup_maintenance(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_startup_maintenance(handle);
    }

    pub async fn shutdown_runtime_tasks(&self, timeout: Duration) {
        let drain_started = Instant::now();
        self.request_shutdown();

        if let Err(error) = self.search.request_shutdown().await {
            tracing::warn!("search worker shutdown signal failed: {error}");
        }
        if let Err(error) = self.semantic.request_shutdown().await {
            tracing::warn!("semantic worker shutdown signal failed: {error}");
        }

        let deadline = Instant::now() + timeout;
        for task in self.runtime_tasks.take_all() {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                tracing::warn!(task = %task.name, "runtime task drain timed out");
                continue;
            };

            match tokio::time::timeout(remaining, task.handle).await {
                Ok(Ok(())) => tracing::trace!(task = %task.name, "runtime task stopped"),
                Ok(Err(error)) => {
                    tracing::warn!(task = %task.name, "runtime task join failed: {error}")
                }
                Err(_) => tracing::warn!(task = %task.name, "runtime task drain timed out"),
            }
        }

        tracing::info!(
            elapsed_ms = drain_started.elapsed().as_secs_f64() * 1000.0,
            "daemon shutdown drain completed"
        );
    }

    pub async fn acquire_admin_blocking_permit(
        &self,
    ) -> std::result::Result<OwnedSemaphorePermit, String> {
        self.admin_blocking
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "admin blocking executor unavailable".to_string())
    }

    pub async fn mutate_config<F>(
        &self,
        mutator: F,
    ) -> std::result::Result<mxr_config::MxrConfig, String>
    where
        F: FnOnce(&mut mxr_config::MxrConfig),
    {
        let mut config = self.config_snapshot();
        mutator(&mut config);
        mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        self.semantic
            .apply_config(config.search.semantic.clone())
            .await
            .map_err(|e| e.to_string())?;
        *self.config.write() = config.clone();
        Ok(config)
    }

    #[cfg(test)]
    pub async fn set_config_for_test(&self, config: mxr_config::MxrConfig) {
        self.semantic
            .apply_config(config.search.semantic.clone())
            .await
            .expect("apply semantic config");
        *self.config.write() = config;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn default_provider(&self) -> Arc<dyn MailSyncProvider> {
        self.runtime
            .read()
            .default_provider
            .clone()
            .expect("no sync-capable accounts configured")
    }

    pub fn default_account_id_opt(&self) -> Option<AccountId> {
        self.runtime
            .read()
            .default_provider
            .as_ref()
            .map(|provider| provider.account_id().clone())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn default_account_id(&self) -> AccountId {
        self.default_provider().account_id().clone()
    }

    pub fn sync_provider_for_account(
        &self,
        account_id: &AccountId,
    ) -> Option<Arc<dyn MailSyncProvider>> {
        self.runtime.read().providers.get(account_id).cloned()
    }

    /// Get provider for a specific account, or fall back to default.
    pub fn get_provider(
        &self,
        account_id: Option<&AccountId>,
    ) -> std::result::Result<Arc<dyn MailSyncProvider>, String> {
        let runtime = self.runtime.read();
        account_id
            .and_then(|id| runtime.providers.get(id).cloned())
            .or_else(|| runtime.default_provider.clone())
            .ok_or_else(|| "no sync-capable accounts configured".to_string())
    }

    /// Get send provider for a specific account, or fall back to default.
    pub fn get_send_provider(
        &self,
        account_id: Option<&AccountId>,
    ) -> Option<Arc<dyn MailSendProvider>> {
        let runtime = self.runtime.read();
        account_id
            .and_then(|id| runtime.send_providers.get(id).cloned())
            .or_else(|| runtime.default_send_provider.clone())
    }

    pub fn runtime_account_ids(&self) -> Vec<AccountId> {
        let runtime = self.runtime.read();
        runtime
            .providers
            .keys()
            .chain(runtime.send_providers.keys())
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn sync_provider_entries(&self) -> Vec<(AccountId, Arc<dyn MailSyncProvider>)> {
        self.runtime
            .read()
            .providers
            .iter()
            .map(|(account_id, provider)| (account_id.clone(), provider.clone()))
            .collect()
    }

    pub fn mark_sync_loop_spawned(&self, account_id: &AccountId) -> bool {
        self.sync_loop_accounts.lock().insert(account_id.clone())
    }

    pub async fn reload_accounts_from_disk(self: &Arc<Self>) -> std::result::Result<(), String> {
        let config = mxr_config::load_config().map_err(|e| e.to_string())?;
        let provider_setup = Self::create_providers_from_config(&config, &self.store)
            .await
            .map_err(|e| e.to_string())?;

        {
            let mut runtime = self.runtime.write();
            *runtime = ProviderRuntime {
                providers: provider_setup.providers,
                send_providers: provider_setup.send_providers,
                default_provider: provider_setup.default_provider,
                default_send_provider: provider_setup.default_send_provider,
            };
        }
        self.semantic
            .apply_config(config.search.semantic.clone())
            .await
            .map_err(|e| e.to_string())?;
        *self.config.write() = config;
        crate::loops::spawn_sync_loops(self.clone());
        Ok(())
    }

    /// Create an in-memory AppState for tests.
    #[cfg(test)]
    pub async fn in_memory() -> anyhow::Result<Self> {
        let (state, _) = Self::in_memory_with_fake().await?;
        Ok(state)
    }

    #[cfg(test)]
    pub async fn in_memory_with_sync_provider(
        account: Account,
        provider: Arc<dyn MailSyncProvider>,
        send_provider: Option<Arc<dyn MailSendProvider>>,
    ) -> anyhow::Result<Self> {
        debug_assert_eq!(provider.account_id(), &account.id);

        let mut config = mxr_config::MxrConfig::default();
        config.general.attachment_dir = test_attachment_dir();
        let store = Arc::new(Store::in_memory().await?);
        let runtime_tasks = RuntimeTasks::default();
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(SemanticEngine::new(
            store.clone(),
            &std::env::temp_dir(),
            config.search.semantic.clone(),
        ));
        runtime_tasks.set_semantic_worker(semantic_worker);
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        store.insert_account(&account).await?;

        let account_id = account.id.clone();
        let mut providers = HashMap::new();
        providers.insert(account_id.clone(), provider.clone());

        let mut send_providers = HashMap::new();
        if let Some(send_provider) = &send_provider {
            send_providers.insert(account_id.clone(), send_provider.clone());
        }

        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        Ok(Self {
            store,
            search,
            semantic,
            sync_engine,
            runtime: RwLock::new(ProviderRuntime {
                providers,
                send_providers,
                default_provider: Some(provider),
                default_send_provider: send_provider,
            }),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            event_tx,
            start_time: Instant::now(),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
        })
    }

    #[cfg(test)]
    pub async fn in_memory_without_accounts() -> anyhow::Result<Self> {
        let mut config = mxr_config::MxrConfig::default();
        config.general.attachment_dir = test_attachment_dir();
        let store = Arc::new(Store::in_memory().await?);
        let runtime_tasks = RuntimeTasks::default();
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(SemanticEngine::new(
            store.clone(),
            &std::env::temp_dir(),
            config.search.semantic.clone(),
        ));
        runtime_tasks.set_semantic_worker(semantic_worker);
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));
        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        Ok(Self {
            store,
            search,
            semantic,
            sync_engine,
            runtime: RwLock::new(ProviderRuntime {
                providers: HashMap::new(),
                send_providers: HashMap::new(),
                default_provider: None,
                default_send_provider: None,
            }),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            event_tx,
            start_time: Instant::now(),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
        })
    }

    #[cfg(test)]
    pub async fn in_memory_with_fake(
    ) -> anyhow::Result<(Self, Arc<mxr_provider_fake::FakeProvider>)> {
        let mut config = mxr_config::MxrConfig::default();
        config.general.attachment_dir = test_attachment_dir();
        let store = Arc::new(Store::in_memory().await?);
        let runtime_tasks = RuntimeTasks::default();
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(SemanticEngine::new(
            store.clone(),
            &std::env::temp_dir(),
            config.search.semantic.clone(),
        ));
        runtime_tasks.set_semantic_worker(semantic_worker);
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

        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let provider: Arc<dyn MailSyncProvider> = fake.clone();
        let send_provider: Option<Arc<dyn MailSendProvider>> = Some(fake.clone());

        let mut providers = HashMap::new();
        let mut send_providers = HashMap::new();
        providers.insert(account_id.clone(), provider.clone());
        send_providers.insert(account_id, fake.clone() as Arc<dyn MailSendProvider>);

        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        Ok((
            Self {
                store,
                search,
                semantic,
                sync_engine,
                runtime: RwLock::new(ProviderRuntime {
                    providers,
                    send_providers,
                    default_provider: Some(provider),
                    default_send_provider: send_provider,
                }),
                sync_loop_accounts: ParkingMutex::new(HashSet::new()),
                event_tx,
                start_time: Instant::now(),
                config: RwLock::new(config),
                shutdown_tx,
                runtime_tasks,
                admin_blocking,
            },
            fake,
        ))
    }

    pub fn socket_path() -> std::path::PathBuf {
        mxr_config::socket_path()
    }

    #[cfg(test)]
    pub fn set_attachment_dir_for_tests(&self, path: std::path::PathBuf) {
        self.config.write().general.attachment_dir = path;
    }
}

#[cfg(test)]
fn test_attachment_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mxr-attachments-test-{}", uuid::Uuid::new_v4()))
}

async fn open_search_index(
    index_path: &std::path::Path,
    store: &Store,
) -> anyhow::Result<SearchIndex> {
    match SearchIndex::open_with_rebuild_status(index_path) {
        Ok((index, rebuilt)) => {
            if rebuilt {
                store
                    .insert_event(
                        "warn",
                        "search",
                        "Lexical index rebuilt",
                        None,
                        Some("reason=schema_mismatch"),
                    )
                    .await?;
            }
            Ok(index)
        }
        Err(error) if search_error_requires_repair(&error.to_string()) => {
            tracing::warn!("Search index open failed, rebuilding from SQLite: {error}");
            if index_path.exists() {
                std::fs::remove_dir_all(index_path)?;
            }
            std::fs::create_dir_all(index_path)?;
            let (index, _) = SearchIndex::open_with_rebuild_status(index_path)?;
            let details = format!("reason=startup_repair error={error}");
            store
                .insert_event(
                    "warn",
                    "search",
                    "Lexical index rebuilt",
                    None,
                    Some(&details),
                )
                .await?;
            Ok(index)
        }
        Err(error) => Err(error.into()),
    }
}

fn search_error_requires_repair(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    !(lower.contains("lockbusy")
        || lower.contains("lockfile")
        || lower.contains("failed to acquire index lock")
        || lower.contains("failed to acquire lockfile")
        || lower.contains("already an `indexwriter` working")
        || lower.contains("already an indexwriter working"))
}

fn build_outlook_sync_provider(
    client_id: &Option<String>,
    token_ref: &str,
    tenant: mxr_provider_outlook::OutlookTenant,
    account_id: &mxr_core::AccountId,
    email: &str,
    key: &str,
) -> anyhow::Result<Option<Arc<dyn MailSyncProvider>>> {
    let cid = client_id
        .clone()
        .or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Outlook account '{key}' has no client_id and no bundled OUTLOOK_CLIENT_ID was compiled in"
            )
        })?;
    let auth = std::sync::Arc::new(mxr_provider_outlook::OutlookAuth::new(
        cid,
        token_ref.to_string(),
        tenant,
    ));
    let token_fn: std::sync::Arc<
        dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>> + Send + Sync,
    > = std::sync::Arc::new(move || {
        let auth = auth.clone();
        Box::pin(async move {
            auth.get_valid_access_token()
                .await
                .map_err(|e| anyhow::anyhow!(e))
        })
    });
    let factory = mxr_provider_imap::XOAuth2ImapSessionFactory::new(
        "outlook.office365.com".to_string(),
        993,
        email.to_string(),
        token_fn,
    );
    Ok(Some(Arc::new(
        mxr_provider_imap::ImapProvider::with_session_factory(
            account_id.clone(),
            mxr_provider_imap::config::ImapConfig::new(
                "outlook.office365.com".to_string(),
                993,
                email.to_string(),
                String::new(),
                true,
                true,
            ),
            Box::new(factory),
        ),
    ) as Arc<dyn MailSyncProvider>))
}

fn sync_provider_kind(sync: Option<&mxr_config::SyncProviderConfig>) -> Option<ProviderKind> {
    match sync {
        Some(mxr_config::SyncProviderConfig::Gmail { .. }) => Some(ProviderKind::Gmail),
        Some(mxr_config::SyncProviderConfig::Imap { .. }) => Some(ProviderKind::Imap),
        Some(mxr_config::SyncProviderConfig::OutlookPersonal { .. }) => {
            Some(ProviderKind::OutlookPersonal)
        }
        Some(mxr_config::SyncProviderConfig::OutlookWork { .. }) => {
            Some(ProviderKind::OutlookWork)
        }
        None => None,
    }
}

fn send_provider_kind(send: Option<&mxr_config::SendProviderConfig>) -> Option<ProviderKind> {
    match send {
        Some(mxr_config::SendProviderConfig::Gmail) => Some(ProviderKind::Gmail),
        Some(mxr_config::SendProviderConfig::Smtp { .. }) => Some(ProviderKind::Smtp),
        Some(mxr_config::SendProviderConfig::OutlookPersonal { .. }) => {
            Some(ProviderKind::OutlookPersonal)
        }
        Some(mxr_config::SendProviderConfig::OutlookWork { .. }) => {
            Some(ProviderKind::OutlookWork)
        }
        None => None,
    }
}

fn provider_kind_name(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Gmail => "gmail",
        ProviderKind::Imap => "imap",
        ProviderKind::Smtp => "smtp",
        ProviderKind::OutlookPersonal => "outlook",
        ProviderKind::OutlookWork => "outlook-work",
        ProviderKind::Fake => "fake",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imap_smtp_config(default_account: &str) -> mxr_config::MxrConfig {
        mxr_config::load_config_from_str(&format!(
            r#"
[general]
default_account = "{default_account}"

[accounts.personal]
name = "Personal"
email = "me@example.com"

[accounts.personal.sync]
type = "imap"
host = "imap.example.com"
port = 993
username = "me@example.com"
password_ref = "keyring:test-imap"
use_tls = true

[accounts.personal.send]
type = "smtp"
host = "smtp.example.com"
port = 587
username = "me@example.com"
password_ref = "keyring:test-smtp"
use_tls = true

[accounts.work]
name = "Work"
email = "me@corp.com"

[accounts.work.sync]
type = "imap"
host = "imap.corp.com"
port = 993
username = "me@corp.com"
password_ref = "keyring:test-work-imap"
use_tls = true
"#
        ))
        .expect("parse config")
    }

    #[tokio::test]
    async fn create_providers_from_config_supports_imap_and_smtp() {
        let store = Arc::new(Store::in_memory().await.expect("store"));
        let config = imap_smtp_config("personal");

        let setup = AppState::create_providers_from_config(&config, &store)
            .await
            .expect("provider setup");

        assert_eq!(setup.providers.len(), 2);
        assert_eq!(setup.send_providers.len(), 1);
        assert_eq!(
            setup
                .default_provider
                .as_ref()
                .expect("default provider")
                .account_id()
                .as_str()
                .len(),
            36
        );
        assert_eq!(
            setup
                .default_send_provider
                .as_ref()
                .expect("default send provider")
                .name(),
            "smtp"
        );

        let accounts = store.list_accounts().await.expect("list accounts");
        assert_eq!(accounts.len(), 2);
        assert!(accounts.iter().any(|account| {
            account
                .sync_backend
                .as_ref()
                .map(|backend| &backend.provider_kind)
                == Some(&ProviderKind::Imap)
        }));
    }

    #[tokio::test]
    async fn create_providers_from_config_uses_default_account() {
        let store = Arc::new(Store::in_memory().await.expect("store"));
        let config = imap_smtp_config("work");

        let setup = AppState::create_providers_from_config(&config, &store)
            .await
            .expect("provider setup");

        let default_account = store
            .get_account(
                setup
                    .default_provider
                    .as_ref()
                    .expect("default provider")
                    .account_id(),
            )
            .await
            .expect("account fetch")
            .expect("stored account");
        assert_eq!(default_account.name, "Work");
    }

    #[tokio::test]
    async fn create_providers_from_config_allows_empty_config() {
        let store = Arc::new(Store::in_memory().await.expect("store"));
        let config = mxr_config::MxrConfig::default();

        let setup = AppState::create_providers_from_config(&config, &store)
            .await
            .expect("provider setup");

        assert!(setup.providers.is_empty());
        assert!(setup.send_providers.is_empty());
        assert!(setup.default_provider.is_none());
        assert!(setup.default_send_provider.is_none());
    }
}
