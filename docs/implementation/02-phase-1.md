# 02 — Phase 1: Gmail Read-Only + Search

> **Current Layout Note**
> This phase plan still uses the historical `mxr-*` crate names. Current code ships as one publishable package, `mxr`; those old crate names now map to modules mounted from `crates/*/src` under the root package.

## Goal

Read real email from Gmail. Search actually works with full query syntax. Config file parsed. After this phase, you can add your Gmail account, sync your inbox, browse messages in the TUI, read message bodies, search with the query syntax, create saved searches, and use the command palette. Full CLI surface for reading: `mxr cat`, `mxr thread`, `mxr headers`, `mxr count`, `mxr saved` subcommands. Daemon observability via `mxr status`, `mxr sync --status/--history`, `mxr logs`. TUI uses vim-native + Gmail keybindings (A005) with `g`-prefix navigation and `Ctrl-p` command palette. All from real Gmail data.

## Prerequisites

All of Phase 0 complete and working:
- Cargo workspace with all crate stubs
- `mxr-core`: All types (Envelope, Label, Thread, Draft, SavedSearch, MessageFlags, typed IDs, provider traits, error types)
- `mxr-store`: SQLite with sqlx, migrations, basic CRUD
- `mxr-protocol`: Request/Response/Command enums for IPC
- `mxr-provider-fake`: In-memory fake provider with fixture data
- `mxr-daemon`: Socket server, sync loop against fake provider, IPC dispatch
- `mxr-search`: Tantivy index, schema, basic indexing, simple text query
- `mxr-tui`: Two-pane layout, vim navigation, connected to daemon
- CI: fmt, clippy, test, build

## Key Decisions (Settled)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Gmail auth | `yup-oauth2` crate | Handles installed app flow, auto token refresh, disk persistence. Settled in D006. |
| Gmail API | Direct REST via `reqwest` | Not generated `google-gmail1` crate. Full control, less bloat. Settled in D006. |
| Delta sync | `history.list` API | Gmail's killer feature. Subsequent syncs are a handful of API calls. |
| Token storage | System keyring via `keyring` crate | Credentials never in config files. Settled in blueprint 12-config. |
| Query parser | Custom parser in search crate | Translates mxr query syntax to tantivy queries. Well-tested module. |
| Config format | TOML at XDG paths | `serde` + `toml` crate. Resolution: defaults -> config.toml -> env vars -> CLI flags. |
| SQL checking | Compile-time via `cargo sqlx prepare` | Runtime queries in Phase 0, compile-time checked from Phase 1+. Settled in decision log. |

---

## Step 1: Config Crate

### Crate/Module

New crate: `crates/config/`

### Files to Create/Modify

```
crates/config/Cargo.toml
crates/config/src/lib.rs
crates/config/src/types.rs
crates/config/src/defaults.rs
crates/config/src/env.rs
crates/config/src/resolve.rs
Cargo.toml                        # Add to workspace members + workspace.dependencies
crates/daemon/Cargo.toml          # Add mxr-config dependency
```

### External Dependencies

```toml
# crates/config/Cargo.toml
[dependencies]
mxr-core = { workspace = true }
serde = { workspace = true }
toml = "0.8"                  # TOML parsing
dirs = { workspace = true }   # XDG path resolution
thiserror = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tempfile = "3"
```

Add to workspace `Cargo.toml`:
```toml
[workspace.dependencies]
mxr-config = { path = "crates/config" }
toml = "0.8"
```

### Key Code Patterns

```rust
// crates/config/src/types.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level config file structure matching config.toml schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MxrConfig {
    pub general: GeneralConfig,
    pub accounts: HashMap<String, AccountConfig>,
    pub render: RenderConfig,
    pub search: SearchConfig,
    pub snooze: SnoozeConfig,
    pub appearance: AppearanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub editor: Option<String>,
    pub default_account: Option<String>,
    pub sync_interval: u64,
    pub attachment_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub name: String,
    pub email: String,
    pub sync: Option<SyncProviderConfig>,
    pub send: Option<SendProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum SyncProviderConfig {
    #[serde(rename = "gmail")]
    Gmail {
        client_id: String,
        client_secret: Option<String>,
        token_ref: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum SendProviderConfig {
    #[serde(rename = "gmail")]
    Gmail,
    #[serde(rename = "smtp")]
    Smtp {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        use_tls: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderConfig {
    pub html_command: Option<String>,
    pub reader_mode: bool,
    pub show_reader_stats: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    pub default_sort: SortOrder,
    pub max_results: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SnoozeConfig {
    pub morning_hour: u8,
    pub evening_hour: u8,
    pub weekend_day: String,
    pub weekend_hour: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: String,
    pub sidebar: bool,
    pub date_format: String,
    pub date_format_full: String,
    pub subject_max_width: usize,
}
```

```rust
// crates/config/src/defaults.rs

impl Default for MxrConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            accounts: HashMap::new(),
            render: RenderConfig::default(),
            search: SearchConfig::default(),
            snooze: SnoozeConfig::default(),
            appearance: AppearanceConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            editor: None, // falls back to $EDITOR -> $VISUAL -> "vi"
            default_account: None,
            sync_interval: 60,
            attachment_dir: dirs::home_dir()
                .unwrap_or_default()
                .join("mxr")
                .join("attachments"),
        }
    }
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            html_command: None,
            reader_mode: true,
            show_reader_stats: true,
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_sort: SortOrder::DateDesc,
            max_results: 200,
        }
    }
}

impl Default for SnoozeConfig {
    fn default() -> Self {
        Self {
            morning_hour: 9,
            evening_hour: 18,
            weekend_day: "saturday".into(),
            weekend_hour: 10,
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "default".into(),
            sidebar: true,
            date_format: "%b %d".into(),
            date_format_full: "%Y-%m-%d %H:%M".into(),
            subject_max_width: 60,
        }
    }
}
```

```rust
// crates/config/src/resolve.rs

use crate::types::MxrConfig;
use std::path::{Path, PathBuf};

/// Returns the config file path, respecting XDG on Linux and
/// ~/Library/Application Support on macOS.
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("mxr")
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("mxr")
}

/// Load and resolve config: defaults -> config.toml -> env vars.
/// CLI flags are applied by the caller after this returns.
pub fn load_config() -> Result<MxrConfig, ConfigError> {
    let mut config = MxrConfig::default();

    // Layer 2: config.toml (if it exists)
    let path = config_file_path();
    if path.exists() {
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| ConfigError::ReadFile { path: path.clone(), source: e })?;
        config = toml::from_str(&contents)
            .map_err(|e| ConfigError::ParseToml { path: path.clone(), source: e })?;
    }

    // Layer 3: environment variable overrides
    apply_env_overrides(&mut config);

    Ok(config)
}

fn apply_env_overrides(config: &mut MxrConfig) {
    if let Ok(val) = std::env::var("MXR_EDITOR") {
        config.general.editor = Some(val);
    }
    if let Ok(val) = std::env::var("MXR_SYNC_INTERVAL") {
        if let Ok(v) = val.parse::<u64>() {
            config.general.sync_interval = v;
        }
    }
    if let Ok(val) = std::env::var("MXR_DEFAULT_ACCOUNT") {
        config.general.default_account = Some(val);
    }
    if let Ok(val) = std::env::var("MXR_ATTACHMENT_DIR") {
        config.general.attachment_dir = PathBuf::from(val);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML in {path}: {source}")]
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },
}
```

```rust
// crates/config/src/lib.rs

mod types;
mod defaults;
mod env;
mod resolve;

pub use types::*;
pub use resolve::{load_config, config_dir, config_file_path, data_dir, ConfigError};
```

### What to Test

1. **Default config**: `MxrConfig::default()` produces valid config with expected values.
2. **TOML parsing**: Round-trip a full config.toml string through `toml::from_str` -> `toml::to_string_pretty`.
3. **Partial TOML**: A config.toml with only `[general]` section parses, rest uses defaults.
4. **Env overrides**: Set `MXR_SYNC_INTERVAL=30`, verify it overrides config file value.
5. **XDG paths**: On Linux, `config_dir()` returns `~/.config/mxr`. On macOS, `~/Library/Application Support/mxr`.
6. **Missing file**: `load_config()` returns defaults when no config file exists.
7. **Invalid TOML**: `load_config()` returns `ConfigError::ParseToml`.
8. **Account config variants**: Gmail sync + Gmail send, Gmail sync + SMTP send both parse correctly.

### `mxr config` Subcommand

Add to the daemon binary's clap CLI:

```rust
// In crates/daemon/src/cli.rs (or wherever clap is defined)

#[derive(clap::Subcommand)]
pub enum Command {
    /// Show resolved configuration
    Config,
    // ... existing subcommands
}

// Handler:
fn handle_config() -> anyhow::Result<()> {
    let config = mxr_config::load_config()?;
    let output = toml::to_string_pretty(&config)?;
    println!("{output}");
    Ok(())
}
```

---

## Step 2: Gmail Provider Crate

### Crate/Module

New crate: `crates/provider-gmail/`

### Files to Create/Modify

```
crates/provider-gmail/Cargo.toml
crates/provider-gmail/src/lib.rs
crates/provider-gmail/src/auth.rs           # OAuth2 flow + token management
crates/provider-gmail/src/client.rs         # HTTP client wrapper for Gmail REST
crates/provider-gmail/src/provider.rs       # MailSyncProvider implementation
crates/provider-gmail/src/types.rs          # Gmail API response types (serde)
crates/provider-gmail/src/parse.rs          # Gmail message -> Envelope conversion
crates/provider-gmail/src/error.rs          # Provider-specific errors
Cargo.toml                                  # Add to workspace members + deps
crates/daemon/Cargo.toml                    # Add mxr-provider-gmail dependency
```

### External Dependencies

```toml
# crates/provider-gmail/Cargo.toml
[dependencies]
mxr-core = { workspace = true }
mxr-config = { workspace = true }

# OAuth2
yup-oauth2 = "11"

# HTTP
reqwest = { version = "0.12", features = ["json"] }

# Token storage
keyring = { version = "3", features = ["apple-native", "linux-native"] }

# Parsing
serde = { workspace = true }
serde_json = { workspace = true }
base64 = "0.22"
mail-parser = "0.9"       # RFC 2822 / MIME parsing for List-Unsubscribe

# Async
tokio = { workspace = true }
async-trait = { workspace = true }

# Error handling
thiserror = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }
url = "2"

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
wiremock = "0.6"          # HTTP mocking for API tests
```

Add to workspace `Cargo.toml`:
```toml
[workspace.dependencies]
mxr-provider-gmail = { path = "crates/provider-gmail" }
yup-oauth2 = "11"
reqwest = { version = "0.12", features = ["json"] }
keyring = { version = "3", features = ["apple-native", "linux-native"] }
base64 = "0.22"
mail-parser = "0.9"
url = "2"
wiremock = "0.6"
```

### Key Code Patterns

#### OAuth2 Authentication

```rust
// crates/provider-gmail/src/auth.rs

use keyring::Entry;
use yup_oauth2::{
    InstalledFlowAuthenticator, InstalledFlowReturnMethod,
    authenticator::Authenticator,
    hyper_rustls::HttpsConnector,
};
use thiserror::Error;

const GMAIL_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.labels",
];

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("OAuth2 error: {0}")]
    OAuth2(String),
    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Token not found for {token_ref}")]
    TokenNotFound { token_ref: String },
    #[error("Browser auth required — run `mxr accounts add gmail`")]
    BrowserAuthRequired,
}

/// Manages OAuth2 tokens for a single Gmail account.
pub struct GmailAuth {
    token_ref: String,
    client_id: String,
    client_secret: String,
    authenticator: Option<Authenticator<HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>>>,
}

impl GmailAuth {
    pub fn new(client_id: String, client_secret: String, token_ref: String) -> Self {
        Self {
            token_ref,
            client_id,
            client_secret,
            authenticator: None,
        }
    }

    /// Run the interactive OAuth2 flow: opens browser, waits for callback on localhost.
    /// Stores the refresh token in the system keyring.
    pub async fn interactive_auth(&mut self) -> Result<(), AuthError> {
        let secret = yup_oauth2::ApplicationSecret {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
            token_uri: "https://oauth2.googleapis.com/token".into(),
            redirect_uris: vec!["http://localhost".into()],
            ..Default::default()
        };

        // yup-oauth2 handles:
        // 1. Starting a localhost HTTP server for the redirect
        // 2. Opening the browser to Google's consent page
        // 3. Exchanging the auth code for tokens
        // 4. Persisting tokens to disk (we also store refresh token in keyring)
        let token_storage_path = mxr_config::data_dir().join(
            format!("tokens/{}.json", self.token_ref)
        );

        std::fs::create_dir_all(token_storage_path.parent().unwrap())
            .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        let auth = InstalledFlowAuthenticator::builder(
            secret,
            InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(token_storage_path)
        .build()
        .await
        .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        // Force a token fetch to validate the flow completed
        let _token = auth
            .token(GMAIL_SCOPES)
            .await
            .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        self.authenticator = Some(auth);
        Ok(())
    }

    /// Load existing tokens from disk (non-interactive).
    /// Returns error if no tokens exist (user must run interactive_auth first).
    pub async fn load_existing(&mut self) -> Result<(), AuthError> {
        let token_storage_path = mxr_config::data_dir().join(
            format!("tokens/{}.json", self.token_ref)
        );

        if !token_storage_path.exists() {
            return Err(AuthError::BrowserAuthRequired);
        }

        let secret = yup_oauth2::ApplicationSecret {
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            auth_uri: "https://accounts.google.com/o/oauth2/auth".into(),
            token_uri: "https://oauth2.googleapis.com/token".into(),
            redirect_uris: vec!["http://localhost".into()],
            ..Default::default()
        };

        let auth = InstalledFlowAuthenticator::builder(
            secret,
            InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(token_storage_path)
        .build()
        .await
        .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        self.authenticator = Some(auth);
        Ok(())
    }

    /// Get a valid access token (auto-refreshes if expired).
    pub async fn access_token(&self) -> Result<String, AuthError> {
        let auth = self.authenticator.as_ref()
            .ok_or(AuthError::BrowserAuthRequired)?;

        let token = auth
            .token(GMAIL_SCOPES)
            .await
            .map_err(|e| AuthError::OAuth2(e.to_string()))?;

        Ok(token.token()
            .ok_or_else(|| AuthError::OAuth2("no access token in response".into()))?
            .to_string())
    }
}
```

