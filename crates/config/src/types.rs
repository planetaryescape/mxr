use mxr_core::{SearchMode as CoreSearchMode, SemanticProfile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GmailCredentialSource {
    #[default]
    Bundled,
    Custom,
}

/// Top-level mxr configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MxrConfig {
    pub general: GeneralConfig,
    pub accounts: HashMap<String, AccountConfig>,
    #[serde(alias = "agents", alias = "agent")]
    pub agent_surfaces: AgentSurfaceConfig,
    pub render: RenderConfig,
    pub search: SearchConfig,
    pub snooze: SnoozeConfig,
    pub logging: LoggingConfig,
    pub appearance: AppearanceConfig,
    pub bridge: BridgeConfig,
    pub llm: LlmConfig,
    pub humanizer: HumanizerConfig,
    pub activity: ActivityConfig,
    pub deliveries: DeliveriesConfig,
    pub notifications: NotificationConfig,
}

/// Package/delivery tracking. Detection is local-first; the optional LLM
/// enrichment is gated by the global LLM config plus its privacy policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeliveriesConfig {
    /// Scan new mail for deliveries during the post-sync fan-out. On by
    /// default; detection is fully local and cheap.
    pub enabled: bool,
}

impl Default for DeliveriesConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Local notification preferences. Audio is opt-in so daemon startup never
/// surprises users in shared or quiet environments.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    pub chimes: ChimeConfig,
}

/// Audio feedback for daemon-observed events and successful user actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChimeConfig {
    pub enabled: bool,
    pub volume: f32,
    pub new_mail: ChimeSound,
    pub sent: ChimeSound,
    pub archived: ChimeSound,
    pub trashed: ChimeSound,
    pub spam: ChimeSound,
    pub snoozed: ChimeSound,
    pub unsnoozed: ChimeSound,
    pub reminder: ChimeSound,
    pub error: ChimeSound,
}

impl Default for ChimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.35,
            new_mail: ChimeSound::Bell,
            sent: ChimeSound::Sent,
            archived: ChimeSound::Archive,
            trashed: ChimeSound::Thud,
            spam: ChimeSound::Alert,
            snoozed: ChimeSound::Pop,
            unsnoozed: ChimeSound::Glass,
            reminder: ChimeSound::Bell,
            error: ChimeSound::Alert,
        }
    }
}

impl ChimeConfig {
    pub fn sound_for(&self, event: ChimeEvent) -> ChimeSound {
        match event {
            ChimeEvent::NewMail => self.new_mail,
            ChimeEvent::Sent => self.sent,
            ChimeEvent::Archived => self.archived,
            ChimeEvent::Trashed => self.trashed,
            ChimeEvent::Spam => self.spam,
            ChimeEvent::Snoozed => self.snoozed,
            ChimeEvent::Unsnoozed => self.unsnoozed,
            ChimeEvent::Reminder => self.reminder,
            ChimeEvent::Error => self.error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChimeEvent {
    NewMail,
    Sent,
    Archived,
    Trashed,
    Spam,
    Snoozed,
    Unsnoozed,
    Reminder,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChimeSound {
    None,
    Bell,
    Glass,
    Pop,
    Sent,
    Archive,
    Thud,
    Alert,
}

/// Configuration for the user-activity log. Strictly local; see
/// `docs/activity-log.md`. Every field here is opt-out from
/// recording, never opt-in to transmission — the log never leaves the
/// device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityConfig {
    /// Global enable. When `false`, the recorder is spawned but every
    /// `record()` call is dropped silently. Equivalent to setting
    /// `MXR_ACTIVITY=off` at startup, but persists in config.
    pub enabled: bool,
    pub retention: ActivityRetentionConfig,
    /// Opt-in: record `link.click` actions with the URL in `context_json`.
    /// Default `false` because URL history reveals a lot.
    pub track_link_clicks: bool,
    /// Record subjects in `context_json`. Default `true`.
    pub track_subjects: bool,
    /// Record recipient handles in `context_json`. Default `true`.
    pub track_recipient_handles: bool,
    /// Record search query text verbatim in `context_json`. Default `true`.
    pub track_search_queries: bool,
}

impl Default for ActivityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention: ActivityRetentionConfig::default(),
            track_link_clicks: false,
            track_subjects: true,
            track_recipient_handles: true,
            track_search_queries: true,
        }
    }
}

