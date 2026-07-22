use mxr_core::id::AccountId;
use mxr_core::*;
use mxr_protocol::{AccountConfigData, AuthSessionData, AuthSessionId, IpcMessage};
use mxr_relationship::RelationshipServiceHandle;
use mxr_search::{SearchIndex, SearchServiceHandle};
use mxr_semantic::{SemanticEngine, SemanticServiceHandle};
use mxr_store::{ContactsRefreshHandle, Store};
use mxr_sync::SyncEngine;
use parking_lot::{Mutex as ParkingMutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{
    broadcast, watch, Mutex as TokioMutex, Notify, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore,
};
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
    relationship_worker: ParkingMutex<Option<JoinHandle<()>>>,
    contacts_refresh_worker: ParkingMutex<Option<JoinHandle<()>>>,
    sync_loops: ParkingMutex<HashMap<AccountId, JoinHandle<()>>>,
    idle_loops: ParkingMutex<HashMap<AccountId, JoinHandle<()>>>,
    snooze_loop: ParkingMutex<Option<JoinHandle<()>>>,
    auto_reminders_loop: ParkingMutex<Option<JoinHandle<()>>>,
    scheduled_sends_loop: ParkingMutex<Option<JoinHandle<()>>>,
    reply_pair_reconciler: ParkingMutex<Option<JoinHandle<()>>>,
    contacts_refresher: ParkingMutex<Option<JoinHandle<()>>>,
    wrapped_warmer: ParkingMutex<Option<JoinHandle<()>>>,
    startup_maintenance: ParkingMutex<Option<JoinHandle<()>>>,
    bridge_loop: ParkingMutex<Option<JoinHandle<()>>>,
}

impl RuntimeTasks {
    fn set_search_worker(&self, handle: JoinHandle<()>) {
        *self.search_worker.lock() = Some(handle);
    }

    fn set_semantic_worker(&self, handle: JoinHandle<()>) {
        *self.semantic_worker.lock() = Some(handle);
    }

    fn set_relationship_worker(&self, handle: JoinHandle<()>) {
        *self.relationship_worker.lock() = Some(handle);
    }

    fn set_contacts_refresh_worker(&self, handle: JoinHandle<()>) {
        *self.contacts_refresh_worker.lock() = Some(handle);
    }

    fn register_sync_loop(&self, account_id: AccountId, handle: JoinHandle<()>) {
        self.sync_loops.lock().insert(account_id, handle);
    }

    fn register_idle_loop(&self, account_id: AccountId, handle: JoinHandle<()>) {
        self.idle_loops.lock().insert(account_id, handle);
    }

    fn finish_sync_loop(&self, account_id: &AccountId) {
        self.sync_loops.lock().remove(account_id);
    }

    fn finish_idle_loop(&self, account_id: &AccountId) {
        self.idle_loops.lock().remove(account_id);
    }

    fn set_snooze_loop(&self, handle: JoinHandle<()>) {
        *self.snooze_loop.lock() = Some(handle);
    }

    fn set_auto_reminders_loop(&self, handle: JoinHandle<()>) {
        *self.auto_reminders_loop.lock() = Some(handle);
    }

    fn set_scheduled_sends_loop(&self, handle: JoinHandle<()>) {
        *self.scheduled_sends_loop.lock() = Some(handle);
    }

    fn set_reply_pair_reconciler(&self, handle: JoinHandle<()>) {
        *self.reply_pair_reconciler.lock() = Some(handle);
    }

    fn set_contacts_refresher(&self, handle: JoinHandle<()>) {
        *self.contacts_refresher.lock() = Some(handle);
    }

    fn set_wrapped_warmer(&self, handle: JoinHandle<()>) {
        *self.wrapped_warmer.lock() = Some(handle);
    }

    fn set_startup_maintenance(&self, handle: JoinHandle<()>) {
        *self.startup_maintenance.lock() = Some(handle);
    }

    fn set_bridge_loop(&self, handle: JoinHandle<()>) {
        *self.bridge_loop.lock() = Some(handle);
    }

    /// Take a registered task handle out of its slot, holding the lock only for
    /// the `take()` itself so the guard never spans the caller's control flow.
    fn take_named(
        slot: &ParkingMutex<Option<JoinHandle<()>>>,
        name: &str,
    ) -> Option<NamedTaskHandle> {
        slot.lock().take().map(|handle| NamedTaskHandle {
            name: name.to_string(),
            handle,
        })
    }

    fn take_all(&self) -> Vec<NamedTaskHandle> {
        let mut handles = Vec::new();

        handles.extend(Self::take_named(&self.search_worker, "search_worker"));
        handles.extend(Self::take_named(&self.semantic_worker, "semantic_worker"));
        handles.extend(Self::take_named(
            &self.relationship_worker,
            "relationship_worker",
        ));
        handles.extend(Self::take_named(
            &self.contacts_refresh_worker,
            "contacts_refresh_worker",
        ));
        handles.extend(Self::take_named(&self.snooze_loop, "snooze_loop"));
        handles.extend(Self::take_named(
            &self.auto_reminders_loop,
            "auto_reminders_loop",
        ));
        handles.extend(Self::take_named(
            &self.scheduled_sends_loop,
            "scheduled_sends_loop",
        ));
        handles.extend(Self::take_named(
            &self.reply_pair_reconciler,
            "reply_pair_reconciler",
        ));
        handles.extend(Self::take_named(
            &self.contacts_refresher,
            "contacts_refresher",
        ));
        handles.extend(Self::take_named(&self.wrapped_warmer, "wrapped_warmer"));
        handles.extend(Self::take_named(
            &self.startup_maintenance,
            "startup_maintenance",
        ));
        handles.extend(Self::take_named(&self.bridge_loop, "bridge_loop"));

        for (account_id, handle) in self.sync_loops.lock().drain() {
            handles.push(NamedTaskHandle {
                name: format!("sync_loop:{account_id}"),
                handle,
            });
        }
        for (account_id, handle) in self.idle_loops.lock().drain() {
            handles.push(NamedTaskHandle {
                name: format!("idle_loop:{account_id}"),
                handle,
            });
        }

        handles
    }
}