#### Gmail API Client

```rust
// crates/provider-gmail/src/client.rs

use reqwest::Client;
use crate::auth::GmailAuth;
use crate::types::*;
use crate::error::GmailError;

const BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

/// Rate limit: Gmail allows 15,000 quota units per user per minute.
/// messages.list = 5 units, messages.get = 5 units, history.list = 2 units.
/// We track usage and back off when approaching the limit.
pub struct GmailClient {
    http: Client,
    auth: GmailAuth,
}

impl GmailClient {
    pub fn new(auth: GmailAuth) -> Self {
        let http = Client::builder()
            .user_agent("mxr/0.1")
            .build()
            .expect("failed to build HTTP client");
        Self { http, auth }
    }

    /// List message IDs, paginated. Returns (message_ids, next_page_token).
    pub async fn list_messages(
        &self,
        label_ids: Option<&[&str]>,
        max_results: u32,
        page_token: Option<&str>,
    ) -> Result<GmailListResponse, GmailError> {
        let token = self.auth.access_token().await?;
        let mut req = self.http
            .get(format!("{BASE_URL}/messages"))
            .bearer_auth(&token)
            .query(&[("maxResults", max_results.to_string())]);

        if let Some(labels) = label_ids {
            for label in labels {
                req = req.query(&[("labelIds", label)]);
            }
        }
        if let Some(pt) = page_token {
            req = req.query(&[("pageToken", pt)]);
        }

        let resp = req.send().await?;
        self.handle_error(resp).await?.json().await.map_err(Into::into)
    }

    /// Get a single message by ID.
    pub async fn get_message(
        &self,
        id: &str,
        format: MessageFormat,
    ) -> Result<GmailMessage, GmailError> {
        let token = self.auth.access_token().await?;
        let format_str = match format {
            MessageFormat::Metadata => "metadata",
            MessageFormat::Full => "full",
            MessageFormat::Minimal => "minimal",
        };

        let resp = self.http
            .get(format!("{BASE_URL}/messages/{id}"))
            .bearer_auth(&token)
            .query(&[("format", format_str)])
            .send()
            .await?;

        self.handle_error(resp).await?.json().await.map_err(Into::into)
    }

    /// Fetch message history (delta sync).
    pub async fn list_history(
        &self,
        start_history_id: u64,
        page_token: Option<&str>,
    ) -> Result<GmailHistoryResponse, GmailError> {
        let token = self.auth.access_token().await?;
        let mut req = self.http
            .get(format!("{BASE_URL}/history"))
            .bearer_auth(&token)
            .query(&[
                ("startHistoryId", start_history_id.to_string()),
                ("historyTypes", "messageAdded,messageDeleted,labelAdded,labelRemoved".into()),
            ]);

        if let Some(pt) = page_token {
            req = req.query(&[("pageToken", pt)]);
        }

        let resp = req.send().await?;
        self.handle_error(resp).await?.json().await.map_err(Into::into)
    }

    /// List all labels for the account.
    pub async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
        let token = self.auth.access_token().await?;
        let resp = self.http
            .get(format!("{BASE_URL}/labels"))
            .bearer_auth(&token)
            .send()
            .await?;

        self.handle_error(resp).await?.json().await.map_err(Into::into)
    }

    /// Batch get multiple messages in a single HTTP request.
    /// Gmail supports multipart batch requests (up to 100 operations).
    pub async fn batch_get_messages(
        &self,
        ids: &[String],
        format: MessageFormat,
    ) -> Result<Vec<GmailMessage>, GmailError> {
        // For initial implementation: parallel individual requests
        // (true batch API uses multipart/mixed which is more complex).
        // Limit concurrency to avoid hitting rate limits.
        let token = self.auth.access_token().await?;
        let format_str = match format {
            MessageFormat::Metadata => "metadata",
            MessageFormat::Full => "full",
            MessageFormat::Minimal => "minimal",
        };

        let mut results = Vec::with_capacity(ids.len());
        // Process in chunks of 20 to limit concurrency
        for chunk in ids.chunks(20) {
            let futs: Vec<_> = chunk.iter().map(|id| {
                let http = &self.http;
                let token = &token;
                async move {
                    let resp = http
                        .get(format!("{BASE_URL}/messages/{id}"))
                        .bearer_auth(token)
                        .query(&[("format", format_str)])
                        .send()
                        .await?;
                    resp.json::<GmailMessage>().await
                }
            }).collect();

            let chunk_results = futures::future::join_all(futs).await;
            for r in chunk_results {
                results.push(r?);
            }
        }
        Ok(results)
    }

    /// Handle HTTP errors: 401 -> refresh, 404 on history -> full re-sync, 429 -> backoff.
    async fn handle_error(&self, resp: reqwest::Response) -> Result<reqwest::Response, GmailError> {
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            401 => {
                // Token expired — yup-oauth2 should auto-refresh, but if we
                // still get 401 the refresh token may be revoked.
                Err(GmailError::AuthExpired)
            }
            404 => {
                let body = resp.text().await.unwrap_or_default();
                Err(GmailError::NotFound(body))
            }
            429 => {
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(60);
                Err(GmailError::RateLimited { retry_after_secs: retry_after })
            }
            status => {
                let body = resp.text().await.unwrap_or_default();
                Err(GmailError::Api { status, body })
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MessageFormat {
    Metadata,
    Full,
    Minimal,
}
```

#### Gmail API Response Types

```rust
// crates/provider-gmail/src/types.rs

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailListResponse {
    pub messages: Option<Vec<GmailMessageRef>>,
    pub next_page_token: Option<String>,
    pub result_size_estimate: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct GmailMessageRef {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailMessage {
    pub id: String,
    pub thread_id: String,
    pub label_ids: Option<Vec<String>>,
    pub snippet: Option<String>,
    pub history_id: Option<String>,
    pub internal_date: Option<String>,
    pub size_estimate: Option<u64>,
    pub payload: Option<GmailPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailPayload {
    pub mime_type: Option<String>,
    pub headers: Option<Vec<GmailHeader>>,
    pub body: Option<GmailBody>,
    pub parts: Option<Vec<GmailPayload>>,
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GmailHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailBody {
    pub attachment_id: Option<String>,
    pub size: Option<u64>,
    pub data: Option<String>, // base64url encoded
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryResponse {
    pub history: Option<Vec<GmailHistoryRecord>>,
    pub next_page_token: Option<String>,
    pub history_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryRecord {
    pub id: String,
    pub messages: Option<Vec<GmailMessageRef>>,
    pub messages_added: Option<Vec<GmailHistoryMessageAdded>>,
    pub messages_deleted: Option<Vec<GmailHistoryMessageDeleted>>,
    pub labels_added: Option<Vec<GmailHistoryLabelAdded>>,
    pub labels_removed: Option<Vec<GmailHistoryLabelRemoved>>,
}

#[derive(Debug, Deserialize)]
pub struct GmailHistoryMessageAdded {
    pub message: GmailMessageRef,
}

#[derive(Debug, Deserialize)]
pub struct GmailHistoryMessageDeleted {
    pub message: GmailMessageRef,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryLabelAdded {
    pub message: GmailMessageRef,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailHistoryLabelRemoved {
    pub message: GmailMessageRef,
    pub label_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GmailLabelsResponse {
    pub labels: Option<Vec<GmailLabel>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: Option<String>,
    pub messages_total: Option<u32>,
    pub messages_unread: Option<u32>,
    pub color: Option<GmailLabelColor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GmailLabelColor {
    pub text_color: Option<String>,
    pub background_color: Option<String>,
}
```

#### Gmail Message -> Envelope Conversion

```rust
// crates/provider-gmail/src/parse.rs

use mxr_core::{Address, Envelope, MessageFlags, MessageId, AccountId, ThreadId, UnsubscribeMethod};
use crate::types::{GmailMessage, GmailHeader, GmailPayload};
use chrono::{DateTime, Utc};

/// Convert a Gmail API message (metadata format) into an mxr Envelope.
pub fn gmail_message_to_envelope(
    msg: &GmailMessage,
    account_id: &AccountId,
) -> Result<Envelope, ParseError> {
    let headers = msg.payload.as_ref()
        .and_then(|p| p.headers.as_ref())
        .cloned()
        .unwrap_or_default();

    let from = parse_address(&find_header(&headers, "From").unwrap_or_default());
    let to = parse_address_list(&find_header(&headers, "To").unwrap_or_default());
    let cc = parse_address_list(&find_header(&headers, "Cc").unwrap_or_default());
    let subject = find_header(&headers, "Subject").unwrap_or_default();
    let message_id_header = find_header(&headers, "Message-ID");
    let in_reply_to = find_header(&headers, "In-Reply-To");
    let references = find_header(&headers, "References")
        .map(|r| r.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    let date = msg.internal_date.as_ref()
        .and_then(|d| d.parse::<i64>().ok())
        .and_then(|ms| DateTime::from_timestamp_millis(ms))
        .unwrap_or_else(Utc::now);

    let flags = labels_to_flags(msg.label_ids.as_deref().unwrap_or(&[]));
    let has_attachments = check_has_attachments(msg.payload.as_ref());
    let unsubscribe = parse_list_unsubscribe(&headers);

    Ok(Envelope {
        id: MessageId::new(),
        account_id: account_id.clone(),
        provider_id: msg.id.clone(),
        thread_id: ThreadId::from_string(&msg.thread_id),
        message_id_header,
        in_reply_to,
        references,
        from,
        to,
        cc,
        bcc: vec![],
        subject,
        date,
        flags,
        snippet: msg.snippet.clone().unwrap_or_default(),
        has_attachments,
        size_bytes: msg.size_estimate.unwrap_or(0),
        unsubscribe,
    })
}

fn find_header(headers: &[GmailHeader], name: &str) -> Option<String> {
    headers.iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
}

fn labels_to_flags(label_ids: &[String]) -> MessageFlags {
    let mut flags = MessageFlags::empty();
    for label in label_ids {
        match label.as_str() {
            "UNREAD" => {} // absence of READ flag means unread
            "STARRED" => flags |= MessageFlags::STARRED,
            "DRAFT" => flags |= MessageFlags::DRAFT,
            "SENT" => flags |= MessageFlags::SENT,
            "TRASH" => flags |= MessageFlags::TRASH,
            "SPAM" => flags |= MessageFlags::SPAM,
            _ => {}
        }
    }
    // If UNREAD is NOT in labels, the message is read
    if !label_ids.iter().any(|l| l == "UNREAD") {
        flags |= MessageFlags::READ;
    }
    flags
}

fn check_has_attachments(payload: Option<&GmailPayload>) -> bool {
    let Some(payload) = payload else { return false };
    if let Some(parts) = &payload.parts {
        for part in parts {
            if part.filename.as_ref().is_some_and(|f| !f.is_empty()) {
                return true;
            }
            if check_has_attachments(Some(part)) {
                return true;
            }
        }
    }
    false
}

/// Parse List-Unsubscribe header (RFC 2369) into UnsubscribeMethod.
fn parse_list_unsubscribe(headers: &[GmailHeader]) -> UnsubscribeMethod {
    let Some(value) = find_header(headers, "List-Unsubscribe") else {
        return UnsubscribeMethod::None;
    };

    // Check for one-click (RFC 8058)
    let has_one_click = find_header(headers, "List-Unsubscribe-Post")
        .is_some_and(|v| v.contains("List-Unsubscribe=One-Click"));

    // Parse angle-bracket URIs: <https://...>, <mailto:...>
    let uris: Vec<&str> = value
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            if s.starts_with('<') && s.ends_with('>') {
                Some(&s[1..s.len()-1])
            } else {
                None
            }
        })
        .collect();

    // Prefer one-click POST, then HTTP link, then mailto
    if has_one_click {
        if let Some(url) = uris.iter().find(|u| u.starts_with("https://") || u.starts_with("http://")) {
            return UnsubscribeMethod::OneClick { url: url.to_string() };
        }
    }

    if let Some(url) = uris.iter().find(|u| u.starts_with("https://") || u.starts_with("http://")) {
        return UnsubscribeMethod::HttpLink { url: url.to_string() };
    }

    if let Some(mailto) = uris.iter().find(|u| u.starts_with("mailto:")) {
        let addr = mailto.trim_start_matches("mailto:");
        let (address, subject) = if let Some((a, params)) = addr.split_once('?') {
            let subj = params.strip_prefix("subject=").map(String::from);
            (a.to_string(), subj)
        } else {
            (addr.to_string(), None)
        };
        return UnsubscribeMethod::Mailto { address, subject };
    }

    UnsubscribeMethod::None
}

/// Parse a single email address: "Name <email@example.com>" or "email@example.com"
fn parse_address(raw: &str) -> Address {
    let raw = raw.trim();
    if let Some(pos) = raw.rfind('<') {
        let name = raw[..pos].trim().trim_matches('"');
        let email = raw[pos+1..].trim_end_matches('>').trim();
        Address {
            name: if name.is_empty() { None } else { Some(name.to_string()) },
            email: email.to_string(),
        }
    } else {
        Address { name: None, email: raw.to_string() }
    }
}

/// Parse comma-separated address list.
fn parse_address_list(raw: &str) -> Vec<Address> {
    if raw.is_empty() {
        return vec![];
    }
    // Simplified: split on commas not inside angle brackets
    raw.split(',').map(|s| parse_address(s.trim())).collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("missing required header: {0}")]
    MissingHeader(String),
}
```