/// Per-tier retention windows in days. Daily prune sweep hard-deletes rows
/// older than these.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct ActivityRetentionConfig {
    pub ephemeral_days: u32,
    pub standard_days: u32,
    pub important_days: u32,
}

impl Default for ActivityRetentionConfig {
    fn default() -> Self {
        Self {
            ephemeral_days: 30,
            standard_days: 90,
            important_days: 365,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HumanizerConfig {
    pub enabled: bool,
    pub score_threshold: u8,
    pub auto_fix: bool,
    pub max_rewrite_iterations: u8,
    pub apply_to_drafts: bool,
    pub apply_to_summaries: bool,
}

impl Default for HumanizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            score_threshold: 70,
            auto_fix: true,
            max_rewrite_iterations: 2,
            apply_to_drafts: true,
            apply_to_summaries: false,
        }
    }
}

/// LLM configuration. Disabled by default — opt-in for users who want
/// thread summarisation and draft assist. Local-first: the
/// recommended setup points at a local Ollama or LM Studio instance,
/// with cloud endpoints (OpenAI, Groq, OpenRouter) supported via the
/// same OpenAI-compatible config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub enabled: bool,
    /// OpenAI-compatible base URL. Defaults to local Ollama.
    pub base_url: String,
    /// Model name (e.g. `"qwen2.5:3b-instruct"` for Ollama,
    /// `"gpt-4o-mini"` for OpenAI, `"llama-3.1-8b-instant"` for Groq).
    pub model: String,
    /// Environment variable to read the API key from. Empty/missing =
    /// no auth header (correct for Ollama / LM Studio). Naming the env
    /// var rather than embedding the key keeps the secret out of the
    /// config file.
    pub api_key_env: String,
    /// Context window in tokens; used to bound prompt construction.
    pub context_window: u32,
    /// Per-request timeout in seconds. Local LLMs can be slow.
    pub request_timeout_secs: u64,
    /// Timeout in seconds for *background* LLM work (relationship
    /// summary, commitment extraction). Held tighter than
    /// `request_timeout_secs` so a slow/dead endpoint can't pin a
    /// background worker (and its reserved DB slot) for the full
    /// foreground budget. Foreground/user-initiated LLM keeps
    /// `request_timeout_secs`.
    pub background_request_timeout_secs: u64,
    /// Allow relationship/profile data to be sent to non-local LLM endpoints.
    pub allow_cloud_relationship_data: bool,
    /// Optional feature-specific provider overrides. Each override inherits
    /// unspecified fields from this base `[llm]` section.
    pub overrides: LlmOverrides,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "http://localhost:11434/v1".into(),
            model: "qwen2.5:3b-instruct".into(),
            api_key_env: String::new(),
            context_window: 8192,
            request_timeout_secs: 120,
            background_request_timeout_secs: 45,
            allow_cloud_relationship_data: false,
            overrides: LlmOverrides::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmOverrides {
    pub summarize: Option<LlmOverrideConfig>,
    pub relationship_summary: Option<LlmOverrideConfig>,
    pub commitments: Option<LlmOverrideConfig>,
    pub draft_assist: Option<LlmOverrideConfig>,
    pub draft_new: Option<LlmOverrideConfig>,
    pub draft_refine: Option<LlmOverrideConfig>,
    pub voice_match: Option<LlmOverrideConfig>,
    pub humanize_rewrite: Option<LlmOverrideConfig>,
    pub answer_coverage: Option<LlmOverrideConfig>,
    pub archive_ask: Option<LlmOverrideConfig>,
    pub decision_log: Option<LlmOverrideConfig>,
    pub briefing: Option<LlmOverrideConfig>,
    pub expert: Option<LlmOverrideConfig>,
    pub delivery_extraction: Option<LlmOverrideConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmOverrideConfig {
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub context_window: Option<u32>,
    pub request_timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveLlmConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub context_window: u32,
    pub request_timeout_secs: u64,
}

impl LlmConfig {
    pub fn effective_override(&self, override_config: &LlmOverrideConfig) -> EffectiveLlmConfig {
        EffectiveLlmConfig {
            enabled: override_config.enabled.unwrap_or(self.enabled),
            base_url: override_config
                .base_url
                .clone()
                .unwrap_or_else(|| self.base_url.clone()),
            model: override_config
                .model
                .clone()
                .unwrap_or_else(|| self.model.clone()),
            api_key_env: override_config
                .api_key_env
                .clone()
                .unwrap_or_else(|| self.api_key_env.clone()),
            context_window: override_config
                .context_window
                .unwrap_or(self.context_window),
            request_timeout_secs: override_config
                .request_timeout_secs
                .unwrap_or(self.request_timeout_secs),
        }
    }
}

/// HTTP bridge configuration. The bridge runs as a managed task inside
/// `mxr daemon` by default and binds to loopback only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeConfig {
    /// When true (default), `mxr daemon` automatically starts the bridge.
    /// Disable with `enabled = false` or `mxr daemon --no-bridge`.
    pub enabled: bool,
    /// Bind address. Defaults to `127.0.0.1`. Setting this to a non-loopback
    /// address requires explicit operator opt-in and additional safeguards
    /// (see `mxr-web`'s startup checks).
    pub bind: String,
    /// TCP port. Default `42829`, used for the stable local web URL.
    pub port: u16,
    /// Origins additive to the loopback CORS defaults. Empty by default.
    pub cors_allowlist: Vec<String>,
    /// Hostnames additive to the loopback Host-header defaults. Empty
    /// by default; populated only when binding to a non-loopback address.
    pub host_allowlist: Vec<String>,
    /// Path to the bridge token file. Defaults to
    /// `~/.config/mxr/bridge-token` (resolved at runtime when `None`).
    /// File is mode 0600, generated on first daemon start.
    pub token_path: Option<PathBuf>,
    /// When true (default), the bridge exposes an unauthenticated
    /// `GET /api/v1/auth/local-token` endpoint that returns the bridge
    /// token to callers connecting from a loopback peer address.
    ///
    /// This is a same-machine convenience so the web SPA can bootstrap
    /// without making the user paste a token, while still rejecting any
    /// caller whose TCP peer is non-loopback (preventing cross-network
    /// token disclosure even if the bridge is bound to 0.0.0.0).
    ///
    /// Set to `false` for paranoid setups that want a strict bearer
    /// handshake even on the local machine.
    pub auto_local_token: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "127.0.0.1".into(),
            // High unprivileged unassigned port that doesn't clash with the
            // common dev-server set (3000/5173/8000/8080/7777/4200).
            port: 42829,
            cors_allowlist: Vec::new(),
            host_allowlist: Vec::new(),
            token_path: None,
            auto_local_token: true,
        }
    }
}

