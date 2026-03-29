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
}

/// Configuration for a single email account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub email: String,
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
}

fn default_auth_required() -> bool {
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