#### Gmail Provider (MailSyncProvider impl)

```rust
// crates/provider-gmail/src/provider.rs

use async_trait::async_trait;
use mxr_core::{
    AccountId, Envelope, Label, LabelId, LabelKind, MessageBody,
    MailSyncProvider, SyncCapabilities, SyncCursor, SyncBatch, LabelChange,
};
use crate::client::{GmailClient, MessageFormat};
use crate::parse::gmail_message_to_envelope;

pub struct GmailProvider {
    account_id: AccountId,
    client: GmailClient,
}

impl GmailProvider {
    pub fn new(account_id: AccountId, client: GmailClient) -> Self {
        Self { account_id, client }
    }
}

#[async_trait]
impl MailSyncProvider for GmailProvider {
    fn name(&self) -> &str { "gmail" }
    fn account_id(&self) -> &AccountId { &self.account_id }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: true,
            server_search: true,
            delta_sync: true,
            push: false, // pub/sub not implemented in Phase 1
            batch_operations: true,
        }
    }

    async fn authenticate(&mut self) -> Result<()> {
        // Called during interactive setup (`mxr accounts add gmail`)
        // Delegates to GmailAuth::interactive_auth
        todo!("wire to self.client.auth.interactive_auth()")
    }

    async fn refresh_auth(&mut self) -> Result<()> {
        // yup-oauth2 handles this automatically on token() calls.
        // This is a no-op for Gmail — if the refresh token is revoked,
        // access_token() will return AuthExpired.
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>> {
        let resp = self.client.list_labels().await?;
        let labels = resp.labels.unwrap_or_default();
        Ok(labels.into_iter().map(|gl| {
            let kind = match gl.label_type.as_deref() {
                Some("system") => LabelKind::System,
                Some("user") => LabelKind::User,
                _ => LabelKind::User,
            };
            Label {
                id: LabelId::new(),
                account_id: self.account_id.clone(),
                name: gl.name,
                kind,
                color: gl.color.and_then(|c| c.background_color),
                provider_id: gl.id,
                unread_count: gl.messages_unread.unwrap_or(0),
                total_count: gl.messages_total.unwrap_or(0),
            }
        }).collect())
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch> {
        match cursor {
            SyncCursor::Initial => self.initial_sync().await,
            SyncCursor::Gmail { history_id } => self.delta_sync(*history_id).await,
            _ => Err(anyhow::anyhow!("unsupported cursor type for Gmail")),
        }
    }

    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody> {
        let msg = self.client.get_message(provider_message_id, MessageFormat::Full).await?;
        extract_body(&msg)
    }

    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>> {
        todo!("GET /messages/{id}/attachments/{attachment_id}")
    }

    async fn modify_labels(
        &self,
        _provider_message_id: &str,
        _add: &[String],
        _remove: &[String],
    ) -> Result<()> {
        // Phase 1 is read-only. This will be implemented in Phase 2.
        Err(anyhow::anyhow!("mutations not supported in Phase 1"))
    }

    async fn trash(&self, _provider_message_id: &str) -> Result<()> {
        Err(anyhow::anyhow!("mutations not supported in Phase 1"))
    }

    async fn set_read(&self, _provider_message_id: &str, _read: bool) -> Result<()> {
        Err(anyhow::anyhow!("mutations not supported in Phase 1"))
    }

    async fn set_starred(&self, _provider_message_id: &str, _starred: bool) -> Result<()> {
        Err(anyhow::anyhow!("mutations not supported in Phase 1"))
    }
}

impl GmailProvider {
    /// Full initial sync: list all messages, fetch metadata in batches.
    async fn initial_sync(&self) -> Result<SyncBatch> {
        let mut all_envelopes = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest_history_id: u64 = 0;

        loop {
            // 1. List message IDs (paginated, newest first)
            let list_resp = self.client
                .list_messages(None, 100, page_token.as_deref())
                .await?;

            let msg_refs = list_resp.messages.unwrap_or_default();
            if msg_refs.is_empty() {
                break;
            }

            // 2. Batch fetch metadata for this page
            let ids: Vec<String> = msg_refs.iter().map(|m| m.id.clone()).collect();
            let messages = self.client
                .batch_get_messages(&ids, MessageFormat::Metadata)
                .await?;

            // 3. Convert to envelopes
            for msg in &messages {
                let envelope = gmail_message_to_envelope(msg, &self.account_id)?;
                all_envelopes.push(envelope);

                // Track highest history ID
                if let Some(hid) = msg.history_id.as_ref().and_then(|h| h.parse::<u64>().ok()) {
                    latest_history_id = latest_history_id.max(hid);
                }
            }

            page_token = list_resp.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(SyncBatch {
            upserted: all_envelopes,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor: SyncCursor::Gmail { history_id: latest_history_id },
        })
    }

    /// Delta sync: fetch only changes since last history_id.
    async fn delta_sync(&self, start_history_id: u64) -> Result<SyncBatch> {
        let mut upserted_ids: Vec<String> = Vec::new();
        let mut deleted_ids: Vec<String> = Vec::new();
        let mut label_changes: Vec<LabelChange> = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest_history_id = start_history_id;

        loop {
            let resp = self.client
                .list_history(start_history_id, page_token.as_deref())
                .await;

            // Handle 404: history ID too old -> need full re-sync
            let resp = match resp {
                Ok(r) => r,
                Err(crate::error::GmailError::NotFound(_)) => {
                    tracing::warn!("History ID {start_history_id} expired, triggering full re-sync");
                    return self.initial_sync().await;
                }
                Err(e) => return Err(e.into()),
            };

            if let Some(history_id) = resp.history_id.as_ref().and_then(|h| h.parse::<u64>().ok()) {
                latest_history_id = latest_history_id.max(history_id);
            }

            for record in resp.history.unwrap_or_default() {
                // Messages added
                if let Some(added) = record.messages_added {
                    for a in added {
                        upserted_ids.push(a.message.id);
                    }
                }
                // Messages deleted
                if let Some(deleted) = record.messages_deleted {
                    for d in deleted {
                        deleted_ids.push(d.message.id);
                    }
                }
                // Labels added
                if let Some(label_added) = record.labels_added {
                    for la in label_added {
                        label_changes.push(LabelChange {
                            provider_message_id: la.message.id,
                            added_labels: la.label_ids,
                            removed_labels: vec![],
                        });
                    }
                }
                // Labels removed
                if let Some(label_removed) = record.labels_removed {
                    for lr in label_removed {
                        label_changes.push(LabelChange {
                            provider_message_id: lr.message.id,
                            added_labels: vec![],
                            removed_labels: lr.label_ids,
                        });
                    }
                }
            }

            page_token = resp.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        // Fetch full metadata for newly added messages
        let mut envelopes = Vec::new();
        if !upserted_ids.is_empty() {
            let messages = self.client
                .batch_get_messages(&upserted_ids, MessageFormat::Metadata)
                .await?;
            for msg in &messages {
                envelopes.push(gmail_message_to_envelope(msg, &self.account_id)?);
            }
        }

        Ok(SyncBatch {
            upserted: envelopes,
            deleted_provider_ids: deleted_ids,
            label_changes,
            next_cursor: SyncCursor::Gmail { history_id: latest_history_id },
        })
    }
}

/// Extract text/plain and text/html from a full Gmail message.
fn extract_body(msg: &GmailMessage) -> Result<MessageBody> {
    let mut text_plain: Option<String> = None;
    let mut text_html: Option<String> = None;
    let mut attachments = Vec::new();

    if let Some(payload) = &msg.payload {
        walk_parts(payload, &mut text_plain, &mut text_html, &mut attachments);
    }

    Ok(MessageBody {
        message_id: MessageId::new(), // caller maps this
        text_plain,
        text_html,
        attachments,
        fetched_at: Utc::now(),
    })
}

fn walk_parts(
    part: &GmailPayload,
    text_plain: &mut Option<String>,
    text_html: &mut Option<String>,
    attachments: &mut Vec<AttachmentMeta>,
) {
    let mime = part.mime_type.as_deref().unwrap_or("");

    // If this part has a filename, it's an attachment
    if part.filename.as_ref().is_some_and(|f| !f.is_empty()) {
        if let Some(body) = &part.body {
            attachments.push(AttachmentMeta {
                id: AttachmentId::new(),
                message_id: MessageId::new(), // caller maps
                filename: part.filename.clone().unwrap_or_default(),
                mime_type: mime.to_string(),
                size_bytes: body.size.unwrap_or(0),
                local_path: None,
                provider_id: body.attachment_id.clone().unwrap_or_default(),
            });
        }
        return;
    }

    // Extract body text
    if mime == "text/plain" {
        if let Some(data) = part.body.as_ref().and_then(|b| b.data.as_ref()) {
            if let Ok(decoded) = base64_decode_url(data) {
                *text_plain = Some(decoded);
            }
        }
    } else if mime == "text/html" {
        if let Some(data) = part.body.as_ref().and_then(|b| b.data.as_ref()) {
            if let Ok(decoded) = base64_decode_url(data) {
                *text_html = Some(decoded);
            }
        }
    }

    // Recurse into multipart
    if let Some(parts) = &part.parts {
        for child in parts {
            walk_parts(child, text_plain, text_html, attachments);
        }
    }
}

/// Decode base64url (Gmail's encoding) to UTF-8 string.
fn base64_decode_url(data: &str) -> Result<String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let bytes = URL_SAFE_NO_PAD.decode(data)?;
    Ok(String::from_utf8(bytes)?)
}
```

#### Error Types

```rust
// crates/provider-gmail/src/error.rs

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GmailError {
    #[error("authentication expired — re-run `mxr accounts add gmail`")]
    AuthExpired,

    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("rate limited — retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Gmail API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("auth error: {0}")]
    Auth(#[from] crate::auth::AuthError),

    #[error("parse error: {0}")]
    Parse(#[from] crate::parse::ParseError),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
```

### What to Test

1. **Parse: Gmail message -> Envelope**: Unit tests with fixture JSON responses. Cover messages with/without attachments, various header combinations.
2. **Parse: List-Unsubscribe header**: Test one-click, HTTP, mailto, multi-URI, missing header cases.
3. **Parse: Address parsing**: "Name <email>", bare email, quoted names, empty strings.
4. **Parse: Labels -> Flags**: All system label combinations (UNREAD, STARRED, DRAFT, etc.).
5. **Client: Error handling**: Use `wiremock` to simulate 401, 404, 429 responses and verify correct error variants.
6. **Client: Pagination**: Mock paginated list_messages with 3 pages, verify all message IDs collected.
7. **Client: History parsing**: Mock history.list response with all delta types, verify SyncBatch is correct.
8. **Body extraction**: Test multipart/alternative (text + html), multipart/mixed (body + attachments), deeply nested MIME.
9. **Base64url decoding**: Gmail uses URL-safe base64 without padding.

---

## Step 3: Real Sync Integration

### Crate/Module

Modify existing `crates/sync/`

### Files to Create/Modify

```
crates/sync/src/lib.rs            # Update SyncEngine to be provider-generic
crates/sync/src/engine.rs         # Core sync orchestration
crates/sync/src/progress.rs       # Sync progress reporting
crates/daemon/src/main.rs         # Wire up Gmail provider based on config
crates/daemon/src/accounts.rs     # `mxr accounts add gmail` handler
```

### Key Code Patterns

```rust
// crates/sync/src/engine.rs

use mxr_core::{
    AccountId, MailSyncProvider, SyncCursor, SyncBatch,
};
use mxr_store::Store;
use mxr_search::SearchIndex;

pub struct SyncEngine {
    store: Store,
    search: SearchIndex,
}

impl SyncEngine {
    pub fn new(store: Store, search: SearchIndex) -> Self {
        Self { store, search }
    }

    /// Sync a single account. Works with any MailSyncProvider implementation.
    pub async fn sync_account(
        &self,
        provider: &dyn MailSyncProvider,
        progress: &dyn SyncProgressHandler,
    ) -> Result<SyncResult, SyncError> {
        let account_id = provider.account_id();

        // 1. Get current cursor from store
        let cursor = self.store
            .get_sync_cursor(account_id)
            .await?
            .unwrap_or(SyncCursor::Initial);

        let is_initial = matches!(cursor, SyncCursor::Initial);

        // 2. Log sync start
        let sync_log_id = self.store.log_sync_start(account_id).await?;

        // 3. Sync labels first
        if is_initial || provider.capabilities().labels {
            let labels = provider.sync_labels().await?;
            self.store.upsert_labels(account_id, &labels).await?;
            progress.on_labels_synced(labels.len());
        }

        // 4. Fetch message changes
        let batch = provider.sync_messages(&cursor).await?;

        // 5. Apply batch to store and search index
        let stats = self.apply_batch(account_id, &batch, progress).await?;

        // 6. Update cursor
        self.store.set_sync_cursor(account_id, &batch.next_cursor).await?;

        // 7. Update label counts
        self.store.recalculate_label_counts(account_id).await?;

        // 8. Log sync completion
        self.store.log_sync_complete(sync_log_id, stats.messages_synced).await?;

        progress.on_sync_complete(&stats);
        Ok(stats)
    }

    /// Apply a SyncBatch to store and search index.
    /// Processes in chunks for progressive loading.
    async fn apply_batch(
        &self,
        account_id: &AccountId,
        batch: &SyncBatch,
        progress: &dyn SyncProgressHandler,
    ) -> Result<SyncResult, SyncError> {
        let mut messages_synced = 0u32;

        // Upsert envelopes in chunks of 50 for progressive loading
        for chunk in batch.upserted.chunks(50) {
            self.store.upsert_envelopes(chunk).await?;

            // Index each envelope in Tantivy
            for envelope in chunk {
                self.search.index_envelope(envelope)?;
            }

            messages_synced += chunk.len() as u32;
            progress.on_messages_synced(messages_synced, batch.upserted.len() as u32);
        }

        // Apply deletions
        for provider_id in &batch.deleted_provider_ids {
            if let Some(msg_id) = self.store
                .get_message_id_by_provider_id(account_id, provider_id)
                .await?
            {
                self.store.delete_message(&msg_id).await?;
                self.search.delete_document(&msg_id)?;
            }
        }

        // Apply label changes
        for change in &batch.label_changes {
            self.store.apply_label_change(account_id, change).await?;
        }

        // Commit Tantivy changes
        self.search.commit()?;

        Ok(SyncResult {
            messages_synced,
            messages_deleted: batch.deleted_provider_ids.len() as u32,
            label_changes: batch.label_changes.len() as u32,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub messages_synced: u32,
    pub messages_deleted: u32,
    pub label_changes: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("store error: {0}")]
    Store(#[from] mxr_store::StoreError),
    #[error("search error: {0}")]
    Search(#[from] mxr_search::SearchError),
    #[error("provider error: {0}")]
    Provider(#[from] anyhow::Error),
}

/// Progress callback trait for sync updates (IPC events to TUI).
pub trait SyncProgressHandler: Send + Sync {
    fn on_labels_synced(&self, count: usize);
    fn on_messages_synced(&self, current: u32, total: u32);
    fn on_sync_complete(&self, result: &SyncResult);
    fn on_sync_error(&self, error: &SyncError);
}
```