impl BridgeConfig {
    /// True iff `bind` is one of the loopback addresses. Used by the
    /// daemon's startup checks to refuse non-loopback binds without TLS.
    pub fn is_loopback_bind(&self) -> bool {
        matches!(
            self.bind.as_str(),
            "127.0.0.1" | "::1" | "[::1]" | "localhost"
        )
    }
}

/// General application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub editor: Option<String>,
    pub default_account: Option<String>,
    pub sync_interval: u64,
    pub hook_timeout: u64,
    pub attachment_dir: PathBuf,
    /// Default destination directory for user-initiated attachment saves
    /// (the TUI "save as..." flow). Distinct from `attachment_dir`, which
    /// is the daemon's internal cache for opened/inline attachments.
    pub download_dir: PathBuf,
    pub safety_policy: SafetyPolicy,
    /// IETF language tag for user-facing strings. Resolved against
    /// `mxr_core::i18n::AVAILABLE_LOCALES`; unknown values fall back to `en`.
    /// Override at runtime with the `MXR_LOCALE` environment variable.
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn default_locale() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SafetyPolicy {
    #[default]
    Full,
    Restricted,
    DraftOnly,
    ReadOnly,
}

impl SafetyPolicy {
    pub fn parse_env(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "restricted" => Some(Self::Restricted),
            "draft-only" | "draft_only" => Some(Self::DraftOnly),
            "read-only" | "read_only" => Some(Self::ReadOnly),
            _ => None,
        }
    }
}

/// Local runtime authority profiles for non-human daemon clients such as
/// agents and MCP servers. Profiles are selected by IPC `ClientKind`
/// (`agent` or `mcp`) and enforced by the daemon before handlers touch
/// providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSurfaceConfig {
    pub profiles: HashMap<String, AgentProfileConfig>,
}

