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
    pub render: RenderConfig,
    pub search: SearchConfig,
    pub snooze: SnoozeConfig,
    pub logging: LoggingConfig,
    pub appearance: AppearanceConfig,
    pub bridge: BridgeConfig,
    pub llm: LlmConfig,
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
    /// TCP port. Default `7777` — mnemonic, easy to type, low collision.
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
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: "127.0.0.1".into(),
            port: 7777,
            cors_allowlist: Vec::new(),
            host_allowlist: Vec::new(),
            token_path: None,
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
    pub safety_policy: SafetyPolicy,
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