```rust
// crates/daemon/src/accounts.rs

use mxr_config::{AccountConfig, SyncProviderConfig};
use mxr_provider_gmail::auth::GmailAuth;

/// Interactive `mxr accounts add gmail` flow.
pub async fn add_gmail_account(account_name: &str) -> anyhow::Result<()> {
    println!("Adding Gmail account: {account_name}");
    println!();
    println!("You need a Google Cloud project with the Gmail API enabled.");
    println!("See: https://console.cloud.google.com/apis/library/gmail.googleapis.com");
    println!();

    // 1. Prompt for client ID and secret
    let client_id = prompt("Client ID: ")?;
    let client_secret = prompt("Client Secret: ")?;

    let token_ref = format!("mxr/{account_name}-gmail");

    // 2. Run OAuth2 flow
    println!();
    println!("Opening browser for Google authorization...");
    let mut auth = GmailAuth::new(client_id.clone(), client_secret.clone(), token_ref.clone());
    auth.interactive_auth().await?;

    println!("Authorization successful!");
    println!();

    // 3. Prompt for email address
    let email = prompt("Email address for this account: ")?;

    // 4. Write account config to config.toml
    let config_entry = format!(
        r#"
[accounts.{account_name}]
name = "{account_name}"
email = "{email}"

[accounts.{account_name}.sync]
provider = "gmail"
client_id = "{client_id}"
client_secret = "{client_secret}"
token_ref = "{token_ref}"

[accounts.{account_name}.send]
provider = "gmail"
"#
    );

    // Append to config file (or create it)
    let config_path = mxr_config::config_file_path();
    std::fs::create_dir_all(config_path.parent().unwrap())?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config_path)?;

    use std::io::Write;
    writeln!(file, "{config_entry}")?;

    println!("Account config written to {}", config_path.display());
    println!("Run `mxr sync --account {account_name}` to start syncing.");
    Ok(())
}

fn prompt(msg: &str) -> anyhow::Result<String> {
    use std::io::{self, Write};
    print!("{msg}");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}
```

### What to Test

1. **Sync engine with fake provider**: Run full sync cycle (initial + delta) with FakeProvider, verify store and search index are populated correctly.
2. **Progressive loading**: Verify that after syncing 200 messages, they appear in 50-message chunks.
3. **Delta sync application**: Verify upserts, deletions, and label changes apply correctly.
4. **Cursor persistence**: After sync, cursor is stored. Next sync uses stored cursor.
5. **Cursor invalidation**: Simulate a provider returning "history expired" error, verify fallback to full re-sync.
6. **Sync error handling**: Provider errors don't crash the engine. Errors are logged and reported.
7. **Label count recalculation**: After sync, label unread/total counts are correct.

---

## Step 4: Search Query Parser

### Crate/Module

Modify existing `crates/search/`

### Files to Create/Modify

```
crates/search/src/parser.rs          # Query parser: string -> AST
crates/search/src/query_builder.rs   # AST -> tantivy::query::Query
crates/search/src/ast.rs             # Query AST types
crates/search/src/saved.rs           # Saved search CRUD
crates/search/src/lib.rs             # Re-export parser API
```

### External Dependencies

No new dependencies. The parser is hand-written (small grammar, no need for `nom` or `pest`). Tantivy is already a workspace dep.

### Key Code Patterns

#### Query AST

```rust
// crates/search/src/ast.rs

use chrono::NaiveDate;

/// Parsed query AST. Produced by the parser, consumed by the query builder.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryNode {
    /// Free text search across all searchable fields.
    Text(String),

    /// Exact phrase: "deployment plan"
    Phrase(String),

    /// Field-specific query: from:alice@example.com
    Field {
        field: QueryField,
        value: String,
    },

    /// Boolean filter: is:unread, is:starred, is:read, has:attachment
    Filter(FilterKind),

    /// Label filter: label:work
    Label(String),

    /// Date range: after:2026-01-01, before:2026-03-15
    DateRange {
        bound: DateBound,
        date: DateValue,
    },

    /// Boolean operators
    And(Box<QueryNode>, Box<QueryNode>),
    Or(Box<QueryNode>, Box<QueryNode>),
    Not(Box<QueryNode>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum QueryField {
    From,
    To,
    Subject,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterKind {
    Unread,
    Read,
    Starred,
    HasAttachment,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateBound {
    After,
    Before,
    Exact,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateValue {
    Specific(NaiveDate),
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
}
```

#### Query Parser

```rust
// crates/search/src/parser.rs

use crate::ast::*;
use chrono::NaiveDate;

/// Parse an mxr query string into a QueryNode AST.
///
/// Grammar (informal):
///   query     = term (("AND" | "OR") term)*
///   term      = "NOT" atom | "-" atom | atom
///   atom      = phrase | field_query | filter | label | date | text | "(" query ")"
///   phrase    = '"' ... '"'
///   field     = ("from" | "to" | "subject") ":" value
///   filter    = "is:" ("unread" | "read" | "starred") | "has:" "attachment"
///   label     = "label:" value
///   date      = ("after" | "before" | "date") ":" date_value
///   text      = word+
pub fn parse_query(input: &str) -> Result<QueryNode, ParseError> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(&tokens);
    let node = parser.parse_expression()?;

    if parser.pos < tokens.len() {
        return Err(ParseError::UnexpectedToken {
            pos: parser.pos,
            token: tokens[parser.pos].clone(),
        });
    }

    Ok(node)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Phrase(String),        // contents inside quotes
    Colon,
    Minus,
    LParen,
    RParen,
    And,
    Or,
    Not,
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    /// Parse top-level expression with implicit AND between terms.
    fn parse_expression(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_or()?;
        // Implicit AND: adjacent terms without explicit operator
        while self.pos < self.tokens.len()
            && !matches!(self.peek(), Some(Token::RParen))
        {
            let right = self.parse_or()?;
            left = QueryNode::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.advance();
            let right = self.parse_and()?;
            left = QueryNode::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<QueryNode, ParseError> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.advance();
            let right = self.parse_unary()?;
            left = QueryNode::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<QueryNode, ParseError> {
        match self.peek() {
            Some(Token::Not) => {
                self.advance();
                let inner = self.parse_atom()?;
                Ok(QueryNode::Not(Box::new(inner)))
            }
            Some(Token::Minus) => {
                self.advance();
                let inner = self.parse_atom()?;
                Ok(QueryNode::Not(Box::new(inner)))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<QueryNode, ParseError> {
        match self.peek().cloned() {
            Some(Token::Phrase(text)) => {
                self.advance();
                Ok(QueryNode::Phrase(text))
            }
            Some(Token::LParen) => {
                self.advance();
                let inner = self.parse_expression()?;
                match self.advance() {
                    Some(Token::RParen) => Ok(inner),
                    _ => Err(ParseError::UnmatchedParen),
                }
            }
            Some(Token::Word(word)) => {
                self.advance();
                // Check if this is a field:value pair
                if matches!(self.peek(), Some(Token::Colon)) {
                    self.advance(); // consume colon
                    let value = match self.advance() {
                        Some(Token::Word(v)) => v.clone(),
                        Some(Token::Phrase(v)) => v.clone(),
                        _ => return Err(ParseError::ExpectedValue),
                    };
                    self.parse_field_value(&word, &value)
                } else {
                    Ok(QueryNode::Text(word))
                }
            }
            _ => Err(ParseError::UnexpectedEnd),
        }
    }

    fn parse_field_value(&self, field: &str, value: &str) -> Result<QueryNode, ParseError> {
        match field {
            "from" => Ok(QueryNode::Field {
                field: QueryField::From,
                value: value.to_string(),
            }),
            "to" => Ok(QueryNode::Field {
                field: QueryField::To,
                value: value.to_string(),
            }),
            "subject" => Ok(QueryNode::Field {
                field: QueryField::Subject,
                value: value.to_string(),
            }),
            "label" => Ok(QueryNode::Label(value.to_string())),
            "is" => match value {
                "unread" => Ok(QueryNode::Filter(FilterKind::Unread)),
                "read" => Ok(QueryNode::Filter(FilterKind::Read)),
                "starred" => Ok(QueryNode::Filter(FilterKind::Starred)),
                _ => Err(ParseError::UnknownFilter(value.to_string())),
            },
            "has" => match value {
                "attachment" => Ok(QueryNode::Filter(FilterKind::HasAttachment)),
                _ => Err(ParseError::UnknownFilter(value.to_string())),
            },
            "after" => Ok(QueryNode::DateRange {
                bound: DateBound::After,
                date: parse_date_value(value)?,
            }),
            "before" => Ok(QueryNode::DateRange {
                bound: DateBound::Before,
                date: parse_date_value(value)?,
            }),
            "date" => Ok(QueryNode::DateRange {
                bound: DateBound::Exact,
                date: parse_date_value(value)?,
            }),
            _ => {
                // Unknown field: treat as text search for "field:value"
                Ok(QueryNode::Text(format!("{field}:{value}")))
            }
        }
    }
}

fn parse_date_value(s: &str) -> Result<DateValue, ParseError> {
    match s {
        "today" => Ok(DateValue::Today),
        "yesterday" => Ok(DateValue::Yesterday),
        "this-week" => Ok(DateValue::ThisWeek),
        "this-month" => Ok(DateValue::ThisMonth),
        _ => {
            let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| ParseError::InvalidDate(s.to_string()))?;
            Ok(DateValue::Specific(date))
        }
    }
}

/// Tokenize input string into tokens.
fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' | '\n' => { chars.next(); }
            '"' => {
                chars.next();
                let mut phrase = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '"' { chars.next(); break; }
                    phrase.push(ch);
                    chars.next();
                }
                tokens.push(Token::Phrase(phrase));
            }
            ':' => { chars.next(); tokens.push(Token::Colon); }
            '-' => { chars.next(); tokens.push(Token::Minus); }
            '(' => { chars.next(); tokens.push(Token::LParen); }
            ')' => { chars.next(); tokens.push(Token::RParen); }
            _ => {
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_whitespace() || ch == ':' || ch == '(' || ch == ')' {
                        break;
                    }
                    word.push(ch);
                    chars.next();
                }
                match word.as_str() {
                    "AND" => tokens.push(Token::And),
                    "OR" => tokens.push(Token::Or),
                    "NOT" => tokens.push(Token::Not),
                    _ => tokens.push(Token::Word(word)),
                }
            }
        }
    }

    Ok(tokens)
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected end of query")]
    UnexpectedEnd,
    #[error("unexpected token at position {pos}: {token:?}")]
    UnexpectedToken { pos: usize, token: Token },
    #[error("unmatched parenthesis")]
    UnmatchedParen,
    #[error("expected value after field name")]
    ExpectedValue,
    #[error("unknown filter: {0}")]
    UnknownFilter(String),
    #[error("invalid date: {0}")]
    InvalidDate(String),
}
```

#### Query Builder (AST -> Tantivy Query)