/// Build the configured LLM provider. Returns a `NoopProvider` when
/// LLM is disabled in config, or an `OpenAiCompatibleProvider`
/// pointed at the configured `base_url` (Ollama / LM Studio / OpenAI
/// / etc.) when enabled. The API key is read from `api_key_env` —
/// keeping the secret out of the config file itself.
///
/// When the daemon is bound to the demo instance, every feature is routed
/// through `DemoLlmProvider` instead. That short-circuits all outbound LLM
/// traffic so a demo runs fully offline and can never spend the user's
/// real-account API key by accident, even if `[llm]` is configured.
fn build_llm_provider(config: &mxr_config::EffectiveLlmConfig) -> Arc<dyn mxr_llm::LlmProvider> {
    if mxr_config::is_demo_instance() {
        return Arc::new(mxr_llm::DemoLlmProvider::new());
    }
    if !config.enabled {
        return Arc::new(mxr_llm::NoopProvider);
    }
    let api_key = if config.api_key_env.is_empty() {
        None
    } else {
        std::env::var(&config.api_key_env)
            .ok()
            .filter(|s| !s.is_empty())
    };
    Arc::new(mxr_llm::OpenAiCompatibleProvider::new(
        mxr_llm::OpenAiCompatibleConfig {
            base_url: config.base_url.clone(),
            api_key,
            model: config.model.clone(),
            context_window: config.context_window,
            request_timeout: std::time::Duration::from_secs(config.request_timeout_secs),
        },
    ))
}

fn base_llm_config(config: &mxr_config::LlmConfig) -> mxr_config::EffectiveLlmConfig {
    mxr_config::EffectiveLlmConfig {
        enabled: config.enabled,
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        api_key_env: config.api_key_env.clone(),
        context_window: config.context_window,
        request_timeout_secs: config.request_timeout_secs,
    }
}

fn build_llm_runtime(config: &mxr_config::LlmConfig) -> Arc<mxr_llm::LlmRuntime> {
    let runtime = Arc::new(mxr_llm::LlmRuntime::new(build_llm_provider(
        &base_llm_config(config),
    )));
    apply_llm_config_to_runtime(&runtime, config);
    runtime
}

fn apply_llm_config_to_runtime(runtime: &Arc<mxr_llm::LlmRuntime>, config: &mxr_config::LlmConfig) {
    runtime.replace(build_llm_provider(&base_llm_config(config)));
    runtime.set_background_timeout(std::time::Duration::from_secs(
        config.background_request_timeout_secs,
    ));
    let mut providers = HashMap::<mxr_llm::LlmFeature, Arc<dyn mxr_llm::LlmProvider>>::new();
    let mut blocked = HashMap::<mxr_llm::LlmFeature, String>::new();
    for (feature, override_config) in llm_override_entries(&config.overrides) {
        let effective = config.effective_override(override_config);
        if let Some(reason) = relationship_data_block_reason(feature, config, &effective) {
            blocked.insert(feature, reason);
            continue;
        }
        providers.insert(feature, build_llm_provider(&effective));
    }
    for feature in relationship_data_features() {
        if providers.contains_key(&feature) || blocked.contains_key(&feature) {
            continue;
        }
        let effective = base_llm_config(config);
        if let Some(reason) = relationship_data_block_reason(feature, config, &effective) {
            blocked.insert(feature, reason);
        }
    }
    runtime.replace_feature_providers(providers, blocked);
}

fn llm_override_entries(
    overrides: &mxr_config::LlmOverrides,
) -> Vec<(mxr_llm::LlmFeature, &mxr_config::LlmOverrideConfig)> {
    use mxr_llm::LlmFeature;
    [
        (LlmFeature::Summarize, overrides.summarize.as_ref()),
        (
            LlmFeature::RelationshipSummary,
            overrides.relationship_summary.as_ref(),
        ),
        (LlmFeature::Commitments, overrides.commitments.as_ref()),
        (LlmFeature::DraftAssist, overrides.draft_assist.as_ref()),
        (LlmFeature::DraftNew, overrides.draft_new.as_ref()),
        (LlmFeature::DraftRefine, overrides.draft_refine.as_ref()),
        (LlmFeature::VoiceMatch, overrides.voice_match.as_ref()),
        (
            LlmFeature::HumanizeRewrite,
            overrides.humanize_rewrite.as_ref(),
        ),
        (
            LlmFeature::AnswerCoverage,
            overrides.answer_coverage.as_ref(),
        ),
        (LlmFeature::ArchiveAsk, overrides.archive_ask.as_ref()),
        (LlmFeature::DecisionLog, overrides.decision_log.as_ref()),
        (LlmFeature::Briefing, overrides.briefing.as_ref()),
        (LlmFeature::Expert, overrides.expert.as_ref()),
        (
            LlmFeature::DeliveryExtraction,
            overrides.delivery_extraction.as_ref(),
        ),
    ]
    .into_iter()
    .filter_map(|(feature, config)| config.map(|config| (feature, config)))
    .collect()
}

fn relationship_data_feature(feature: mxr_llm::LlmFeature) -> bool {
    matches!(
        feature,
        mxr_llm::LlmFeature::RelationshipSummary
            | mxr_llm::LlmFeature::Commitments
            | mxr_llm::LlmFeature::VoiceMatch
            | mxr_llm::LlmFeature::AnswerCoverage
            | mxr_llm::LlmFeature::ArchiveAsk
            | mxr_llm::LlmFeature::DecisionLog
            | mxr_llm::LlmFeature::Briefing
            | mxr_llm::LlmFeature::Expert
    )
}

fn relationship_data_features() -> [mxr_llm::LlmFeature; 8] {
    [
        mxr_llm::LlmFeature::RelationshipSummary,
        mxr_llm::LlmFeature::Commitments,
        mxr_llm::LlmFeature::VoiceMatch,
        mxr_llm::LlmFeature::AnswerCoverage,
        mxr_llm::LlmFeature::ArchiveAsk,
        mxr_llm::LlmFeature::DecisionLog,
        mxr_llm::LlmFeature::Briefing,
        mxr_llm::LlmFeature::Expert,
    ]
}