/// A specific destructive action an agent profile can be allowed to
/// perform. Used to refine the coarse `allow_destructive` gate down to
/// individual operations (the daemon maps each destructive request to
/// one of these).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructiveAction {
    Archive,
    Trash,
    Spam,
    Move,
    DeleteLabel,
    RemoveAccount,
    Unsubscribe,
    RedactActivity,
    PruneActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentProfileConfig {
    pub safety_policy: SafetyPolicy,
    pub allowed_accounts: Vec<String>,
    pub allow_send: bool,
    pub allow_destructive: bool,
    /// Optional fine-grained restriction *within* the destructive gate.
    /// When non-empty, a destructive request is allowed only if its
    /// mapped action is listed here (and `allow_destructive` is still
    /// true). When empty (the default), `allow_destructive` alone gates,
    /// preserving the previous all-or-nothing behaviour.
    pub allowed_destructive_actions: Vec<DestructiveAction>,
}

impl Default for AgentProfileConfig {
    fn default() -> Self {
        Self {
            safety_policy: SafetyPolicy::ReadOnly,
            allowed_accounts: Vec::new(),
            allow_send: false,
            allow_destructive: false,
            allowed_destructive_actions: Vec::new(),
        }
    }
}

/// Configuration for a single email account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub email: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub sync: Option<SyncProviderConfig>,
    pub send: Option<SendProviderConfig>,
}

/// Sync provider configuration (tagged enum).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SyncProviderConfig {
    Gmail {
        #[serde(default)]
        credential_source: GmailCredentialSource,
        client_id: String,
        client_secret: Option<String>,
        token_ref: String,
    },
    Imap {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        #[serde(default = "default_auth_required")]
        auth_required: bool,
        use_tls: bool,
    },
    OutlookPersonal {
        /// Azure app client ID. None = use bundled OUTLOOK_CLIENT_ID.
        client_id: Option<String>,
        /// Token file reference (e.g., "mxr/adrian-outlook").
        token_ref: String,
    },
    OutlookWork {
        /// Azure app client ID. None = use bundled OUTLOOK_CLIENT_ID.
        client_id: Option<String>,
        /// Token file reference (e.g., "mxr/work-outlook").
        token_ref: String,
    },
    /// In-memory provider used for CLI smoke tests.
    /// Generates fixture mail on startup. Not for production use.
    Fake,
}

/// Send provider configuration (tagged enum).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SendProviderConfig {
    Gmail,
    Smtp {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        #[serde(default = "default_auth_required")]
        auth_required: bool,
        use_tls: bool,
    },
    OutlookPersonal {
        /// Azure app client ID. None = use bundled OUTLOOK_CLIENT_ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// Token file reference — shared with sync provider.
        token_ref: String,
    },
    OutlookWork {
        /// Azure app client ID. None = use bundled OUTLOOK_CLIENT_ID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// Token file reference — shared with sync provider.
        token_ref: String,
    },
    /// Records sent drafts in memory; used with the fake sync provider for tests.
    Fake,
}

fn default_auth_required() -> bool {
    true
}

fn default_enabled() -> bool {
    true
}

/// HTML rendering configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    pub html_command: Option<String>,
    pub reader_mode: bool,
    pub show_reader_stats: bool,
    pub html_remote_content: bool,
}

/// Search configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub default_sort: SortOrder,
    pub max_results: usize,
    pub default_mode: CoreSearchMode,
    pub semantic: SemanticConfig,
}

/// Sort order for search results (config-local, not reusing core's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

/// Semantic search configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SemanticConfig {
    pub enabled: bool,
    pub auto_download_models: bool,
    pub active_profile: SemanticProfile,
    pub max_pending_jobs: usize,
    pub query_timeout_ms: u64,
}

/// Snooze timing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SnoozeConfig {
    pub morning_hour: u8,
    pub evening_hour: u8,
    pub weekend_day: String,
    pub weekend_hour: u8,
}

/// Logging / retention configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub max_size_mb: u32,
    pub max_files: u32,
    pub stderr: bool,
    pub event_retention_days: u32,
}

/// Appearance / UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: String,
    pub sidebar: bool,
    pub date_format: String,
    pub date_format_full: String,
    pub subject_max_width: usize,
}