```rust
// crates/search/src/query_builder.rs

use crate::ast::*;
use tantivy::query::{
    BooleanQuery, Occur, Query, TermQuery, PhraseQuery, AllQuery,
    RangeQuery, BoostQuery,
};
use tantivy::schema::{Schema, Field, IndexRecordOption};
use tantivy::{Term, DateTime as TantivyDateTime};
use chrono::{Local, NaiveDate, Duration, Datelike, Weekday};

pub struct QueryBuilder {
    schema: Schema,
    // Cached field handles
    subject: Field,
    from_name: Field,
    from_email: Field,
    to_email: Field,
    snippet: Field,
    body_text: Field,
    labels: Field,
    date: Field,
    flags: Field,
    has_attachments: Field,
}

impl QueryBuilder {
    pub fn new(schema: Schema) -> Self {
        let subject = schema.get_field("subject").unwrap();
        let from_name = schema.get_field("from_name").unwrap();
        let from_email = schema.get_field("from_email").unwrap();
        let to_email = schema.get_field("to_email").unwrap();
        let snippet = schema.get_field("snippet").unwrap();
        let body_text = schema.get_field("body_text").unwrap();
        let labels = schema.get_field("labels").unwrap();
        let date = schema.get_field("date").unwrap();
        let flags = schema.get_field("flags").unwrap();
        let has_attachments = schema.get_field("has_attachments").unwrap();

        Self {
            schema, subject, from_name, from_email, to_email,
            snippet, body_text, labels, date, flags, has_attachments,
        }
    }

    /// Convert a QueryNode AST into a Tantivy query.
    pub fn build(&self, node: &QueryNode) -> Box<dyn Query> {
        match node {
            QueryNode::Text(text) => self.build_text_query(text),
            QueryNode::Phrase(phrase) => self.build_phrase_query(phrase),
            QueryNode::Field { field, value } => self.build_field_query(field, value),
            QueryNode::Filter(kind) => self.build_filter_query(kind),
            QueryNode::Label(label) => self.build_label_query(label),
            QueryNode::DateRange { bound, date } => self.build_date_query(bound, date),
            QueryNode::And(left, right) => {
                let lq = self.build(left);
                let rq = self.build(right);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, lq),
                    (Occur::Must, rq),
                ]))
            }
            QueryNode::Or(left, right) => {
                let lq = self.build(left);
                let rq = self.build(right);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Should, lq),
                    (Occur::Should, rq),
                ]))
            }
            QueryNode::Not(inner) => {
                let iq = self.build(inner);
                Box::new(BooleanQuery::new(vec![
                    (Occur::Must, Box::new(AllQuery)),
                    (Occur::MustNot, iq),
                ]))
            }
        }
    }

    /// Text search across multiple fields with boosts.
    fn build_text_query(&self, text: &str) -> Box<dyn Query> {
        let subqueries: Vec<(Occur, Box<dyn Query>)> = vec![
            (Occur::Should, Box::new(BoostQuery::new(
                Box::new(TermQuery::new(Term::from_field_text(self.subject, text), IndexRecordOption::WithFreqs)),
                3.0,
            ))),
            (Occur::Should, Box::new(BoostQuery::new(
                Box::new(TermQuery::new(Term::from_field_text(self.from_name, text), IndexRecordOption::WithFreqs)),
                2.0,
            ))),
            (Occur::Should, Box::new(BoostQuery::new(
                Box::new(TermQuery::new(Term::from_field_text(self.from_email, text), IndexRecordOption::WithFreqs)),
                2.0,
            ))),
            (Occur::Should, Box::new(TermQuery::new(
                Term::from_field_text(self.snippet, text), IndexRecordOption::WithFreqs,
            ))),
            (Occur::Should, Box::new(BoostQuery::new(
                Box::new(TermQuery::new(Term::from_field_text(self.body_text, text), IndexRecordOption::WithFreqs)),
                0.5,
            ))),
        ];
        Box::new(BooleanQuery::new(subqueries))
    }

    fn build_phrase_query(&self, phrase: &str) -> Box<dyn Query> {
        let terms: Vec<Term> = phrase
            .split_whitespace()
            .map(|w| Term::from_field_text(self.subject, w))
            .collect();

        if terms.len() == 1 {
            return Box::new(TermQuery::new(terms[0].clone(), IndexRecordOption::WithFreqs));
        }

        // Search phrase across subject (primary) with boost
        Box::new(BoostQuery::new(
            Box::new(PhraseQuery::new(terms)),
            3.0,
        ))
    }

    fn build_field_query(&self, field: &QueryField, value: &str) -> Box<dyn Query> {
        let tantivy_field = match field {
            QueryField::From => self.from_email,
            QueryField::To => self.to_email,
            QueryField::Subject => self.subject,
        };
        Box::new(TermQuery::new(
            Term::from_field_text(tantivy_field, value),
            IndexRecordOption::WithFreqs,
        ))
    }

    fn build_filter_query(&self, kind: &FilterKind) -> Box<dyn Query> {
        match kind {
            FilterKind::Unread => {
                // Unread = NOT has READ flag (bit 0)
                // This requires checking the flags bitfield.
                // For Tantivy, we index flags as u64 and query with range/term.
                // Simplified: we store individual boolean-like fields or use
                // a term query on a pre-computed "is_unread" field.
                // Implementation note: may need to add is_unread as a separate
                // indexed boolean field in the Tantivy schema.
                todo!("implement flag-based filtering")
            }
            FilterKind::Read => {
                todo!("implement flag-based filtering")
            }
            FilterKind::Starred => {
                todo!("implement flag-based filtering")
            }
            FilterKind::HasAttachment => {
                Box::new(TermQuery::new(
                    Term::from_field_bool(self.has_attachments, true),
                    IndexRecordOption::Basic,
                ))
            }
        }
    }

    fn build_label_query(&self, label: &str) -> Box<dyn Query> {
        Box::new(TermQuery::new(
            Term::from_field_text(self.labels, label),
            IndexRecordOption::Basic,
        ))
    }

    fn build_date_query(&self, bound: &DateBound, date: &DateValue) -> Box<dyn Query> {
        let resolved = resolve_date(date);
        match bound {
            DateBound::After => {
                let start = date_to_tantivy(&resolved);
                Box::new(RangeQuery::new_date_bounds(
                    "date".to_string(),
                    std::ops::Bound::Included(start),
                    std::ops::Bound::Unbounded,
                ))
            }
            DateBound::Before => {
                let end = date_to_tantivy(&resolved);
                Box::new(RangeQuery::new_date_bounds(
                    "date".to_string(),
                    std::ops::Bound::Unbounded,
                    std::ops::Bound::Excluded(end),
                ))
            }
            DateBound::Exact => {
                let start = date_to_tantivy(&resolved);
                let end = date_to_tantivy(&(resolved + Duration::days(1)));
                Box::new(RangeQuery::new_date_bounds(
                    "date".to_string(),
                    std::ops::Bound::Included(start),
                    std::ops::Bound::Excluded(end),
                ))
            }
        }
    }
}

fn resolve_date(date: &DateValue) -> NaiveDate {
    let today = Local::now().date_naive();
    match date {
        DateValue::Specific(d) => *d,
        DateValue::Today => today,
        DateValue::Yesterday => today - Duration::days(1),
        DateValue::ThisWeek => {
            // Start of current week (Monday)
            let weekday = today.weekday().num_days_from_monday();
            today - Duration::days(weekday as i64)
        }
        DateValue::ThisMonth => {
            NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap()
        }
    }
}

fn date_to_tantivy(date: &NaiveDate) -> TantivyDateTime {
    let dt = date.and_hms_opt(0, 0, 0).unwrap();
    TantivyDateTime::from_timestamp_secs(dt.and_utc().timestamp())
}
```

#### Saved Search CRUD

```rust
// crates/search/src/saved.rs

use mxr_core::{SavedSearch, SavedSearchId, AccountId, SortOrder};
use mxr_store::Store;
use chrono::Utc;

pub struct SavedSearchService {
    store: Store,
}

impl SavedSearchService {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    pub async fn create(
        &self,
        name: String,
        query: String,
        account_id: Option<AccountId>,
        sort: Option<SortOrder>,
    ) -> Result<SavedSearch, anyhow::Error> {
        let search = SavedSearch {
            id: SavedSearchId::new(),
            account_id,
            name,
            query,
            sort: sort.unwrap_or(SortOrder::DateDesc),
            icon: None,
            position: 0,
            created_at: Utc::now(),
        };
        self.store.insert_saved_search(&search).await?;
        Ok(search)
    }

    pub async fn list(&self) -> Result<Vec<SavedSearch>, anyhow::Error> {
        self.store.list_saved_searches().await
    }

    pub async fn delete(&self, id: &SavedSearchId) -> Result<(), anyhow::Error> {
        self.store.delete_saved_search(id).await
    }

    pub async fn get_by_name(&self, name: &str) -> Result<Option<SavedSearch>, anyhow::Error> {
        self.store.get_saved_search_by_name(name).await
    }
}
```

### What to Test

This is the most test-intensive module in Phase 1. The parser accumulates edge cases.

**Parser tests (unit tests for `parse_query`):**

1. Simple text: `"invoice"` -> `Text("invoice")`
2. Exact phrase: `'"deployment plan"'` -> `Phrase("deployment plan")`
3. Field query: `"from:alice@example.com"` -> `Field { From, "alice@example.com" }`
4. Subject query: `"subject:quarterly report"` -> `Field { Subject, "quarterly" }` (note: multi-word field values need quotes)
5. Boolean AND: `"from:alice AND subject:invoice"` -> `And(Field{From}, Field{Subject})`
6. Boolean OR: `"budget OR forecast"` -> `Or(Text, Text)`
7. NOT: `"NOT spam"` -> `Not(Text("spam"))`
8. Negation prefix: `"-subject:spam"` -> `Not(Field{Subject, "spam"})`
9. Label: `"label:work"` -> `Label("work")`
10. Filters: `"is:unread"`, `"is:starred"`, `"is:read"`, `"has:attachment"`
11. Date specific: `"after:2026-01-01"` -> `DateRange { After, Specific(2026-01-01) }`
12. Date relative: `"date:today"`, `"date:yesterday"`, `"date:this-week"`, `"date:this-month"`
13. Combinations: `"from:alice subject:invoice after:2026-01-01 is:unread"` -> nested AND tree
14. Implicit AND: `"from:alice is:unread"` -> `And(Field{From}, Filter{Unread})`
15. Parenthesized: `"(from:alice OR from:bob) AND is:unread"`
16. Empty query: returns error
17. Unmatched quotes: graceful handling
18. Unknown field: treated as text

**Query builder tests (integration with Tantivy):**

1. Build a text query and execute against a small test index, verify results.
2. Build a phrase query, verify exact phrase matching works.
3. Build a date range query, verify messages in/out of range.
4. Build a boolean AND query, verify intersection of results.
5. Build a label query against indexed label field.
6. Full round-trip: parse -> build -> search against test index with 100 fixture documents.

**Saved search tests:**

1. Create, list, delete cycle.
2. Get by name (exact match).
3. Duplicate names are allowed (different IDs).

---

## Step 5: TUI Enhancements

### Crate/Module

Modify existing `crates/tui/`

### Files to Create/Modify

```
crates/tui/src/layout.rs           # Three-pane layout manager
crates/tui/src/views/sidebar.rs    # Sidebar: labels + saved searches
crates/tui/src/views/message_list.rs  # Message list pane
crates/tui/src/views/message_view.rs  # Message reading pane
crates/tui/src/views/thread.rs     # Thread view (stacked messages)
crates/tui/src/views/search.rs     # Search input bar
crates/tui/src/views/palette.rs    # Command palette overlay
crates/tui/src/views/status_bar.rs # Status bar (enhanced for per-account sync, A006)
crates/tui/src/input/keymap.rs     # Multi-key state machine for g-prefix navigation (A005)
crates/tui/src/state.rs            # Application state
crates/tui/Cargo.toml              # Add nucleo dependency
```

### External Dependencies

```toml
# Add to crates/tui/Cargo.toml
[dependencies]
nucleo = "0.5"      # Fuzzy matching (from Helix editor)
unicode-width = "0.2"  # Correct column width for Unicode
```

Add to workspace `Cargo.toml`:
```toml
[workspace.dependencies]
nucleo = "0.5"
unicode-width = "0.2"
```

### Key Code Patterns

#### Layout Manager

```rust
// crates/tui/src/layout.rs

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    /// Sidebar + message list (no message selected)
    TwoPane,
    /// Sidebar + message list + message view
    ThreePane,
    /// Message view full width
    FullScreen,
}

pub struct LayoutManager {
    pub mode: LayoutMode,
}

impl LayoutManager {
    pub fn compute(&self, area: Rect) -> LayoutRegions {
        // Reserve 1 row at bottom for status bar
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),      // main area
                Constraint::Length(1),   // status bar
            ])
            .split(area);

        let main = chunks[0];
        let status_bar = chunks[1];

        match self.mode {
            LayoutMode::TwoPane => {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(20),   // sidebar
                        Constraint::Min(40),      // message list
                    ])
                    .split(main);
                LayoutRegions {
                    sidebar: Some(cols[0]),
                    message_list: Some(cols[1]),
                    message_view: None,
                    status_bar,
                }
            }
            LayoutMode::ThreePane => {
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(20),   // sidebar
                        Constraint::Percentage(35), // message list
                        Constraint::Min(30),     // message view
                    ])
                    .split(main);
                LayoutRegions {
                    sidebar: Some(cols[0]),
                    message_list: Some(cols[1]),
                    message_view: Some(cols[2]),
                    status_bar,
                }
            }
            LayoutMode::FullScreen => {
                LayoutRegions {
                    sidebar: None,
                    message_list: None,
                    message_view: Some(main),
                    status_bar,
                }
            }
        }
    }
}

pub struct LayoutRegions {
    pub sidebar: Option<Rect>,
    pub message_list: Option<Rect>,
    pub message_view: Option<Rect>,
    pub status_bar: Rect,
}
```

#### Command Palette