fn relationship_data_block_reason(
    feature: mxr_llm::LlmFeature,
    config: &mxr_config::LlmConfig,
    effective: &mxr_config::EffectiveLlmConfig,
) -> Option<String> {
    if relationship_data_feature(feature)
        && effective.enabled
        && !config.allow_cloud_relationship_data
        && !is_local_llm_url(&effective.base_url)
    {
        return Some(format!(
            "{feature:?} points at non-local endpoint {}; set llm.allow_cloud_relationship_data=true to permit relationship data",
            effective.base_url
        ));
    }
    None
}

fn is_local_llm_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://::1")
        || lower.starts_with("https://localhost")
        || lower.starts_with("https://127.")
        || lower.starts_with("https://[::1]")
        || lower.starts_with("https://::1")
}

/// Key for the in-memory `Wrapped` summary cache. Disambiguates by
/// account scope (`None` = "all accounts") and the human label.
///
/// `since_unix`/`until_unix` are intentionally NOT in the key: for
/// live windows ("year-to-date", "last 30 days") `until_unix` shifts
/// with `now()` on every request, which used to make the cache miss
/// 100% of the time. The label encodes the window semantics
/// uniquely ("2024" vs "last 30 days" vs "2026 year-to-date") so it
/// is the right primary key. The TTL — and the background warmer —
/// keep entries fresh enough.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct WrappedCacheKey {
    pub account_id: Option<AccountId>,
    pub label: String,
}

/// TTL for the daemon-side `Wrapped` summary cache. The background
/// `wrapped_warmer_loop` re-primes the default-window entry on a
/// shorter cadence than this so opening the Wrapped tab is normally
/// instant. Bound at 30 minutes so a cache hit on a non-warmed
/// window (e.g. an arbitrary `--year`) still reflects roughly
/// current data.
pub(crate) const WRAPPED_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

pub struct AppState {
    pub store: Arc<Store>,
    /// Static locale resolved once at daemon startup from `MXR_LOCALE` env
    /// var or config `locale` key. Handlers reference this for subject and
    /// body construction in iCal REPLY emails. Default `&EN`.
    pub locale: &'static mxr_core::i18n::Locale,
    /// User-activity recorder. Single seam between the IPC dispatcher and
    /// the `user_activity` table. Failures here are observability-only.
    pub activity: crate::activity::Recorder,
    pub search: SearchServiceHandle,
    pub semantic: SemanticServiceHandle,
    pub relationship: RelationshipServiceHandle,
    pub contacts_refresh: ContactsRefreshHandle,
    /// LLM provider for thread summarisation and draft assist. Always
    /// present; defaults to `NoopProvider` when LLM is disabled in
    /// config so callers can return `LlmDisabled` without `Option`
    /// gymnastics.
    pub llm: Arc<mxr_llm::LlmRuntime>,
    pub sync_engine: Arc<SyncEngine>,
    /// Account-owned address cache. SyncEngine consults this for direction
    /// classification; handlers refresh it after every mutation through
    /// `account_addresses`. See Slice 8 in the analytics plan.
    pub account_addresses: Arc<mxr_core::types::InMemoryAccountAddressLookup>,
    runtime: RwLock<ProviderRuntime>,
    provider_operation_locks: ParkingMutex<HashMap<AccountId, Arc<TokioMutex<()>>>>,
    sync_loop_accounts: ParkingMutex<HashSet<AccountId>>,
    /// Phase 3.1: tracks which accounts already have an IDLE watcher
    /// loop spawned. Mirrors `sync_loop_accounts` so a config reload
    /// doesn't double-spawn watchers.
    idle_loop_accounts: ParkingMutex<HashSet<AccountId>>,
    /// Phase 3.1: per-account `Notify` the IDLE watcher signals when
    /// the server pushes EXISTS / EXPUNGE / equivalent. The sync loop
    /// races this against its periodic timer so a notification wakes
    /// the next sync within a tick instead of waiting for the poll
    /// interval.
    idle_notifies: ParkingMutex<HashMap<AccountId, Arc<Notify>>>,
    pub event_tx: broadcast::Sender<IpcMessage>,
    pub start_time: Instant,
    /// Memoized reply quoted-text per message. Bodies are immutable
    /// post-sync, so this never needs invalidation. Lets `PrepareReply`
    /// skip rendering on the second hit for the same message — drives
    /// the "blazing fast `r`/`a`" UX in the TUI. Returned values are
    /// shared via `Arc` so the cache lookup is O(1) and lock-free
    /// after the first render.
    pub reply_context_cache: ParkingMutex<HashMap<MessageId, Arc<String>>>,
    /// 60s in-memory cache for `Wrapped` summaries. See
    /// `WrappedCacheKey` and `WRAPPED_CACHE_TTL` above.
    wrapped_cache: ParkingMutex<HashMap<WrappedCacheKey, (Instant, Arc<types::WrappedSummary>)>>,
    /// Set on the first successful sync after daemon start when the
    /// one-shot heavy analytics repair (reply_pairs backfill from
    /// existing messages) has run. Subsequent syncs only run the
    /// cheap incremental steps. Reset by daemon restart, which is
    /// the right cadence for "post-upgrade rescan" — a release that
    /// changes derived columns will start a fresh daemon process.
    pub analytics_startup_repair_done: std::sync::atomic::AtomicBool,
    config: RwLock<mxr_config::MxrConfig>,
    shutdown_tx: watch::Sender<bool>,
    runtime_tasks: RuntimeTasks,
    admin_blocking: Arc<Semaphore>,
    pub(crate) auth_sessions: ParkingMutex<HashMap<AuthSessionId, AuthSessionRuntime>>,
}

/// Background-worker DB concurrency cap. Held below the reader pool's
/// `max_connections` (see `crates/store/src/pool.rs`) so at least
/// `reader_max - BACKGROUND_DB_PERMITS` connections always remain free
/// for interactive/status traffic even when every background worker is
/// busy. The semaphore is a construction-local shared across the three
/// background workers (semantic ingest, relationship analytics,
/// contacts refresh); each worker holds an `Arc` clone for its lifetime.
const BACKGROUND_DB_PERMITS: usize = 2;