```rust
// crates/tui/src/views/palette.rs

use nucleo::Matcher;
use mxr_core::Action;

pub struct CommandPalette {
    pub visible: bool,
    pub input: String,
    pub commands: Vec<PaletteCommand>,
    pub filtered: Vec<usize>,  // indices into commands
    pub selected: usize,
    matcher: Matcher,
}

pub struct PaletteCommand {
    pub label: String,
    pub shortcut: Option<String>,
    pub action: Action,
    pub category: PaletteCategory,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaletteCategory {
    Actions,
    Navigation,
    SavedSearches,
    System,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            commands: Self::default_commands(),
            filtered: Vec::new(),
            selected: 0,
            matcher: Matcher::new(nucleo::Config::DEFAULT),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.input.clear();
            self.selected = 0;
            self.update_filtered();
        }
    }

    pub fn on_input(&mut self, c: char) {
        self.input.push(c);
        self.selected = 0;
        self.update_filtered();
    }

    pub fn on_backspace(&mut self) {
        self.input.pop();
        self.selected = 0;
        self.update_filtered();
    }

    pub fn select_next(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn confirm(&mut self) -> Option<Action> {
        let idx = self.filtered.get(self.selected)?;
        let action = self.commands[*idx].action.clone();
        self.visible = false;
        Some(action)
    }

    fn update_filtered(&mut self) {
        if self.input.is_empty() {
            self.filtered = (0..self.commands.len()).collect();
            return;
        }

        // Use nucleo for fuzzy matching
        let mut scored: Vec<(usize, u32)> = self.commands.iter()
            .enumerate()
            .filter_map(|(i, cmd)| {
                let mut buf = Vec::new();
                let haystack: Vec<char> = cmd.label.chars().collect();
                let needle: Vec<char> = self.input.chars().collect();
                // Simplified: use contains check as fallback
                // Full nucleo integration uses Nucleo<T> worker
                if cmd.label.to_lowercase().contains(&self.input.to_lowercase()) {
                    Some((i, 100))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        self.filtered = scored.into_iter().map(|(i, _)| i).collect();
    }

    fn default_commands() -> Vec<PaletteCommand> {
        vec![
            PaletteCommand {
                label: "Search...".into(),
                shortcut: Some("/".into()),       // A005: / for search
                action: Action::OpenSearch,
                category: PaletteCategory::Navigation,
            },
            PaletteCommand {
                label: "Sync now".into(),
                shortcut: None,
                action: Action::SyncNow,
                category: PaletteCategory::System,
            },
            // A005: g-prefix go-to navigation entries
            PaletteCommand {
                label: "Go to Inbox".into(),
                shortcut: Some("gi".into()),
                action: Action::GoToInbox,
                category: PaletteCategory::Navigation,
            },
            PaletteCommand {
                label: "Go to Starred".into(),
                shortcut: Some("gs".into()),
                action: Action::GoToStarred,
                category: PaletteCategory::Navigation,
            },
            PaletteCommand {
                label: "Go to Sent".into(),
                shortcut: Some("gt".into()),
                action: Action::GoToSent,
                category: PaletteCategory::Navigation,
            },
            PaletteCommand {
                label: "Go to Drafts".into(),
                shortcut: Some("gd".into()),
                action: Action::GoToDrafts,
                category: PaletteCategory::Navigation,
            },
            // ... more commands populated at runtime from saved searches, labels, etc.
        ]
    }

    /// Add saved searches to the palette.
    pub fn set_saved_searches(&mut self, searches: Vec<(String, String)>) {
        // Remove old saved search entries
        self.commands.retain(|c| c.category != PaletteCategory::SavedSearches);

        for (name, query) in searches {
            self.commands.push(PaletteCommand {
                label: format!("Saved: {name}"),
                shortcut: None,
                action: Action::ExecuteSavedSearch(mxr_core::SavedSearchId::new()), // TODO: pass real ID
                category: PaletteCategory::SavedSearches,
            });
        }
    }
}
```

#### Search Input

```rust
// crates/tui/src/views/search.rs

pub struct SearchInput {
    pub active: bool,
    pub query: String,
    pub cursor_pos: usize,
}

impl SearchInput {
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.cursor_pos = 0;
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn on_char(&mut self, c: char) {
        self.query.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn on_backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.query.remove(self.cursor_pos);
        }
    }

    pub fn submit(&self) -> Option<String> {
        if self.query.is_empty() {
            None
        } else {
            Some(self.query.clone())
        }
    }
}
```

#### Multi-Key State Machine (A005)

```rust
// crates/tui/src/input/keymap.rs

use crossterm::event::KeyCode;

/// Handles multi-key sequences like `gg`, `gi`, `gs`, `gt`, `gd`, `ga`, `gl`.
/// Per A005: vim-native first, Gmail go-to navigation with `g` prefix.
pub struct KeymapState {
    pending: Option<char>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeymapAction {
    /// No complete sequence yet, key consumed
    Pending,
    /// Multi-key sequence resolved to an action
    Action(Action),
    /// Key was not part of a sequence, pass through
    Passthrough(KeyCode),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // Navigation (vim)
    JumpToTop,        // gg
    OpenSearch,       // /
    NextSearchResult, // n
    PrevSearchResult, // N

    // Go-to navigation (Gmail g-prefix, A005)
    GoToInbox,        // gi
    GoToStarred,      // gs
    GoToSent,         // gt
    GoToDrafts,       // gd
    GoToAllMail,      // ga
    GoToLabel,        // gl (opens label picker)

    // Command palette (A005)
    OpenCommandPalette, // Ctrl-p
}

impl KeymapState {
    pub fn new() -> Self {
        Self { pending: None }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> KeymapAction {
        if let Some(prev) = self.pending.take() {
            // We have a pending 'g' prefix
            if prev == 'g' {
                match key {
                    KeyCode::Char('g') => return KeymapAction::Action(Action::JumpToTop),
                    KeyCode::Char('i') => return KeymapAction::Action(Action::GoToInbox),
                    KeyCode::Char('s') => return KeymapAction::Action(Action::GoToStarred),
                    KeyCode::Char('t') => return KeymapAction::Action(Action::GoToSent),
                    KeyCode::Char('d') => return KeymapAction::Action(Action::GoToDrafts),
                    KeyCode::Char('a') => return KeymapAction::Action(Action::GoToAllMail),
                    KeyCode::Char('l') => return KeymapAction::Action(Action::GoToLabel),
                    _ => return KeymapAction::Passthrough(key),
                }
            }
        }

        match key {
            KeyCode::Char('g') => {
                self.pending = Some('g');
                KeymapAction::Pending
            }
            KeyCode::Char('/') => KeymapAction::Action(Action::OpenSearch),
            KeyCode::Char('n') => KeymapAction::Action(Action::NextSearchResult),
            KeyCode::Char('N') => KeymapAction::Action(Action::PrevSearchResult),
            _ => KeymapAction::Passthrough(key),
        }
    }

    /// Handle Ctrl-key combinations separately.
    pub fn handle_ctrl_key(&mut self, key: char) -> KeymapAction {
        match key {
            'p' => KeymapAction::Action(Action::OpenCommandPalette), // Ctrl-p (A005)
            _ => KeymapAction::Passthrough(KeyCode::Char(key)),
        }
    }

    pub fn reset(&mut self) {
        self.pending = None;
    }
}
```

#### Status Bar

```rust
// crates/tui/src/views/status_bar.rs

use chrono::{DateTime, Utc};
use ratatui::{
    widgets::{Paragraph, Widget},
    style::{Style, Color},
    text::{Line, Span},
    layout::Rect,
    Frame,
};

pub struct StatusBarState {
    pub context: String,           // e.g., "[INBOX]", "[search: invoice]"
    pub unread_count: u32,
    /// Per-account sync status (A006: enhanced status bar showing per-account sync status)
    pub account_sync: Vec<AccountSyncStatus>,
}

/// Per-account sync status for the status bar (A006)
pub struct AccountSyncStatus {
    pub name: String,
    pub status: SyncStatus,
}

pub enum SyncStatus {
    Idle { last_sync: Option<DateTime<Utc>> },
    /// Syncing with optional progress (current/total) (A006)
    Syncing { current: Option<u32>, total: Option<u32> },
    Error(String),
}

impl StatusBarState {
    pub fn render(&self, f: &mut Frame, area: Rect) {
        // A006: Enhanced status bar with per-account sync status.
        // Format examples:
        //   Normal:  [INBOX] 12 unread | personal: synced 2m ago | work: synced 2m ago
        //   Syncing: [INBOX] 12 unread | personal: syncing (47/200)...
        //   Error:   [INBOX] 12 unread | work: auth expired

        let mut spans = vec![
            Span::styled(&self.context, Style::default().fg(Color::Cyan)),
            Span::raw(format!(" {} unread", self.unread_count)),
        ];

        for acct in &self.account_sync {
            spans.push(Span::raw(" | "));
            let sync_text = match &acct.status {
                SyncStatus::Idle { last_sync: Some(t) } => {
                    let ago = Utc::now().signed_duration_since(*t);
                    let mins = ago.num_minutes();
                    if mins < 1 {
                        format!("{}: synced just now", acct.name)
                    } else {
                        format!("{}: synced {mins}m ago", acct.name)
                    }
                }
                SyncStatus::Idle { last_sync: None } => {
                    format!("{}: not synced", acct.name)
                }
                SyncStatus::Syncing { current: Some(c), total: Some(t) } => {
                    format!("{}: syncing ({c}/{t})...", acct.name)
                }
                SyncStatus::Syncing { .. } => {
                    format!("{}: syncing...", acct.name)
                }
                SyncStatus::Error(e) => {
                    format!("{}: {e}", acct.name)
                }
            };

            let style = match &acct.status {
                SyncStatus::Error(_) => Style::default().fg(Color::Yellow),
                SyncStatus::Syncing { .. } => Style::default().fg(Color::Green),
                _ => Style::default(),
            };
            spans.push(Span::styled(sync_text, style));
        }

        let line = Line::from(spans);
        f.render_widget(Paragraph::new(line), area);
    }
}
```

### What to Test

1. **Layout**: Given terminal size, verify pane dimensions are sane for each LayoutMode.
2. **Command palette**: Toggle open/close with Ctrl-p (A005). Type characters, verify filtering. Select up/down. Confirm returns correct action. Verify go-to navigation entries (gi, gs, gt, gd) appear with correct shortcuts.
3. **Search input**: `/` activates search (A005). Type query, backspace, submit. Verify cursor position. `n` and `N` navigate results (A005).
4. **Status bar (A006)**: Render with per-account sync status. Verify formats: "personal: synced 2m ago", "work: syncing (47/200)...", "work: auth expired". Verify error states show in yellow.
5. **Sidebar**: Labels render with correct unread counts. Saved searches appear below labels.
6. **Multi-key state machine (A005)**: `g` then `g` = jump to top. `g` then `i` = go to inbox. `g` then `s` = go to starred. `g` then `t` = go to sent. `g` then `d` = go to drafts. `g` then unknown key = passthrough. Timeout/reset clears pending state.
7. **Integration**: Full TUI with fake provider data - navigate, open message, see body in message view pane.

---

## Step 6: CLI Subcommands

### Crate/Module

Modify `crates/daemon/` (the `mxr` binary)

### Files to Create/Modify

```
crates/daemon/src/cli.rs           # Clap CLI definition (update existing)
crates/daemon/src/commands/mod.rs
crates/daemon/src/commands/search.rs
crates/daemon/src/commands/sync.rs
crates/daemon/src/commands/accounts.rs
crates/daemon/src/commands/doctor.rs
crates/daemon/src/commands/labels.rs
crates/daemon/src/commands/config.rs
crates/daemon/src/commands/cat.rs      # Print message body (A004)
crates/daemon/src/commands/thread.rs   # Print full thread (A004)
crates/daemon/src/commands/headers.rs  # Print raw headers (A004)
crates/daemon/src/commands/count.rs    # Count matching messages (A004)
crates/daemon/src/commands/saved.rs    # Saved search CRUD (A004)
crates/daemon/src/commands/status.rs   # Daemon/sync status overview (A006)
crates/daemon/src/commands/logs.rs     # Recent daemon logs (A006)
crates/daemon/src/output.rs           # Auto-format detection (A004)
```

### Key Code Patterns

```rust
// crates/daemon/src/cli.rs

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "mxr", about = "Terminal email client")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Override sync interval (seconds)
    #[arg(long)]
    pub sync_interval: Option<u64>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start daemon explicitly
    Daemon {
        /// Run in foreground
        #[arg(long)]
        foreground: bool,
    },

    /// Search messages
    Search {
        /// Search query
        query: Option<String>,

        /// Run a saved search by name
        #[arg(long)]
        saved: Option<String>,

        /// Save a search: --save "name" "query"
        #[arg(long)]
        save: Option<String>,

        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,

        /// Max results
        #[arg(long)]
        limit: Option<usize>,

        /// Sort order
        #[arg(long)]
        sort: Option<String>,

        /// Filter by account
        #[arg(long)]
        account: Option<String>,
    },

    /// Count matching messages (A004)
    Count {
        /// Search query
        query: String,

        /// Filter by account
        #[arg(long)]
        account: Option<String>,
    },

    /// Print message body (A004)
    Cat {
        /// Message ID
        message_id: String,

        /// Print body without any processing
        #[arg(long)]
        raw: bool,

        /// Print original HTML body
        #[arg(long)]
        html: bool,

        /// Print full headers + body
        #[arg(long)]
        headers: bool,

        /// Print everything
        #[arg(long)]
        all: bool,

        /// Output as structured JSON
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },

    /// Print full thread chronologically (A004)
    Thread {
        /// Thread ID
        thread_id: String,

        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Print raw email headers (A004)
    Headers {
        /// Message ID
        message_id: String,
    },

    /// Manage saved searches (A004)
    Saved {
        #[command(subcommand)]
        action: Option<SavedAction>,

        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Trigger sync
    Sync {
        /// Sync specific account
        #[arg(long)]
        account: Option<String>,

        /// Show sync status per account (A006)
        #[arg(long)]
        status: bool,

        /// Show recent sync log (A006)
        #[arg(long)]
        history: bool,
    },

    /// Single-command overview: daemon health, sync status, unread counts (A006)
    Status {
        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,

        /// Live dashboard
        #[arg(long)]
        watch: bool,
    },

    /// View daemon logs (A006)
    Logs {
        /// Don't follow, just print recent logs
        #[arg(long)]
        no_follow: bool,

        /// Filter by level
        #[arg(long)]
        level: Option<String>,

        /// Time filter (e.g. "1h", "30m")
        #[arg(long)]
        since: Option<String>,

        /// Text filter
        #[arg(long, name = "PATTERN")]
        grep: Option<String>,
    },

    /// Manage accounts
    Accounts {
        #[command(subcommand)]
        action: Option<AccountsAction>,

        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Run diagnostics
    Doctor {
        /// Rebuild Tantivy search index
        #[arg(long)]
        reindex: bool,
    },

    /// List labels with counts
    Labels,

    /// Show resolved configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },

    /// Print version info (binary version, build info, data dir)
    Version,

    /// Generate shell completions
    Completions {
        /// Shell type
        shell: String, // bash, zsh, fish
    },
}

#[derive(Subcommand)]
pub enum AccountsAction {
    /// Add a new account
    Add {
        /// Provider type
        provider: String, // "gmail" or "smtp"
    },
    /// Show account details (A004)
    Show {
        /// Account name
        name: String,
    },
    /// Re-authenticate (refresh OAuth2 tokens) (A004)
    Reauth {
        /// Account name
        name: String,
    },
    /// Test connectivity (sync + send) (A004)
    Test {
        /// Account name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum SavedAction {
    /// List all saved searches
    List,
    /// Create a saved search
    Add {
        /// Search name
        name: String,
        /// Search query
        query: String,
    },
    /// Delete a saved search
    Delete {
        /// Search name
        name: String,
    },
    /// Execute a saved search
    Run {
        /// Search name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Print config file path
    Path,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Ids,
}
```

#### Auto-format Detection (A004)

```rust
// crates/daemon/src/output.rs

use crate::cli::OutputFormat;

/// Resolve output format: explicit flag > auto-detect from TTY.
/// TTY → table, piped → json. Per A004 auto-format detection.
pub fn resolve_format(explicit: Option<OutputFormat>) -> OutputFormat {
    match explicit {
        Some(f) => f,
        None => {
            if std::io::stdout().is_terminal() {
                OutputFormat::Table
            } else {
                OutputFormat::Json
            }
        }
    }
}
```

```rust
// crates/daemon/src/commands/search.rs

use crate::cli::OutputFormat;
use mxr_core::Envelope;

pub async fn handle_search(
    query: &str,
    format: OutputFormat,
    // ... daemon connection or direct store/search access
) -> anyhow::Result<()> {
    // 1. Parse query
    let ast = mxr_search::parse_query(query)?;

    // 2. Build tantivy query
    let tantivy_query = query_builder.build(&ast);

    // 3. Execute search
    let results: Vec<Envelope> = search_index.search(&tantivy_query, max_results)?;

    // 4. Output
    match format {
        OutputFormat::Table => print_table(&results),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&results)?;
            println!("{json}");
        }
        OutputFormat::Csv => print_csv(&results),
    }

    Ok(())
}

fn print_table(results: &[Envelope]) {
    // Header
    println!("{:<20} {:<30} {:<40} {:<12}",
        "FROM", "SUBJECT", "SNIPPET", "DATE");
    println!("{}", "-".repeat(102));

    for env in results {
        let from = env.from.email.chars().take(20).collect::<String>();
        let subject = env.subject.chars().take(30).collect::<String>();
        let snippet = env.snippet.chars().take(40).collect::<String>();
        let date = env.date.format("%Y-%m-%d");
        let unread = if env.flags.contains(mxr_core::MessageFlags::READ) { " " } else { "*" };
        println!("{unread}{:<19} {:<30} {:<40} {:<12}",
            from, subject, snippet, date);
    }

    println!("\n{} results", results.len());
}

fn print_csv(results: &[Envelope]) {
    println!("from,subject,date,unread");
    for env in results {
        let unread = !env.flags.contains(mxr_core::MessageFlags::READ);
        println!("{},{},{},{}",
            env.from.email,
            env.subject.replace(',', ";"),
            env.date.format("%Y-%m-%d"),
            unread);
    }
}
```

```rust
// crates/daemon/src/commands/doctor.rs

pub async fn handle_doctor(reindex: bool) -> anyhow::Result<()> {
    println!("mxr doctor");
    println!("==========\n");

    // 1. Config validation
    print!("Config file... ");
    match mxr_config::load_config() {
        Ok(config) => {
            println!("OK ({})", mxr_config::config_file_path().display());
            println!("  Accounts: {}", config.accounts.len());
            println!("  Sync interval: {}s", config.general.sync_interval);
        }
        Err(e) => println!("ERROR: {e}"),
    }
    println!();

    // 2. Database status
    print!("Database... ");
    let db_path = mxr_config::data_dir().join("mxr.db");
    if db_path.exists() {
        let metadata = std::fs::metadata(&db_path)?;
        println!("OK ({}, {:.1} MB)",
            db_path.display(),
            metadata.len() as f64 / 1_048_576.0);
    } else {
        println!("NOT FOUND ({})", db_path.display());
    }
    println!();

    // 3. Search index status
    print!("Search index... ");
    let index_path = mxr_config::data_dir().join("search_index");
    if index_path.exists() {
        println!("OK ({})", index_path.display());
    } else {
        println!("NOT FOUND");
    }
    println!();

    // 4. Auth status per account
    let config = mxr_config::load_config().unwrap_or_default();
    for (name, account) in &config.accounts {
        print!("Account '{name}'... ");
        if let Some(sync) = &account.sync {
            match sync {
                mxr_config::SyncProviderConfig::Gmail { token_ref, .. } => {
                    let token_path = mxr_config::data_dir()
                        .join(format!("tokens/{token_ref}.json"));
                    if token_path.exists() {
                        println!("Gmail, tokens present");
                    } else {
                        println!("Gmail, NO TOKENS — run `mxr accounts add gmail`");
                    }
                }
            }
        } else {
            println!("No sync provider");
        }
    }
    println!();

    // 5. Last sync times
    // TODO: query sync_log table

    // 6. Reindex if requested
    if reindex {
        println!("Rebuilding search index...");
        // TODO: drop and rebuild Tantivy index from SQLite
        println!("Done.");
    }

    Ok(())
}
```

#### Cat Command — Print Message Body (A004)

```rust
// crates/daemon/src/commands/cat.rs

use crate::output::resolve_format;
use crate::cli::OutputFormat;

pub async fn handle_cat(
    message_id: &str,
    raw: bool,
    html: bool,
    headers: bool,
    all: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);

    // 1. Fetch envelope + body from store (via daemon IPC)
    // 2. If --format json: serialize full message as JSON
    // 3. If --raw: print text_plain body as-is
    // 4. If --html: print text_html body
    // 5. If --headers: print all headers then body
    // 6. If --all: print headers + all body parts
    // 7. Default (no flags): print text_plain body
    //    Note: reader mode rendering is Phase 2. For now, just print the body text.

    match format {
        OutputFormat::Json => {
            // Full message as structured JSON
            let json = serde_json::to_string_pretty(&message)?;
            println!("{json}");
        }
        _ => {
            if headers || all {
                // Print raw headers
                for (name, value) in &message.headers {
                    println!("{name}: {value}");
                }
                println!();
            }

            if html {
                if let Some(html_body) = &body.text_html {
                    println!("{html_body}");
                }
            } else if raw || !html {
                // Print text/plain body (reader mode is Phase 2, just print body for now)
                if let Some(text) = &body.text_plain {
                    println!("{text}");
                } else if let Some(html_body) = &body.text_html {
                    // Fallback: print HTML if no plain text
                    println!("{html_body}");
                }
            }
        }
    }

    Ok(())
}
```

#### Thread Command — Print Full Thread (A004)

```rust
// crates/daemon/src/commands/thread.rs

pub async fn handle_thread(
    thread_id: &str,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);

    // 1. Fetch all messages in thread from store, ordered chronologically
    // 2. For each message, fetch body (lazy hydration)
    // 3. Output based on format

    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&thread_messages)?;
            println!("{json}");
        }
        _ => {
            for (i, msg) in thread_messages.iter().enumerate() {
                if i > 0 { println!("\n{}", "─".repeat(60)); }
                println!("From: {}", msg.from);
                println!("Date: {}", msg.date);
                println!("Subject: {}", msg.subject);
                println!();
                // Print body text (reader mode is Phase 2)
                if let Some(text) = &msg.body.text_plain {
                    println!("{text}");
                }
            }
        }
    }

    Ok(())
}
```

#### Headers Command — Print Raw Headers (A004)

```rust
// crates/daemon/src/commands/headers.rs

pub async fn handle_headers(message_id: &str) -> anyhow::Result<()> {
    // Fetch full message with headers from store
    // Print each header as "Name: Value"
    for (name, value) in &message.headers {
        println!("{name}: {value}");
    }
    Ok(())
}
```

#### Count Command — Count Matching Messages (A004)

```rust
// crates/daemon/src/commands/count.rs

pub async fn handle_count(query: &str, account: Option<&str>) -> anyhow::Result<()> {
    // 1. Parse query
    let ast = mxr_search::parse_query(query)?;
    // 2. Build tantivy query
    let tantivy_query = query_builder.build(&ast);
    // 3. Execute count (not full search — just count)
    let count = search_index.count(&tantivy_query)?;
    // 4. Print just the number (scriptable)
    println!("{count}");
    Ok(())
}
```

#### Saved Searches Subcommands (A004)

```rust
// crates/daemon/src/commands/saved.rs

use crate::cli::{SavedAction, OutputFormat};
use crate::output::resolve_format;

pub async fn handle_saved(
    action: Option<SavedAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);

    match action {
        None | Some(SavedAction::List) => {
            let searches = store.list_saved_searches().await?;
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&searches)?);
                }
                _ => {
                    println!("{:<20} {}", "NAME", "QUERY");
                    println!("{}", "-".repeat(60));
                    for s in &searches {
                        println!("{:<20} {}", s.name, s.query);
                    }
                }
            }
        }
        Some(SavedAction::Add { name, query }) => {
            store.create_saved_search(&name, &query).await?;
            println!("Saved search '{name}' created.");
        }
        Some(SavedAction::Delete { name }) => {
            store.delete_saved_search(&name).await?;
            println!("Saved search '{name}' deleted.");
        }
        Some(SavedAction::Run { name }) => {
            let search = store.get_saved_search(&name).await?;
            // Delegate to search handler with the saved query
            handle_search(&search.query, format).await?;
        }
    }

    Ok(())
}
```

#### Status Command — Daemon Health Overview (A006)

```rust
// crates/daemon/src/commands/status.rs

use crate::cli::OutputFormat;
use crate::output::resolve_format;

pub async fn handle_status(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let format = resolve_format(format);

    // 1. Check daemon status (running/stopped, uptime, connected clients)
    // 2. Per-account sync status from sync_log table
    // 3. Unread counts per account

    let status = StatusReport {
        daemon_running: check_daemon_running(),
        uptime: get_daemon_uptime(),
        accounts: vec![],
    };

    // Query sync_log for each account
    for (name, _account) in &config.accounts {
        let last_sync = store.get_last_sync_log(name).await?;
        let unread = store.count_unread(name).await?;
        status.accounts.push(AccountStatus {
            name: name.clone(),
            last_sync_at: last_sync.map(|l| l.completed_at),
            last_sync_messages: last_sync.map(|l| l.messages_synced),
            last_sync_error: last_sync.and_then(|l| l.error),
            unread_count: unread,
        });
    }

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        _ => {
            println!("mxr status");
            println!("==========\n");
            println!("Daemon: {}", if status.daemon_running { "running" } else { "stopped" });
            if let Some(uptime) = status.uptime {
                println!("Uptime: {uptime}");
            }
            println!();
            for acct in &status.accounts {
                let sync_info = match &acct.last_sync_at {
                    Some(t) => format!("synced {}", format_relative_time(*t)),
                    None => "never synced".to_string(),
                };
                let error_info = acct.last_sync_error.as_ref()
                    .map(|e| format!(" (error: {e})"))
                    .unwrap_or_default();
                println!("  {}: {} unread | {}{}", acct.name, acct.unread_count, sync_info, error_info);
            }
        }
    }

    Ok(())
}
```

#### Sync --status and --history (A006)

```rust
// In crates/daemon/src/commands/sync.rs — extend existing handler

pub async fn handle_sync(
    account: Option<&str>,
    status: bool,
    history: bool,
) -> anyhow::Result<()> {
    if status {
        // Read sync_log table, show per-account sync status
        let logs = store.get_latest_sync_per_account().await?;
        for log in &logs {
            let status_str = if log.error.is_some() {
                format!("error: {}", log.error.as_ref().unwrap())
            } else {
                format!("ok ({} messages)", log.messages_synced)
            };
            println!("{}: last sync {} — {}", log.account_name,
                format_relative_time(log.completed_at), status_str);
        }
        return Ok(());
    }

    if history {
        // Show recent sync log entries
        let logs = store.get_recent_sync_logs(20).await?;
        println!("{:<20} {:<20} {:<10} {}", "ACCOUNT", "TIME", "MESSAGES", "STATUS");
        println!("{}", "-".repeat(70));
        for log in &logs {
            let status_str = log.error.as_deref().unwrap_or("ok");
            println!("{:<20} {:<20} {:<10} {}",
                log.account_name,
                log.completed_at.format("%Y-%m-%d %H:%M"),
                log.messages_synced,
                status_str);
        }
        return Ok(());
    }

    // ... existing sync logic
    Ok(())
}
```

#### Logs Command (A006)

```rust
// crates/daemon/src/commands/logs.rs

pub async fn handle_logs(
    no_follow: bool,
    level: Option<&str>,
    since: Option<&str>,
    grep: Option<&str>,
) -> anyhow::Result<()> {
    if no_follow {
        // Read recent logs from log file or event_log table
        let log_path = mxr_config::data_dir().join("logs/mxr.log");
        let contents = std::fs::read_to_string(&log_path)?;
        let lines: Vec<&str> = contents.lines().collect();

        // Apply filters
        let filtered: Vec<&&str> = lines.iter()
            .rev()
            .take(100)  // last 100 lines
            .filter(|line| {
                if let Some(level) = level {
                    if !line.to_lowercase().contains(level) { return false; }
                }
                if let Some(grep) = grep {
                    if !line.contains(grep) { return false; }
                }
                true
            })
            .collect();

        for line in filtered.iter().rev() {
            println!("{line}");
        }
    } else {
        // Default: tail -f style (connect to daemon log stream via IPC)
        // Follow daemon event stream
        todo!("live log following via daemon IPC subscription")
    }

    Ok(())
}
```

#### Accounts Show / Reauth / Test (A004)