pub(crate) struct AuthSessionRuntime {
    pub account: AccountConfigData,
    pub status: Arc<ParkingMutex<AuthSessionData>>,
    pub handle: JoinHandle<()>,
}

/// Resolve the active locale at startup. Honors `MXR_LOCALE` first, falls
/// back to the config `general.locale` key, then to `"en"`. Trimmed +
/// lowercased so `EN`/` en `/`en` all resolve identically.
fn resolve_locale_code(config: &mxr_config::MxrConfig) -> String {
    std::env::var("MXR_LOCALE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| config.general.locale.clone())
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
        let activity = crate::activity::Recorder::spawn(store.clone());
        let runtime_tasks = RuntimeTasks::default();
        let background_db = Arc::new(Semaphore::new(BACKGROUND_DB_PERMITS));
        let (search, search_worker) =
            SearchServiceHandle::start(open_search_index(&index_path, &store).await?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(
            SemanticEngine::new(store.clone(), &data_dir, config.search.semantic.clone()),
            background_db.clone(),
        );
        runtime_tasks.set_semantic_worker(semantic_worker);
        let llm = build_llm_runtime(&config.llm);
        let (relationship, relationship_worker) =
            RelationshipServiceHandle::start(store.clone(), llm.clone(), background_db.clone());
        runtime_tasks.set_relationship_worker(relationship_worker);
        let (contacts_refresh, contacts_refresh_worker) =
            ContactsRefreshHandle::start(store.clone(), background_db.clone());
        runtime_tasks.set_contacts_refresh_worker(contacts_refresh_worker);
        let account_addresses = Arc::new(mxr_core::types::InMemoryAccountAddressLookup::new());
        // Best-effort initial load. Empty result is fine — `is_loaded` stays
        // false and direction classification falls back to Unknown until the
        // first refresh after an account is configured.
        match store.list_all_account_addresses().await {
            Ok(addresses) if !addresses.is_empty() => {
                account_addresses.replace(addresses.into_iter().map(|a| (a.account_id, a.email)));
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(error = %err, "failed initial account_addresses load");
            }
        }
        let sync_engine = Arc::new(SyncEngine::with_address_lookup(
            store.clone(),
            search.clone(),
            account_addresses.clone() as Arc<dyn mxr_core::types::AccountAddressLookup>,
        ));

        let provider_setup = Self::create_providers_from_config(&config, &store).await?;

        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        let locale = mxr_core::i18n::select(&resolve_locale_code(&config));

        Ok(Self {
            store,
            locale,
            activity,
            search,
            semantic,
            relationship,
            contacts_refresh,
            llm,
            sync_engine,
            account_addresses,
            runtime: RwLock::new(ProviderRuntime {
                providers: provider_setup.providers,
                send_providers: provider_setup.send_providers,
                default_provider: provider_setup.default_provider,
                default_send_provider: provider_setup.default_send_provider,
            }),
            provider_operation_locks: ParkingMutex::new(HashMap::new()),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_notifies: ParkingMutex::new(HashMap::new()),
            event_tx,
            start_time: Instant::now(),
            wrapped_cache: ParkingMutex::new(HashMap::new()),
            reply_context_cache: ParkingMutex::new(HashMap::new()),
            analytics_startup_repair_done: std::sync::atomic::AtomicBool::new(false),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
            auth_sessions: ParkingMutex::new(HashMap::new()),
        })
    }

    /// Reload the account-address cache from the store. Called after every
    /// successful mutation through `account_addresses` so direction inference
    /// stays current. Errors are logged but not surfaced — the next sync will
    /// see at worst a slightly stale cache.
    pub async fn refresh_account_addresses(&self) {
        match self.store.list_all_account_addresses().await {
            Ok(addresses) => {
                self.account_addresses
                    .replace(addresses.into_iter().map(|a| (a.account_id, a.email)));
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to refresh account_addresses cache");
            }
        }
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
                    .or_else(|| send_kind.clone())
                    .map_or("account", provider_kind_name),
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
                enabled: acct_config.enabled,
            };
            store.insert_account(&account).await?;

            if !acct_config.enabled {
                continue;
            }

            let sync_provider = match &acct_config.sync {
                Some(mxr_config::SyncProviderConfig::Gmail {
                    credential_source,
                    client_id,
                    client_secret,
                    token_ref,
                }) => {
                    let Some((cid, csecret)) = resolve_gmail_runtime_credentials(
                        *credential_source,
                        client_id,
                        client_secret.as_deref(),
                    ) else {
                        tracing::warn!(
                            account = %key,
                            "Gmail auth not ready, skipping provider: bundled OAuth credentials are unavailable"
                        );
                        continue;
                    };
                    let mut auth =
                        crate::provider_credentials::gmail_auth(cid, csecret, token_ref.clone());
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
                }) => {
                    // Degrade, never crash: a bad IMAP config skips just this
                    // account (mirroring Gmail above). Credentials resolve
                    // lazily at sync time, so a missing/unreadable secret does
                    // not reach here — it surfaces as account-unhealthy later.
                    match crate::provider_credentials::imap_config_with_credentials(
                        host.clone(),
                        *port,
                        username.clone(),
                        password_ref.clone(),
                        *auth_required,
                        *use_tls,
                    ) {
                        Ok(config) => Some(Arc::new(mxr_provider_imap::ImapProvider::new(
                            account_id.clone(),
                            config,
                        )) as Arc<dyn MailSyncProvider>),
                        Err(error) => {
                            tracing::warn!(
                                account = %key,
                                "IMAP provider config invalid, skipping account: {error}"
                            );
                            None
                        }
                    }
                }
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
                Some(mxr_config::SyncProviderConfig::Fake) => {
                    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
                    if matches!(acct_config.send, Some(mxr_config::SendProviderConfig::Fake)) {
                        let send_provider: Arc<dyn MailSendProvider> = fake.clone();
                        send_providers.insert(account_id.clone(), send_provider.clone());
                        if requested_default == Some(key.as_str())
                            || default_send_provider.is_none()
                        {
                            default_send_provider = Some(send_provider);
                        }
                    }
                    Some(fake as Arc<dyn MailSyncProvider>)
                }
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
                // Degrade, never crash: an invalid SMTP config skips only this
                // account's send provider. The password resolves lazily at send
                // time, so a missing secret does not abort boot here.
                match crate::provider_credentials::smtp_config_with_credentials(
                    host.clone(),
                    *port,
                    username.clone(),
                    password_ref.clone(),
                    *auth_required,
                    *use_tls,
                ) {
                    Ok(config) => {
                        let send_provider =
                            Arc::new(mxr_provider_smtp::SmtpSendProvider::new(config))
                                as Arc<dyn MailSendProvider>;
                        if requested_default == Some(key.as_str())
                            || default_send_provider.is_none()
                        {
                            default_send_provider = Some(send_provider.clone());
                        }
                        send_providers.insert(account_id.clone(), send_provider);
                    }
                    Err(error) => {
                        tracing::warn!(
                            account = %key,
                            "SMTP send provider config invalid, skipping account send: {error}"
                        );
                    }
                }
            }

            if let Some(
                mxr_config::SendProviderConfig::OutlookPersonal { token_ref, .. }
                | mxr_config::SendProviderConfig::OutlookWork { token_ref, .. },
            ) = &acct_config.send
            {
                let tenant = match &acct_config.send {
                    Some(mxr_config::SendProviderConfig::OutlookWork { .. }) => {
                        mxr_provider_outlook::OutlookTenant::Work
                    }
                    _ => mxr_provider_outlook::OutlookTenant::Personal,
                };
                // Resolve client_id: sync config → send config → bundled constant.
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
                    ) => Some(id.clone()),
                    _ => None,
                }
                .or_else(|| match &acct_config.send {
                    Some(
                        mxr_config::SendProviderConfig::OutlookPersonal {
                            client_id: Some(id),
                            ..
                        }
                        | mxr_config::SendProviderConfig::OutlookWork {
                            client_id: Some(id),
                            ..
                        },
                    ) => Some(id.clone()),
                    _ => None,
                })
                .or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Outlook send for '{key}' has no client_id and no bundled OUTLOOK_CLIENT_ID"
                    )
                })?;
                let auth = std::sync::Arc::new(crate::provider_credentials::outlook_auth(
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
                    mxr_provider_outlook::OutlookTenant::Personal => "smtp-mail.outlook.com",
                    mxr_provider_outlook::OutlookTenant::Work => "smtp.office365.com",
                };
                let send_provider = Arc::new(mxr_provider_outlook::OutlookSmtpSendProvider::new(
                    smtp_host.to_string(),
                    587,
                    acct_config.email.clone(),
                    token_fn,
                )) as Arc<dyn MailSendProvider>;
                if requested_default == Some(key.as_str()) || default_send_provider.is_none() {
                    default_send_provider = Some(send_provider.clone());
                }
                send_providers.insert(account_id.clone(), send_provider);
            }

            if matches!(acct_config.send, Some(mxr_config::SendProviderConfig::Fake))
                && !send_providers.contains_key(&account_id)
            {
                let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()))
                    as Arc<dyn MailSendProvider>;
                if requested_default == Some(key.as_str()) || default_send_provider.is_none() {
                    default_send_provider = Some(fake.clone());
                }
                send_providers.insert(account_id.clone(), fake);
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

    /// Look up a cached `Wrapped` summary if one was computed within
    /// `WRAPPED_CACHE_TTL`. Returns `None` on miss or expiry; expired
    /// entries are evicted lazily on the next miss for that key.
    pub(crate) fn wrapped_cache_get(
        &self,
        key: &WrappedCacheKey,
    ) -> Option<Arc<types::WrappedSummary>> {
        let mut cache = self.wrapped_cache.lock();
        if let Some((stored_at, summary)) = cache.get(key) {
            if stored_at.elapsed() < WRAPPED_CACHE_TTL {
                return Some(summary.clone());
            }
            // Stale — drop while we hold the lock.
            cache.remove(key);
        }
        None
    }

    /// Insert a freshly-computed `Wrapped` summary into the cache.
    pub(crate) fn wrapped_cache_put(
        &self,
        key: WrappedCacheKey,
        summary: Arc<types::WrappedSummary>,
    ) {
        let mut cache = self.wrapped_cache.lock();
        cache.insert(key, (Instant::now(), summary));
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

    pub async fn acquire_provider_operation(&self, account_id: &AccountId) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.provider_operation_locks.lock();
            locks
                .entry(account_id.clone())
                .or_insert_with(|| Arc::new(TokioMutex::new(())))
                .clone()
        };
        lock.lock_owned().await
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

    pub fn register_idle_loop_handle(&self, account_id: AccountId, handle: JoinHandle<()>) {
        self.runtime_tasks.register_idle_loop(account_id, handle);
    }

    pub fn finish_sync_loop(&self, account_id: &AccountId) {
        self.sync_loop_accounts.lock().remove(account_id);
        self.runtime_tasks.finish_sync_loop(account_id);
    }

    /// Phase 3.1: returns the per-account `Notify` the IDLE watcher
    /// signals to wake the sync loop. Created on first request; the
    /// same handle is shared between watcher and sync loop.
    pub fn idle_notify_for_account(&self, account_id: &AccountId) -> Arc<Notify> {
        let mut guard = self.idle_notifies.lock();
        guard
            .entry(account_id.clone())
            .or_insert_with(|| Arc::new(Notify::new()))
            .clone()
    }

    /// Phase 3.1: try to claim the right to spawn an IDLE watcher loop
    /// for `account_id`. Returns true on first call per account so the
    /// caller knows it's safe to `tokio::spawn`. Subsequent calls
    /// return false and the caller skips. Mirrors
    /// `mark_sync_loop_spawned`.
    pub fn mark_idle_loop_spawned(&self, account_id: &AccountId) -> bool {
        self.idle_loop_accounts.lock().insert(account_id.clone())
    }

    pub fn finish_idle_loop(&self, account_id: &AccountId) {
        self.idle_loop_accounts.lock().remove(account_id);
        self.runtime_tasks.finish_idle_loop(account_id);
    }

    pub fn register_reply_pair_reconciler(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_reply_pair_reconciler(handle);
    }

    pub fn register_contacts_refresher(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_contacts_refresher(handle);
    }

    pub fn register_wrapped_warmer(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_wrapped_warmer(handle);
    }

    pub fn register_snooze_loop(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_snooze_loop(handle);
    }

    pub fn register_auto_reminders_loop(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_auto_reminders_loop(handle);
    }

    pub fn register_scheduled_sends_loop(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_scheduled_sends_loop(handle);
    }

    pub fn register_startup_maintenance(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_startup_maintenance(handle);
    }

    pub fn register_bridge_loop(&self, handle: JoinHandle<()>) {
        self.runtime_tasks.set_bridge_loop(handle);
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
                    tracing::warn!(task = %task.name, "runtime task join failed: {error}");
                }
                Err(_) => tracing::warn!(task = %task.name, "runtime task drain timed out"),
            }
        }

        for (session_id, session) in self.auth_sessions.lock().drain() {
            session.handle.abort();
            tracing::trace!(session_id = %session_id.0, "auth session task aborted");
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
        apply_llm_config_to_runtime(&self.llm, &config.llm);
        *self.config.write() = config.clone();
        Ok(config)
    }

    #[cfg(test)]
    pub async fn set_config_for_test(&self, config: mxr_config::MxrConfig) {
        self.semantic
            .apply_config(config.search.semantic.clone())
            .await
            .expect("apply semantic config");
        apply_llm_config_to_runtime(&self.llm, &config.llm);
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
        match account_id {
            Some(id) => runtime
                .providers
                .get(id)
                .cloned()
                .ok_or_else(|| format!("No sync provider configured for account {id}")),
            None => runtime
                .default_provider
                .clone()
                .ok_or_else(|| "no sync-capable accounts configured".to_string()),
        }
    }

    pub fn send_provider_for_account(
        &self,
        account_id: &AccountId,
    ) -> std::result::Result<Arc<dyn MailSendProvider>, String> {
        self.runtime
            .read()
            .send_providers
            .get(account_id)
            .cloned()
            .ok_or_else(|| format!("No send provider configured for account {account_id}"))
    }

    /// Get send provider for a specific account, or the default when no account is specified.
    pub fn get_send_provider(
        &self,
        account_id: Option<&AccountId>,
    ) -> Option<Arc<dyn MailSendProvider>> {
        let runtime = self.runtime.read();
        match account_id {
            Some(id) => runtime.send_providers.get(id).cloned(),
            None => runtime.default_send_provider.clone(),
        }
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

    #[cfg(test)]
    pub fn add_sync_provider_for_test(&self, provider: Arc<dyn MailSyncProvider>) {
        self.runtime
            .write()
            .providers
            .insert(provider.account_id().clone(), provider);
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
        apply_llm_config_to_runtime(&self.llm, &config.llm);
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
        let background_db = Arc::new(Semaphore::new(BACKGROUND_DB_PERMITS));
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(
            SemanticEngine::new(
                store.clone(),
                &std::env::temp_dir(),
                config.search.semantic.clone(),
            ),
            background_db.clone(),
        );
        runtime_tasks.set_semantic_worker(semantic_worker);
        let llm = build_llm_runtime(&config.llm);
        let (relationship, relationship_worker) =
            RelationshipServiceHandle::start(store.clone(), llm.clone(), background_db.clone());
        runtime_tasks.set_relationship_worker(relationship_worker);
        let (contacts_refresh, contacts_refresh_worker) =
            ContactsRefreshHandle::start(store.clone(), background_db.clone());
        runtime_tasks.set_contacts_refresh_worker(contacts_refresh_worker);
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

        let activity = crate::activity::Recorder::spawn(store.clone());
        Ok(Self {
            store,
            locale: mxr_core::i18n::DEFAULT_LOCALE,
            activity,
            search,
            semantic,
            relationship,
            contacts_refresh,
            llm,
            sync_engine,
            account_addresses: Arc::new(mxr_core::types::InMemoryAccountAddressLookup::new()),
            runtime: RwLock::new(ProviderRuntime {
                providers,
                send_providers,
                default_provider: Some(provider),
                default_send_provider: send_provider,
            }),
            provider_operation_locks: ParkingMutex::new(HashMap::new()),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_notifies: ParkingMutex::new(HashMap::new()),
            event_tx,
            start_time: Instant::now(),
            wrapped_cache: ParkingMutex::new(HashMap::new()),
            reply_context_cache: ParkingMutex::new(HashMap::new()),
            analytics_startup_repair_done: std::sync::atomic::AtomicBool::new(false),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
            auth_sessions: ParkingMutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    pub async fn in_memory_without_accounts() -> anyhow::Result<Self> {
        let mut config = mxr_config::MxrConfig::default();
        config.general.attachment_dir = test_attachment_dir();
        let store = Arc::new(Store::in_memory().await?);
        let runtime_tasks = RuntimeTasks::default();
        let background_db = Arc::new(Semaphore::new(BACKGROUND_DB_PERMITS));
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(
            SemanticEngine::new(
                store.clone(),
                &std::env::temp_dir(),
                config.search.semantic.clone(),
            ),
            background_db.clone(),
        );
        runtime_tasks.set_semantic_worker(semantic_worker);
        let llm = build_llm_runtime(&config.llm);
        let (relationship, relationship_worker) =
            RelationshipServiceHandle::start(store.clone(), llm.clone(), background_db.clone());
        runtime_tasks.set_relationship_worker(relationship_worker);
        let (contacts_refresh, contacts_refresh_worker) =
            ContactsRefreshHandle::start(store.clone(), background_db.clone());
        runtime_tasks.set_contacts_refresh_worker(contacts_refresh_worker);
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));
        let (event_tx, _) = broadcast::channel(256);
        let (shutdown_tx, _) = watch::channel(false);
        let admin_blocking = Arc::new(Semaphore::new(2));

        let activity = crate::activity::Recorder::spawn(store.clone());
        Ok(Self {
            store,
            locale: mxr_core::i18n::DEFAULT_LOCALE,
            activity,
            search,
            semantic,
            relationship,
            contacts_refresh,
            llm,
            sync_engine,
            account_addresses: Arc::new(mxr_core::types::InMemoryAccountAddressLookup::new()),
            runtime: RwLock::new(ProviderRuntime {
                providers: HashMap::new(),
                send_providers: HashMap::new(),
                default_provider: None,
                default_send_provider: None,
            }),
            provider_operation_locks: ParkingMutex::new(HashMap::new()),
            sync_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_loop_accounts: ParkingMutex::new(HashSet::new()),
            idle_notifies: ParkingMutex::new(HashMap::new()),
            event_tx,
            start_time: Instant::now(),
            wrapped_cache: ParkingMutex::new(HashMap::new()),
            reply_context_cache: ParkingMutex::new(HashMap::new()),
            analytics_startup_repair_done: std::sync::atomic::AtomicBool::new(false),
            config: RwLock::new(config),
            shutdown_tx,
            runtime_tasks,
            admin_blocking,
            auth_sessions: ParkingMutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    pub async fn in_memory_with_fake(
    ) -> anyhow::Result<(Self, Arc<mxr_provider_fake::FakeProvider>)> {
        let mut config = mxr_config::MxrConfig::default();
        config.general.attachment_dir = test_attachment_dir();
        let store = Arc::new(Store::in_memory().await?);
        let runtime_tasks = RuntimeTasks::default();
        let background_db = Arc::new(Semaphore::new(BACKGROUND_DB_PERMITS));
        let (search, search_worker) = SearchServiceHandle::start(SearchIndex::in_memory()?);
        runtime_tasks.set_search_worker(search_worker);
        let (semantic, semantic_worker) = SemanticServiceHandle::start(
            SemanticEngine::new(
                store.clone(),
                &std::env::temp_dir(),
                config.search.semantic.clone(),
            ),
            background_db.clone(),
        );
        runtime_tasks.set_semantic_worker(semantic_worker);
        let llm = build_llm_runtime(&config.llm);
        let (relationship, relationship_worker) =
            RelationshipServiceHandle::start(store.clone(), llm.clone(), background_db.clone());
        runtime_tasks.set_relationship_worker(relationship_worker);
        let (contacts_refresh, contacts_refresh_worker) =
            ContactsRefreshHandle::start(store.clone(), background_db.clone());
        runtime_tasks.set_contacts_refresh_worker(contacts_refresh_worker);
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

        let activity = crate::activity::Recorder::spawn(store.clone());
        Ok((
            Self {
                store,
                locale: mxr_core::i18n::DEFAULT_LOCALE,
                activity,
                search,
                semantic,
                relationship,
                contacts_refresh,
                llm,
                sync_engine,
                account_addresses: Arc::new(mxr_core::types::InMemoryAccountAddressLookup::new()),
                runtime: RwLock::new(ProviderRuntime {
                    providers,
                    send_providers,
                    default_provider: Some(provider),
                    default_send_provider: send_provider,
                }),
                provider_operation_locks: ParkingMutex::new(HashMap::new()),
                sync_loop_accounts: ParkingMutex::new(HashSet::new()),
                idle_loop_accounts: ParkingMutex::new(HashSet::new()),
                idle_notifies: ParkingMutex::new(HashMap::new()),
                event_tx,
                start_time: Instant::now(),
                wrapped_cache: ParkingMutex::new(HashMap::new()),
                reply_context_cache: ParkingMutex::new(HashMap::new()),
                analytics_startup_repair_done: std::sync::atomic::AtomicBool::new(false),
                config: RwLock::new(config),
                shutdown_tx,
                runtime_tasks,
                admin_blocking,
                auth_sessions: ParkingMutex::new(HashMap::new()),
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
    let auth = std::sync::Arc::new(crate::provider_credentials::outlook_auth(
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
    Ok(Some(
        Arc::new(mxr_provider_imap::ImapProvider::with_session_factory(
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
        )) as Arc<dyn MailSyncProvider>,
    ))
}

fn sync_provider_kind(sync: Option<&mxr_config::SyncProviderConfig>) -> Option<ProviderKind> {
    match sync {
        Some(mxr_config::SyncProviderConfig::Gmail { .. }) => Some(ProviderKind::Gmail),
        Some(mxr_config::SyncProviderConfig::Imap { .. }) => Some(ProviderKind::Imap),
        Some(mxr_config::SyncProviderConfig::OutlookPersonal { .. }) => {
            Some(ProviderKind::OutlookPersonal)
        }
        Some(mxr_config::SyncProviderConfig::OutlookWork { .. }) => Some(ProviderKind::OutlookWork),
        Some(mxr_config::SyncProviderConfig::Fake) => Some(ProviderKind::Fake),
        None => None,
    }
}

fn resolve_gmail_runtime_credentials(
    credential_source: mxr_config::GmailCredentialSource,
    client_id: &str,
    client_secret: Option<&str>,
) -> Option<(String, String)> {
    match credential_source {
        mxr_config::GmailCredentialSource::Bundled => match (
            mxr_provider_gmail::auth::BUNDLED_CLIENT_ID,
            mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET,
        ) {
            (Some(id), Some(secret)) => Some((id.to_string(), secret.to_string())),
            _ if !client_id.trim().is_empty()
                && !client_secret.unwrap_or_default().trim().is_empty() =>
            {
                Some((
                    client_id.to_string(),
                    client_secret.unwrap_or_default().to_string(),
                ))
            }
            _ => None,
        },
        mxr_config::GmailCredentialSource::Custom => {
            if client_id.trim().is_empty() || client_secret.unwrap_or_default().trim().is_empty() {
                None
            } else {
                Some((
                    client_id.to_string(),
                    client_secret.unwrap_or_default().to_string(),
                ))
            }
        }
    }
}

fn send_provider_kind(send: Option<&mxr_config::SendProviderConfig>) -> Option<ProviderKind> {
    match send {
        Some(mxr_config::SendProviderConfig::Gmail) => Some(ProviderKind::Gmail),
        Some(mxr_config::SendProviderConfig::Smtp { .. }) => Some(ProviderKind::Smtp),
        Some(mxr_config::SendProviderConfig::OutlookPersonal { .. }) => {
            Some(ProviderKind::OutlookPersonal)
        }
        Some(mxr_config::SendProviderConfig::OutlookWork { .. }) => Some(ProviderKind::OutlookWork),
        Some(mxr_config::SendProviderConfig::Fake) => Some(ProviderKind::Fake),
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
auth_required = false
use_tls = true

[accounts.personal.send]
type = "smtp"
host = "smtp.example.com"
port = 587
username = "me@example.com"
password_ref = "keyring:test-smtp"
auth_required = false
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
auth_required = false
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
    async fn create_providers_survives_unreadable_credential() {
        // Regression for the disk-first credential bug: a password-auth IMAP
        // account whose secret is absent/unreadable must NOT abort daemon
        // startup. Credentials resolve lazily at sync time, so the provider is
        // still constructed and the other account keeps working. Pre-fix, the
        // eager keychain read here returned `?` and bricked the whole daemon.
        let store = Arc::new(Store::in_memory().await.expect("store"));
        let config = mxr_config::load_config_from_str(
            r#"
[general]
default_account = "good"

[accounts.good]
name = "Good"
email = "good@example.com"

[accounts.good.sync]
type = "fake"

[accounts.broken]
name = "Broken"
email = "broken@corp.com"

[accounts.broken.sync]
type = "imap"
host = "imap.corp.com"
port = 993
username = "broken@corp.com"
password_ref = "keyring:definitely-absent-secret"
auth_required = true
use_tls = true
"#,
        )
        .expect("parse config");

        // Boot must succeed even though the broken account has no readable
        // secret on disk or in the keychain.
        let setup = AppState::create_providers_from_config(&config, &store)
            .await
            .expect("daemon boots despite an unreadable credential");

        // Both providers are constructed; the healthy account is fully usable.
        assert_eq!(
            setup.providers.len(),
            2,
            "both accounts get a sync provider"
        );
        assert!(
            setup.default_provider.is_some(),
            "the healthy account remains the default provider"
        );
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

    #[tokio::test]
    async fn create_providers_from_config_skips_disabled_accounts() {
        let store = Arc::new(Store::in_memory().await.expect("store"));
        let mut config = imap_smtp_config("personal");
        config
            .accounts
            .get_mut("personal")
            .expect("personal account")
            .enabled = false;

        let setup = AppState::create_providers_from_config(&config, &store)
            .await
            .expect("provider setup");

        assert_eq!(setup.providers.len(), 1);
        let default_account = store
            .get_account(
                setup
                    .default_provider
                    .as_ref()
                    .expect("fallback default provider")
                    .account_id(),
            )
            .await
            .expect("account fetch")
            .expect("stored account");
        assert_eq!(default_account.name, "Work");

        let disabled_account_id = AccountId::from_provider_id("imap", "me@example.com");
        let disabled_account = store
            .get_account(&disabled_account_id)
            .await
            .expect("account fetch")
            .expect("disabled account row");
        assert!(!disabled_account.enabled);
        assert_eq!(store.list_accounts().await.expect("list accounts").len(), 1);
    }

    #[tokio::test]
    async fn explicit_send_provider_lookup_does_not_fallback_to_default() {
        let (state, _) = AppState::in_memory_with_fake()
            .await
            .expect("state with default send provider");
        let other_account_id = AccountId::new();

        assert!(state.get_send_provider(None).is_some());
        assert!(state.get_send_provider(Some(&other_account_id)).is_none());
        assert!(state.send_provider_for_account(&other_account_id).is_err());
    }

    #[tokio::test]
    async fn explicit_sync_provider_lookup_does_not_fallback_to_default() {
        let (state, _) = AppState::in_memory_with_fake()
            .await
            .expect("state with default sync provider");
        let other_account_id = AccountId::new();

        assert!(state.get_provider(None).is_ok());
        assert!(state.get_provider(Some(&other_account_id)).is_err());
        assert!(state.sync_provider_for_account(&other_account_id).is_none());
    }

    #[test]
    fn relationship_llm_features_block_nonlocal_base_without_privacy_opt_in() {
        let mut llm = mxr_config::LlmConfig {
            enabled: true,
            base_url: "https://api.openai.com/v1".to_string(),
            ..mxr_config::LlmConfig::default()
        };

        let runtime = build_llm_runtime(&llm);

        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::RelationshipSummary)
            .is_some());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::Commitments)
            .is_some());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::VoiceMatch)
            .is_some());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::Expert)
            .is_some());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::DraftAssist)
            .is_none());

        llm.allow_cloud_relationship_data = true;
        let runtime = build_llm_runtime(&llm);

        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::RelationshipSummary)
            .is_none());
    }

    #[test]
    fn local_relationship_override_is_not_blocked_by_nonlocal_base() {
        let llm = mxr_config::LlmConfig {
            enabled: true,
            base_url: "https://api.openai.com/v1".to_string(),
            overrides: mxr_config::LlmOverrides {
                relationship_summary: Some(mxr_config::LlmOverrideConfig {
                    base_url: Some("http://localhost:11434/v1".to_string()),
                    ..mxr_config::LlmOverrideConfig::default()
                }),
                expert: Some(mxr_config::LlmOverrideConfig {
                    base_url: Some("http://localhost:11434/v1".to_string()),
                    ..mxr_config::LlmOverrideConfig::default()
                }),
                ..mxr_config::LlmOverrides::default()
            },
            ..mxr_config::LlmConfig::default()
        };

        let runtime = build_llm_runtime(&llm);

        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::RelationshipSummary)
            .is_none());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::Commitments)
            .is_some());
        assert!(runtime
            .feature_block_reason(mxr_llm::LlmFeature::Expert)
            .is_none());
    }
}