```rust
// In crates/daemon/src/commands/accounts.rs — extend existing handler

pub async fn handle_accounts_show(name: &str) -> anyhow::Result<()> {
    let config = mxr_config::load_config()?;
    let account = config.accounts.get(name)
        .ok_or_else(|| anyhow::anyhow!("account '{name}' not found"))?;

    println!("Account: {name}");
    println!("  Email: {}", account.email);
    if let Some(sync) = &account.sync {
        match sync {
            mxr_config::SyncProviderConfig::Gmail { .. } => {
                println!("  Sync: Gmail");
            }
        }
    }
    if let Some(send) = &account.send {
        println!("  Send: {:?}", send);
    }

    // Show sync status from store
    if let Ok(last_sync) = store.get_last_sync_log(name).await {
        if let Some(log) = last_sync {
            println!("  Last sync: {} ({} messages)",
                format_relative_time(log.completed_at), log.messages_synced);
        }
    }

    Ok(())
}

pub async fn handle_accounts_reauth(name: &str) -> anyhow::Result<()> {
    let config = mxr_config::load_config()?;
    let account = config.accounts.get(name)
        .ok_or_else(|| anyhow::anyhow!("account '{name}' not found"))?;

    match &account.sync {
        Some(mxr_config::SyncProviderConfig::Gmail { client_id, client_secret, token_ref }) => {
            println!("Re-authenticating Gmail account '{name}'...");
            let mut auth = GmailAuth::new(
                client_id.clone(),
                client_secret.clone().unwrap_or_default(),
                token_ref.clone(),
            );
            auth.interactive_auth().await?;
            println!("Re-authentication successful!");
        }
        _ => anyhow::bail!("account '{name}' has no sync provider to re-authenticate"),
    }

    Ok(())
}

pub async fn handle_accounts_test(name: &str) -> anyhow::Result<()> {
    let config = mxr_config::load_config()?;
    let account = config.accounts.get(name)
        .ok_or_else(|| anyhow::anyhow!("account '{name}' not found"))?;

    println!("Testing account '{name}'...");

    // Test sync connectivity
    print!("  Sync... ");
    match &account.sync {
        Some(mxr_config::SyncProviderConfig::Gmail { .. }) => {
            // Try to list labels (lightweight API call)
            match provider.sync_labels().await {
                Ok(labels) => println!("OK ({} labels)", labels.len()),
                Err(e) => println!("FAILED: {e}"),
            }
        }
        None => println!("not configured"),
    }

    // Test send connectivity
    print!("  Send... ");
    match &account.send {
        Some(_) => println!("OK (connection test)"),
        None => println!("not configured"),
    }

    Ok(())
}
```

### What to Test

1. **CLI parsing**: Verify clap parses all subcommands and flags correctly, including new commands (`cat`, `thread`, `headers`, `count`, `saved`, `status`, `logs`).
2. **Search output formats**: Table, JSON, CSV output for a set of fixture envelopes.
3. **Search --save**: Creates a saved search, --saved retrieves and executes it.
4. **Doctor**: Runs without crashing in various states (no config, no db, no index).
5. **Labels**: Lists labels from store with correct formatting.
6. **Accounts list**: Shows configured accounts.
7. **Auto-format detection (A004)**: When stdout is a TTY, default format is table. When piped, default is JSON. Explicit `--format` flag overrides both.
8. **Cat command (A004)**: Print body text for a message. `--raw` prints without processing. `--format json` outputs structured JSON. `--headers` includes headers.
9. **Thread command (A004)**: Print all messages in a thread chronologically.
10. **Headers command (A004)**: Print raw headers for a message.
11. **Count command (A004)**: Returns just a number for a search query.
12. **Saved subcommands (A004)**: `saved add`, `saved delete`, `saved run`, `saved` (list) all work.
13. **Accounts show/reauth/test (A004)**: Show account details, re-auth flow, connectivity test.
14. **Status command (A006)**: Shows daemon health, per-account sync status, unread counts.
15. **Sync --status (A006)**: Shows per-account sync status from sync_log.
16. **Sync --history (A006)**: Shows recent sync log entries.
17. **Logs --no-follow (A006)**: Prints recent log lines with optional filters.
18. **Config path (A004)**: `mxr config path` prints the config file path.
19. **Version (A004)**: `mxr version` prints version, build info, data dir.

---

## Step 7: Compile-Time SQL Checking

### Crate/Module

Affects `crates/store/` and CI

### Files to Create/Modify

```
.sqlx/                             # Generated query metadata (committed to repo)
crates/store/src/*.rs              # Convert runtime sqlx queries to compile-time checked
.github/workflows/ci.yml           # Add sqlx prepare check
.env                               # DATABASE_URL for sqlx (development only)
```

### Setup Process

1. Install sqlx CLI: `cargo install sqlx-cli --no-default-features --features sqlite`

2. Create a development database and run migrations:
   ```bash
   export DATABASE_URL="sqlite:dev.db"
   cargo sqlx database create
   cargo sqlx migrate run --source crates/store/migrations
   ```

3. Convert runtime queries to compile-time checked queries. Example conversion:

   **Before (Phase 0 - runtime):**
   ```rust
   sqlx::query("INSERT INTO messages (id, account_id, ...) VALUES (?, ?, ...)")
       .bind(&id)
       .bind(&account_id)
       .execute(&pool)
       .await?;
   ```

   **After (Phase 1 - compile-time checked):**
   ```rust
   sqlx::query!(
       "INSERT INTO messages (id, account_id, provider_id, thread_id, from_email, subject, date, flags, snippet)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
       id, account_id, provider_id, thread_id, from_email, subject, date, flags, snippet
   )
   .execute(&pool)
   .await?;
   ```

4. Generate the `.sqlx/` directory:
   ```bash
   cargo sqlx prepare --workspace
   ```

5. Commit the `.sqlx/` directory to the repo.

### CI Update

```yaml
# Add to .github/workflows/ci.yml

  sqlx-check:
    name: SQLx Prepare Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo install sqlx-cli --no-default-features --features sqlite
      - run: cargo sqlx prepare --workspace --check
```

### Key Conversion Patterns

```rust
// crates/store/src/messages.rs

impl Store {
    /// Insert or update an envelope in the messages table.
    pub async fn upsert_envelope(&self, env: &Envelope) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO messages (
                id, account_id, provider_id, thread_id,
                message_id_header, in_reply_to, reference_headers,
                from_name, from_email, to_addrs, cc_addrs, bcc_addrs,
                subject, date, flags, snippet,
                has_attachments, size_bytes, unsubscribe_method
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            ON CONFLICT (account_id, provider_id) DO UPDATE SET
                flags = excluded.flags,
                snippet = excluded.snippet,
                unsubscribe_method = excluded.unsubscribe_method
            "#,
            env.id.to_string(),
            env.account_id.to_string(),
            env.provider_id,
            env.thread_id.to_string(),
            env.message_id_header,
            env.in_reply_to,
            serde_json::to_string(&env.references).unwrap(),
            env.from.name,
            env.from.email,
            serde_json::to_string(&env.to).unwrap(),
            serde_json::to_string(&env.cc).unwrap(),
            serde_json::to_string(&env.bcc).unwrap(),
            env.subject,
            env.date.timestamp(),
            env.flags.bits() as i64,
            env.snippet,
            env.has_attachments as i32,
            env.size_bytes as i64,
            serde_json::to_string(&env.unsubscribe).ok(),
        )
        .execute(&self.writer)
        .await?;

        Ok(())
    }

    /// Get sync cursor for an account.
    pub async fn get_sync_cursor(&self, account_id: &AccountId) -> Result<Option<SyncCursor>, StoreError> {
        let row = sqlx::query!(
            "SELECT sync_cursor FROM accounts WHERE id = $1",
            account_id.to_string()
        )
        .fetch_optional(&self.reader)
        .await?;

        match row {
            Some(r) => {
                let cursor = r.sync_cursor
                    .map(|s| serde_json::from_str::<SyncCursor>(&s))
                    .transpose()?;
                Ok(cursor)
            }
            None => Ok(None),
        }
    }

    /// Set sync cursor for an account.
    pub async fn set_sync_cursor(
        &self,
        account_id: &AccountId,
        cursor: &SyncCursor,
    ) -> Result<(), StoreError> {
        let cursor_json = serde_json::to_string(cursor)?;
        sqlx::query!(
            "UPDATE accounts SET sync_cursor = $1, updated_at = $2 WHERE id = $3",
            cursor_json,
            chrono::Utc::now().timestamp(),
            account_id.to_string()
        )
        .execute(&self.writer)
        .await?;

        Ok(())
    }

    /// Look up internal message ID by provider's message ID.
    pub async fn get_message_id_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<MessageId>, StoreError> {
        let row = sqlx::query!(
            "SELECT id FROM messages WHERE account_id = $1 AND provider_id = $2",
            account_id.to_string(),
            provider_id
        )
        .fetch_optional(&self.reader)
        .await?;

        Ok(row.map(|r| MessageId::from_string(&r.id)))
    }
}
```

### What to Test

1. **`cargo sqlx prepare --check`**: Passes in CI (queries match schema).
2. **Upsert envelope**: Insert, then upsert with changed flags, verify update applied.
3. **Sync cursor round-trip**: Set cursor, get cursor, verify identical.
4. **Provider ID lookup**: Insert envelope, look up by provider_id, verify match.

---

## Definition of Done

All of the following must be true:

- [x] `mxr accounts add gmail` — interactive flow completes, tokens stored, config written
- [x] `mxr sync` — syncs real Gmail messages into SQLite and Tantivy
- [x] `mxr sync --account NAME` — syncs a specific account
- [x] Delta sync works: second sync uses `history.list`, is fast (seconds, not minutes)
- [x] Messages appear in TUI with real data from Gmail
- [x] Opening a message fetches and displays the body (lazy hydration)
- [x] `mxr search "from:alice"` — returns results from CLI
- [x] `mxr search "subject:invoice is:unread after:2026-01-01"` — compound query works
- [ ] `mxr search --save "name" "query"` — creates saved search (via `mxr saved add` instead)
- [ ] `mxr search --saved "name"` — executes saved search (via `mxr saved run` instead)
- [x] `/` in TUI opens search input, results appear in message list (A005)
- [ ] `n`/`N` navigate to next/prev search result in TUI (A005) — stubbed
- [x] `gi`/`gs`/`gt`/`gd` navigate to inbox/starred/sent/drafts via multi-key state machine (A005)
- [x] `Ctrl-p` opens command palette with fuzzy matching (A005)
- [x] Saved searches appear in sidebar and command palette
- [x] Sidebar shows labels with unread counts
- [x] Status bar shows per-account sync status: "personal: synced 2m ago", "work: syncing (47/200)..." (A006)
- [x] Three-pane layout works (sidebar + list + message view)
- [x] `mxr cat MESSAGE_ID` — prints message body text (A004)
- [x] `mxr cat MESSAGE_ID --raw` — prints body without processing (A004)
- [x] `mxr cat MESSAGE_ID --format json` — outputs structured JSON (A004)
- [x] `mxr thread THREAD_ID` — prints full thread chronologically (A004)
- [x] `mxr headers MESSAGE_ID` — prints raw email headers (A004)
- [x] `mxr count "query"` — prints count of matching messages (A004)
- [x] `mxr saved` — lists saved searches (A004)
- [x] `mxr saved add "name" "query"` — creates saved search (A004)
- [x] `mxr saved delete "name"` — deletes saved search (A004)
- [x] `mxr saved run "name"` — executes saved search (A004)
- [x] `mxr accounts show NAME` — shows account details (A004)
- [ ] `mxr accounts reauth NAME` — re-authenticates account (A004) — stubbed
- [ ] `mxr accounts test NAME` — tests connectivity (A004) — stubbed
- [x] `mxr status` — shows daemon health, account sync status, unread counts (A006)
- [x] `mxr sync --status` — shows per-account sync status from sync_log (A006)
- [x] `mxr sync --history` — shows recent sync log entries (A006)
- [x] `mxr logs --no-follow` — prints recent daemon logs (A006)
- [x] `mxr config` shows resolved config
- [x] `mxr config path` — prints config file path (A004)
- [x] `mxr version` — prints version, build info, data dir (A004)
- [x] `mxr doctor` runs diagnostics (config, auth, db, index)
- [x] `mxr labels` lists labels with counts
- [x] Auto-format detection: TTY outputs table, piped outputs JSON (A004)
- [x] Config.toml is parsed and values are used by daemon
- [x] `cargo sqlx prepare --check` passes in CI
- [x] `cargo test --workspace` passes (125 tests)
- [x] `cargo clippy --workspace -- -D warnings` passes
- [x] No mutations (archive, trash, star) — this is read-only

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Google Cloud Console setup friction | High | Medium | Clear documentation with screenshots. Provide a test client_id for development (with caveat about 100-user limit in test mode). |
| OAuth2 token refresh fails silently | Medium | High | Detect 401 errors aggressively. Surface "re-auth needed" in TUI status bar and `mxr doctor`. Never let auth failure crash the daemon. |
| `history.list` returns 404 (history too old) | Low | Medium | Already handled: fall back to full re-sync. Log clearly so user understands why sync is slow. |
| Gmail rate limiting (429) during initial sync of large mailbox | Medium | Medium | Process in batches of 100, limit concurrent requests to 20, implement exponential backoff on 429. Show progress in TUI. |
| MIME parsing edge cases (deeply nested, broken encoding) | Medium | Low | Use `mail-parser` for heavy lifting. Log and skip unparseable parts rather than failing the entire sync. Body text will just be empty for broken messages. |
| Tantivy schema migration if fields change | Low | High | `mxr doctor --reindex` drops and rebuilds. Document that index is always rebuildable from SQLite. |
| `yup-oauth2` version incompatibility or API changes | Low | Medium | Pin major version. yup-oauth2 v11 is stable. Wrap auth behind our own `GmailAuth` type so swapping the underlying crate is contained. |
| System keyring unavailable (headless Linux, CI) | Medium | Medium | `keyring` crate has fallback modes. For CI, use the fake provider. For headless, fall back to encrypted file storage per blueprint. |
| Query parser edge cases | High | Low | Extensive test suite (18+ test cases listed above). Graceful error messages. Unknown fields treated as text search rather than failing. |
| Large mailbox initial sync takes too long | Medium | Medium | Progressive loading (50-message chunks committed to DB and indexed). TUI shows messages as they arrive. User sees progress, not a blank screen. |
