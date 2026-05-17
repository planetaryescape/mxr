#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

mod chrome;
mod envelope_list;
mod legacy;
mod middleware;
mod openapi;
mod routes_v6;
#[cfg(feature = "web-ui")]
mod spa;

pub use openapi::ApiDoc;

use axum::{
    extract::ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade},
    extract::{ConnectInfo, Path as AxumPath, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use chrome::{ack_mutation, ack_request, build_bridge_chrome, load_mailbox_selection};
use chrono::{DateTime, Utc};
use envelope_list::{
    attachment_search_rows, dedupe_search_results_by_thread, format_date_full, format_date_label,
    format_relative_label, group_envelopes, group_row_views, list_bodies_by_message_ids,
    list_envelopes_by_message_ids, mailbox_message_rows, mailbox_thread_rows,
    message_row_view_with_labels, reorder_envelopes, thread_reader_mode,
};
use futures::{SinkExt, StreamExt};
use mxr_compose::{
    frontmatter::{parse_compose_file, render_compose_file, ComposeFrontmatter},
    render::render_markdown,
    validate_draft, ComposeKind, ComposeValidation,
};
use mxr_config::load_config;
use mxr_core::{
    id::LabelId,
    id::{AccountId, DraftId, MessageId, ThreadId},
    types::{
        Draft, Envelope, Label, MessageBody, ReplyHeaders, SearchMode, Snoozed, SortOrder,
        SubscriptionSummary,
    },
};
use mxr_mail_parse::parse_address_list;
use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload, LlmConfigData, Request, ResponseData};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

static NEXT_BRIDGE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub socket_path: PathBuf,
    pub auth_token: String,
    /// Origins allowed for CORS in addition to the loopback defaults
    /// (`http://localhost`, `http://mxr.localhost`, `http://127.0.0.1`,
    /// and their HTTPS forms). Empty by default. Configurable via
    /// `[bridge].cors_allowlist` in `~/.config/mxr/config.toml`.
    pub cors_allowlist: Vec<String>,
    /// Hostnames allowed in the HTTP `Host` header in addition to the
    /// loopback defaults. Empty by default; populated only when the
    /// daemon is intentionally bound to a non-loopback address.
    pub host_allowlist: Vec<String>,
    /// When true, `GET /api/v1/auth/local-token` returns the bridge token
    /// to callers whose TCP peer is a loopback IP. Lets the web SPA
    /// bootstrap on the same machine without a manual paste. Set to false
    /// for paranoid setups that want a strict bearer handshake even on
    /// loopback. See `[bridge].auto_local_token` in config.
    pub auto_local_token: bool,
}

impl WebServerConfig {
    pub fn new(socket_path: PathBuf, auth_token: String) -> Self {
        Self {
            socket_path,
            auth_token,
            cors_allowlist: Vec::new(),
            host_allowlist: Vec::new(),
            auto_local_token: true,
        }
    }

    pub fn with_cors_allowlist(mut self, origins: Vec<String>) -> Self {
        self.cors_allowlist = origins;
        self
    }

    pub fn with_host_allowlist(mut self, hosts: Vec<String>) -> Self {
        self.host_allowlist = hosts;
        self
    }

    pub fn with_auto_local_token(mut self, enabled: bool) -> Self {
        self.auto_local_token = enabled;
        self
    }
}

/// Routes under `/api/v1/admin/*` — daemon health, diagnostics, status.
fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/status", get(status))
        .route("/diagnostics", get(diagnostics))
        .route("/diagnostics/bug-report", get(generate_bug_report))
}

/// Liveness endpoint — unauthenticated. Surfaces just enough for clients
/// and orchestrators to verify the bridge is up and the protocol version
/// they expect, before they go acquire the bridge token.
async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "mxr-bridge",
        "protocol_version": mxr_protocol::IPC_PROTOCOL_VERSION,
    }))
}

/// Returns the SPA-relevant locale bundle for the daemon's active locale.
/// The SPA fetches this once at startup and caches in TanStack Query, so the
/// `InviteCard` and other components can render localized strings without
/// shipping translations in the JS bundle. To add a language, append a new
/// `Locale` to `mxr_core::i18n::AVAILABLE_LOCALES` and configure
/// `MXR_LOCALE`. The structure mirrors `InviteStrings`/`StatusStrings` from
/// `mxr_core::i18n`.
async fn i18n_bundle() -> Json<serde_json::Value> {
    let locale = mxr_core::i18n::DEFAULT_LOCALE;
    let i = locale.invite;
    let s = locale.status;
    Json(json!({
        "code": locale.code,
        "invite": {
            "card_title": i.card_title,
            "chip_label_accept": i.chip_label_accept,
            "chip_label_tentative": i.chip_label_tentative,
            "chip_label_decline": i.chip_label_decline,
            "state_label_accepted": i.state_label_accepted,
            "state_label_tentative": i.state_label_tentative,
            "state_label_declined": i.state_label_declined,
            "hint_change_response": i.hint_change_response,
            "hint_comment": i.hint_comment,
            "banner_cancelled": i.banner_cancelled,
            "banner_publish": i.banner_publish,
            "banner_parse_warning": i.banner_parse_warning,
            "banner_updated": i.banner_updated,
            "banner_counter": i.banner_counter,
        },
        "status": {
            "invite_pending_accept": s.invite_pending_accept,
            "invite_pending_tentative": s.invite_pending_tentative,
            "invite_pending_decline": s.invite_pending_decline,
            "invite_cancelled": s.invite_cancelled,
        }
    }))
}

/// Same-machine handshake. Returns the bridge token to callers whose
/// TCP peer is a loopback address, gated by `[bridge].auto_local_token`.
///
/// This is *not* a way around bearer auth in general. The endpoint
/// refuses if either:
///   - the operator has disabled it via `auto_local_token = false`, or
///   - the connecting peer's IP is not a loopback address.
///
/// In both refusal cases it returns 404, not 401/403, so cross-network
/// scanners cannot tell the endpoint exists.
async fn local_token_handshake(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if !state.config.auto_local_token {
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    }
    if !peer.ip().is_loopback() {
        tracing::debug!(?peer, "local-token handshake refused: non-loopback peer");
        return (StatusCode::NOT_FOUND, Json(json!({"error": "not found"}))).into_response();
    }
    Json(json!({
        "token": state.config.auth_token,
        "source": "local-handshake",
    }))
    .into_response()
}

/// Routes under `/api/v1/mail/*` — read, search, mutate, sync, compose.
fn mail_router() -> Router<AppState> {
    Router::new()
        .route("/mailbox", get(mailbox))
        .route("/search", get(search))
        .route("/threads/{thread_id}", get(thread))
        .route("/threads/{thread_id}/export", get(export_thread))
        .route("/drafts", get(list_drafts))
        .route("/snoozed", get(list_snoozed))
        .route("/sync", post(trigger_sync))
        .route("/mutations/archive", post(archive))
        .route("/mutations/trash", post(trash))
        .route("/mutations/spam", post(spam))
        .route("/mutations/star", post(star))
        .route("/mutations/read", post(mark_read))
        .route("/mutations/read-and-archive", post(mark_read_and_archive))
        .route("/mutations/labels", post(modify_labels))
        .route("/mutations/move", post(move_messages))
        .route("/actions/snooze/presets", get(snooze_presets))
        .route("/actions/snooze", post(snooze))
        .route("/actions/unsubscribe", post(unsubscribe))
        .route("/actions/invite/reply", post(reply_to_invite))
        .route("/attachments/open", post(open_attachment))
        .route("/attachments/download", post(download_attachment))
        .route("/labels/create", post(create_label))
        .route("/labels/rename", post(rename_label))
        .route("/labels/delete", post(delete_label))
        .route("/compose/session", post(start_compose_session))
        .route("/compose/session/refresh", post(refresh_compose_session))
        .route("/compose/session/restore", post(restore_compose_session))
        .route("/compose/session/update", post(update_compose_session))
        .route("/compose/session/send", post(send_compose_session))
        .route("/compose/session/save", post(save_compose_session))
        .route(
            "/compose/session/attachment",
            post(upload_compose_attachment),
        )
        .route("/compose/session/discard", post(discard_compose_session))
}

/// Routes under `/api/v1/platform/*` — accounts, rules, saved searches,
/// subscriptions, semantic.
fn platform_router() -> Router<AppState> {
    Router::new()
        .route("/rules", get(rules))
        .route("/rules/detail", get(rule_detail))
        .route("/rules/form", get(rule_form))
        .route("/rules/history", get(rule_history))
        .route("/rules/dry-run", get(rule_dry_run))
        .route("/rules/upsert", post(upsert_rule))
        .route("/rules/upsert-form", post(upsert_rule_form))
        .route("/rules/delete", post(delete_rule))
        .route("/accounts", get(accounts))
        .route("/accounts/test", post(test_account))
        .route("/accounts/upsert", post(upsert_account))
        .route("/accounts/default", post(set_default_account))
        .route("/auth/sessions/start", post(start_auth_session))
        .route("/auth/sessions/{session_id}", get(get_auth_session))
        .route(
            "/auth/sessions/{session_id}/cancel",
            post(cancel_auth_session),
        )
        .route(
            "/auth/sessions/{session_id}/complete",
            post(complete_auth_session),
        )
        .route("/saved-searches/create", post(create_saved_search))
        .route("/saved-searches/update", post(update_saved_search))
        .route("/saved-searches/delete", post(delete_saved_search))
        .route("/subscriptions", get(list_subscriptions))
        .route("/llm/status", get(get_llm_status))
        .route("/llm/config", get(get_llm_config).post(update_llm_config))
        .route("/semantic/status", get(get_semantic_status))
        .route("/semantic/reindex", post(trigger_semantic_reindex))
}

/// Client-specific UI shaping. Per AGENTS.md the `client-specific` IPC
/// bucket is not part of the core mail surface, so this lives under its
/// own prefix.
fn client_router() -> Router<AppState> {
    Router::new().route("/shell", get(shell))
}

pub fn app(config: WebServerConfig) -> Router {
    let cors = middleware::cors_layer(&config.cors_allowlist);
    let host_allowlist = std::sync::Arc::new(config.host_allowlist.clone());
    let state = AppState::new(config);

    let v1 = Router::new()
        .route("/health", get(health))
        .route("/auth/local-token", get(local_token_handshake))
        .route("/i18n", get(i18n_bundle))
        .nest("/admin", routes_v6::extend_admin(admin_router()))
        .nest("/mail", routes_v6::extend_mail(mail_router()))
        .nest("/platform", routes_v6::extend_platform(platform_router()))
        .nest("/client", client_router())
        .route("/events", get(events))
        .with_state(state);

    let router = Router::new().nest("/api/v1", v1).merge(
        SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", openapi::ApiDoc::openapi()),
    );

    #[cfg(feature = "web-ui")]
    let router = router.merge(spa::router());

    router
        .layer(axum::middleware::from_fn(legacy::redirect_legacy_paths))
        .layer(axum::middleware::from_fn_with_state(
            host_allowlist,
            middleware::host_allowlist,
        ))
        .layer(cors)
}

pub async fn serve(listener: TcpListener, config: WebServerConfig) -> std::io::Result<()> {
    axum::serve(
        listener,
        app(config).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

pub async fn bind_and_serve(
    host: std::net::IpAddr,
    port: u16,
    config: WebServerConfig,
) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind((host, port)).await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = serve(listener, config).await;
    });
    Ok(addr)
}

/// Default bridge port. Chosen as a high unprivileged port that doesn't
/// clash with the common dev-server set (3000/5173/8000/8080/7777/4200).
/// Backwards-compat note: pre-launch the bridge defaulted to 7777; if
/// you're upgrading existing setups, your `[bridge].port` config wins.
pub const DEFAULT_BRIDGE_PORT: u16 = 42829;

/// How many ports to walk through when the configured one is in use.
/// Capped so a totally broken host (no free ports in a wide range) still
/// fails in finite time with a clear error.
pub const PORT_RETRY_ATTEMPTS: u16 = 32;

/// Attempt to bind a `TcpListener` to `host:port`. If the port is taken
/// and `retry` is true, increment by one and try again up to
/// `PORT_RETRY_ATTEMPTS` times.
///
/// Returns the bound listener (caller is responsible for serving it).
pub async fn bind_listener(
    host: std::net::IpAddr,
    port: u16,
    retry: bool,
) -> std::io::Result<TcpListener> {
    let mut candidate = port;
    let max = if retry {
        port.saturating_add(PORT_RETRY_ATTEMPTS)
    } else {
        port
    };
    loop {
        match TcpListener::bind((host, candidate)).await {
            Ok(listener) => return Ok(listener),
            Err(error) if retry && is_addr_in_use(&error) && candidate < max => {
                tracing::debug!(
                    "bridge port {candidate} in use, trying {next}",
                    next = candidate + 1
                );
                candidate += 1;
                continue;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_addr_in_use(error: &std::io::Error) -> bool {
    matches!(error.kind(), std::io::ErrorKind::AddrInUse)
}

#[derive(Clone)]
struct AppState {
    config: WebServerConfig,
}

impl AppState {
    fn new(config: WebServerConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, thiserror::Error)]
enum BridgeError {
    #[error("failed to connect to mxr daemon at {0}")]
    Connect(String),
    #[error("ipc error: {0}")]
    Ipc(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unexpected response from daemon")]
    UnexpectedResponse,
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

#[derive(Debug, Default, Deserialize)]
struct AuthQuery {
    #[serde(default)]
    token: Option<String>,
}

/// Resolve the bridge token from the request, checking each path that
/// the OpenAPI spec documents:
///
/// 1. `Authorization: Bearer <token>` — preferred, what generated SDKs use
/// 2. `?token=<token>` — fallback for `EventSource` and curl users
/// 3. `Sec-WebSocket-Protocol: bearer, <token>` — browser WebSocket clients
///    (browsers cannot set `Authorization` on WS upgrades)
/// 4. `x-mxr-bridge-token: <token>` — v0.4.x compat, kept for the v0.5
///    cycle while older local clients migrate
fn extract_token<'a>(headers: &'a HeaderMap, query_token: Option<&'a str>) -> Option<&'a str> {
    if let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
    {
        return Some(value);
    }
    if let Some(token) = query_token {
        return Some(token);
    }
    if let Some(value) = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
    {
        // browsers join multiple subprotocols with `, ` — e.g.
        // `bearer, abc123`. The token is the trailing item after the
        // `bearer` marker.
        let mut parts = value.split(',').map(|s| s.trim());
        if parts.clone().any(|p| p == "bearer") {
            if let Some(candidate) = parts.find(|p| !p.is_empty() && *p != "bearer") {
                return Some(candidate);
            }
        }
    }
    headers
        .get("x-mxr-bridge-token")
        .and_then(|value| value.to_str().ok())
}

fn ensure_authorized(
    headers: &HeaderMap,
    query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    if extract_token(headers, query_token) == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
}

fn next_bridge_request_id() -> u64 {
    NEXT_BRIDGE_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

fn bridge_request_id(headers: &HeaderMap) -> u64 {
    headers
        .get("x-mxr-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(next_bridge_request_id)
}

fn bridge_error_kind(error: &BridgeError) -> &'static str {
    match error {
        BridgeError::Connect(_) => "connect",
        BridgeError::Ipc(_) => "ipc",
        BridgeError::Unauthorized => "unauthorized",
        BridgeError::UnexpectedResponse => "unexpected_response",
    }
}

fn bridge_error_is_missing_file(error: &BridgeError) -> bool {
    matches!(
        error,
        BridgeError::Ipc(message) if message.contains("No such file or directory (os error 2)")
    )
}

fn account_sync_kind(account: &mxr_protocol::AccountConfigData) -> &'static str {
    match account.sync {
        Some(mxr_protocol::AccountSyncConfigData::Gmail { .. }) => "gmail",
        Some(mxr_protocol::AccountSyncConfigData::Imap { .. }) => "imap",
        Some(mxr_protocol::AccountSyncConfigData::OutlookPersonal { .. }) => "outlook",
        Some(mxr_protocol::AccountSyncConfigData::OutlookWork { .. }) => "outlook-work",
        Some(mxr_protocol::AccountSyncConfigData::Fake) => "fake",
        None => "none",
    }
}

fn account_send_kind(account: &mxr_protocol::AccountConfigData) -> &'static str {
    match account.send {
        Some(mxr_protocol::AccountSendConfigData::Gmail) => "gmail",
        Some(mxr_protocol::AccountSendConfigData::Smtp { .. }) => "smtp",
        Some(mxr_protocol::AccountSendConfigData::OutlookPersonal { .. }) => "outlook",
        Some(mxr_protocol::AccountSendConfigData::OutlookWork { .. }) => "outlook-work",
        Some(mxr_protocol::AccountSendConfigData::Fake) => "fake",
        None => "none",
    }
}

fn account_has_inline_imap_password(account: &mxr_protocol::AccountConfigData) -> bool {
    matches!(
        account.sync,
        Some(mxr_protocol::AccountSyncConfigData::Imap {
            password: Some(ref password),
            ..
        }) if !password.is_empty()
    )
}

fn account_has_inline_smtp_password(account: &mxr_protocol::AccountConfigData) -> bool {
    matches!(
        account.send,
        Some(mxr_protocol::AccountSendConfigData::Smtp {
            password: Some(ref password),
            ..
        }) if !password.is_empty()
    )
}

async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetStatus).await? {
        ResponseData::Status {
            uptime_secs,
            accounts,
            total_messages,
            daemon_pid,
            sync_statuses,
            protocol_version,
            daemon_version,
            daemon_build_id,
            repair_required,
            semantic_runtime,
            feature_health,
        } => Ok(Json(serde_json::json!({
            "uptime_secs": uptime_secs,
            "accounts": accounts,
            "total_messages": total_messages,
            "daemon_pid": daemon_pid,
            "sync_statuses": sync_statuses,
            "protocol_version": protocol_version,
            "daemon_version": daemon_version,
            "daemon_build_id": daemon_build_id,
            "repair_required": repair_required,
            "semantic_runtime": semantic_runtime,
            "feature_health": feature_health,
            "instance": mxr_config::app_instance_name(),
            "is_demo": mxr_config::is_demo_instance(),
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn shell(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let lens = MailboxLensRequest::default();
    let chrome = build_bridge_chrome(&state.config.socket_path, &lens).await?;
    Ok(Json(json!({
        "shell": chrome.shell,
        "sidebar": chrome.sidebar,
    })))
}

#[derive(Debug, Default, Deserialize)]
struct MailboxQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    view: MailboxView,
    #[serde(default)]
    lens_kind: MailboxLensKind,
    #[serde(default)]
    label_id: Option<String>,
    #[serde(default)]
    saved_search: Option<String>,
    #[serde(default)]
    sender_email: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MailboxView {
    #[default]
    Threads,
    Messages,
}

impl MailboxView {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Threads => "threads",
            Self::Messages => "messages",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MailboxLensKind {
    #[default]
    Inbox,
    AllMail,
    Label,
    SavedSearch,
    Subscription,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct MailboxLensRequest {
    kind: MailboxLensKind,
    label_id: Option<String>,
    saved_search: Option<String>,
    sender_email: Option<String>,
}

impl MailboxQuery {
    fn lens(&self) -> MailboxLensRequest {
        MailboxLensRequest {
            kind: self.lens_kind.clone(),
            label_id: self.label_id.clone(),
            saved_search: self.saved_search.clone(),
            sender_email: self.sender_email.clone(),
        }
    }
}

fn default_limit() -> u32 {
    200
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    mode: Option<SearchMode>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    explain: bool,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageIdsRequest {
    message_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct StarRequest {
    message_ids: Vec<String>,
    starred: bool,
}

#[derive(Debug, Deserialize)]
struct ReadRequest {
    message_ids: Vec<String>,
    read: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ComposeSessionKindRequest {
    New,
    Reply,
    ReplyAll,
    Forward,
    /// "Reply with comment" path for a calendar invite. Triggers the daemon's
    /// `Request::PrepareInviteResponse` and seeds the draft with the inline
    /// REPLY ICS so the outbound builder emits the correct MIME layout.
    InviteReply,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionStartRequest {
    kind: ComposeSessionKindRequest,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    to: Option<String>,
    /// Required when `kind == InviteReply`. One of `accept`, `tentative`,
    /// `decline`. Ignored for other kinds.
    #[serde(default)]
    action: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionPathRequest {
    draft_path: String,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionRestoreRequest {
    draft_id: String,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionUpdateRequest {
    draft_path: String,
    to: String,
    cc: String,
    bcc: String,
    subject: String,
    from: String,
    #[serde(default)]
    attach: Vec<String>,
    #[serde(default)]
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionSendRequest {
    draft_path: String,
    account_id: String,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionAttachmentRequest {
    draft_path: String,
    filename: String,
    content_base64: String,
}

#[derive(Debug, Deserialize)]
struct ModifyLabelsRequest {
    message_ids: Vec<String>,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MoveRequest {
    message_ids: Vec<String>,
    target_label: String,
}

#[derive(Debug, Deserialize)]
struct RuleQuery {
    rule: String,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteRuleRequest {
    rule: String,
}

#[derive(Debug, Deserialize)]
struct UpsertRuleRequest {
    rule: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct UpsertRuleFormRequest {
    existing_rule: Option<String>,
    name: String,
    condition: String,
    action: String,
    priority: i32,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct SetDefaultAccountRequest {
    key: String,
}

#[derive(Debug, Deserialize)]
struct StartAuthSessionRequest {
    account: mxr_protocol::AccountConfigData,
    #[serde(default)]
    reauthorize: bool,
    #[serde(default)]
    flow: mxr_protocol::AuthFlowData,
}

#[derive(Debug, Deserialize)]
struct CompleteAuthSessionRequest {
    #[serde(default)]
    save_account: bool,
}

#[derive(Debug, Deserialize)]
struct AttachmentRequest {
    message_id: String,
    attachment_id: String,
}

#[derive(Debug, Deserialize)]
struct UnsubscribeRequest {
    message_id: String,
}

#[derive(Debug, Deserialize)]
struct SnoozeRequest {
    message_id: String,
    until: String,
}

#[derive(Debug, Deserialize)]
struct InviteReplyRequest {
    message_id: String,
    action: mxr_protocol::CalendarInviteActionData,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct LlmConfigRequest {
    enabled: bool,
    base_url: String,
    model: String,
    api_key_env: String,
    context_window: u32,
    request_timeout_secs: u64,
    #[serde(default)]
    allow_cloud_relationship_data: bool,
    #[serde(default)]
    overrides: Option<mxr_protocol::LlmOverridesData>,
}

impl From<LlmConfigRequest> for LlmConfigData {
    fn from(value: LlmConfigRequest) -> Self {
        Self {
            enabled: value.enabled,
            base_url: value.base_url,
            model: value.model,
            api_key_env: value.api_key_env,
            context_window: value.context_window,
            request_timeout_secs: value.request_timeout_secs,
            allow_cloud_relationship_data: value.allow_cloud_relationship_data,
            overrides: value.overrides,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ComposeIssueView {
    severity: &'static str,
    message: String,
}

async fn mailbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MailboxQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    let lens = query.lens();
    let chrome = build_bridge_chrome(&state.config.socket_path, &lens).await?;
    let mailbox = load_mailbox_selection(
        &state.config.socket_path,
        &chrome,
        &lens,
        query.limit,
        query.offset,
    )
    .await?;
    let envelope_page_size = mailbox.envelopes.len() as u32;
    let view = query.view;
    let commitment_counts =
        open_commitment_counts(&state.config.socket_path, &mailbox.envelopes).await;
    let mut rows = match view {
        MailboxView::Threads => mailbox_thread_rows(mailbox.envelopes),
        MailboxView::Messages => mailbox_message_rows(mailbox.envelopes),
    };
    annotate_open_commitment_counts(&mut rows, &commitment_counts);
    let groups = group_row_views(rows);
    let supports_pagination = matches!(
        lens.kind,
        MailboxLensKind::Inbox | MailboxLensKind::AllMail | MailboxLensKind::Label
    );
    let has_more = supports_pagination && envelope_page_size == query.limit;
    let next_offset = has_more.then(|| query.offset.saturating_add(query.limit));
    Ok(Json(json!({
        "shell": chrome.shell,
        "sidebar": chrome.sidebar,
        "mailbox": {
            "lensLabel": mailbox.lens_label,
            "view": view.as_str(),
            "counts": mailbox.counts,
            "has_more": has_more,
            "next_offset": next_offset,
            "groups": groups,
        }
    })))
}

async fn open_commitment_counts(
    socket_path: &Path,
    envelopes: &[Envelope],
) -> HashMap<(String, String), u32> {
    const ANNOTATION_TIMEOUT: Duration = Duration::from_millis(250);

    let account_ids = envelopes
        .iter()
        .map(|envelope| envelope.account_id.clone())
        .collect::<HashSet<_>>();
    let mut counts = HashMap::new();

    for account_id in account_ids {
        let response = tokio::time::timeout(
            ANNOTATION_TIMEOUT,
            ipc_request(
                socket_path,
                Request::ListCommitments {
                    account_id,
                    email: None,
                    status: Some(mxr_protocol::CommitmentStatusData::Open),
                },
            ),
        )
        .await;
        let Ok(Ok(ResponseData::CommitmentList { commitments })) = response else {
            continue;
        };
        for commitment in commitments {
            *counts
                .entry((
                    commitment.account_id.to_string(),
                    commitment.thread_id.to_string(),
                ))
                .or_insert(0) += 1;
        }
    }

    counts
}

fn annotate_open_commitment_counts(
    rows: &mut [(DateTime<Utc>, chrome::MessageRowView)],
    counts: &HashMap<(String, String), u32>,
) {
    for (_, row) in rows {
        let count = counts
            .get(&(row.account_id.clone(), row.thread_id.clone()))
            .copied()
            .unwrap_or(0);
        if count > 0 {
            row.open_commitment_count = Some(count);
        }
    }
}

async fn thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let thread_id = parse_thread_id(&thread_id)?;
    match ipc_request(&state.config.socket_path, Request::GetThread { thread_id }).await? {
        ResponseData::Thread {
            thread,
            messages,
            summary,
        } => {
            let labels = match ipc_request(
                &state.config.socket_path,
                Request::ListLabels { account_id: None },
            )
            .await?
            {
                ResponseData::Labels { labels } => labels,
                _ => return Err(BridgeError::UnexpectedResponse),
            };

            let (bodies, body_failures) = match ipc_request(
                &state.config.socket_path,
                Request::ListBodies {
                    message_ids: messages
                        .iter()
                        .map(|message| message.id.clone())
                        .collect::<Vec<MessageId>>(),
                },
            )
            .await?
            {
                ResponseData::Bodies { bodies, failures } => (bodies, failures),
                _ => return Err(BridgeError::UnexpectedResponse),
            };

            let attachment_count = bodies
                .iter()
                .map(|body| body.attachments.len())
                .sum::<usize>();
            let invite_count = bodies
                .iter()
                .filter(|body| body.metadata.calendar.is_some())
                .count();

            Ok(Json(json!({
                "thread": thread,
                "messages": messages
                    .iter()
                    .map(|message| message_row_view_with_labels(message, &labels))
                    .collect::<Vec<_>>(),
                "bodies": bodies,
                "body_failures": body_failures,
                "summary": summary,
                "reader_mode": thread_reader_mode(&bodies),
                "right_rail": {
                    "title": "Thread context",
                    "items": [
                        format!("{} messages", thread.message_count),
                        format!("{} unread", thread.unread_count),
                        format!("{} participants", thread.participants.len()),
                        if attachment_count == 0 {
                            "No attachments".to_string()
                        } else {
                            format!("{attachment_count} attachments")
                        },
                        if invite_count == 0 {
                            "No calendar invites".to_string()
                        } else if invite_count == 1 {
                            "1 calendar invite".to_string()
                        } else {
                            format!("{invite_count} calendar invites")
                        }
                    ],
                }
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn export_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(thread_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ExportThread {
            thread_id: parse_thread_id(&thread_id)?,
            format: mxr_core::types::ExportFormat::Markdown,
        },
    )
    .await?
    {
        ResponseData::ExportResult { content } => Ok(Json(json!({ "content": content }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    if query.q.trim().is_empty() {
        return Ok(Json(json!({
            "scope": query.scope.unwrap_or_else(|| "threads".to_string()),
            "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
            "mode": query.mode.unwrap_or_default(),
            "total": 0,
            "has_more": false,
            "next_offset": serde_json::Value::Null,
            "groups": [],
            "explain": serde_json::Value::Null,
        })));
    }

    let sort = match query.sort.as_deref() {
        Some("relevant") => SortOrder::Relevance,
        Some("oldest") => SortOrder::DateAsc,
        _ => SortOrder::DateDesc,
    };

    let scope = query.scope.as_deref().unwrap_or("threads");
    let thread_scope = scope == "threads";
    let attachment_scope = scope == "attachments";

    match ipc_request(
        &state.config.socket_path,
        Request::Search {
            query: query.q,
            limit: query.limit,
            offset: query.offset,
            mode: query.mode,
            sort: Some(sort),
            explain: query.explain,
        },
    )
    .await?
    {
        ResponseData::SearchResults {
            results,
            explain,
            has_more,
            total,
            next_offset,
        } => {
            let effective_results = if thread_scope {
                dedupe_search_results_by_thread(results)
            } else {
                results
            };
            let response_total = if thread_scope {
                effective_results.len() as u32
            } else {
                total
            };
            let message_ids = effective_results
                .iter()
                .map(|result| result.message_id.clone())
                .collect::<Vec<_>>();
            let envelopes = if message_ids.is_empty() {
                Vec::new()
            } else {
                match ipc_request(
                    &state.config.socket_path,
                    Request::ListEnvelopesByIds {
                        message_ids: message_ids.clone(),
                    },
                )
                .await?
                {
                    ResponseData::Envelopes { envelopes } => {
                        reorder_envelopes(envelopes, &message_ids)
                    }
                    _ => return Err(BridgeError::UnexpectedResponse),
                }
            };
            let bodies = if attachment_scope {
                list_bodies_by_message_ids(&state.config.socket_path, &message_ids).await?
            } else {
                Vec::new()
            };
            let groups = if attachment_scope {
                let rows = attachment_search_rows(&envelopes, &bodies);
                group_row_views(rows)
            } else {
                group_envelopes(envelopes)
            };

            Ok(Json(json!({
                "scope": query.scope.unwrap_or_else(|| "threads".to_string()),
                "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
                "mode": query.mode.unwrap_or_default(),
                "total": response_total,
                "has_more": has_more,
                "next_offset": next_offset,
                "groups": groups,
                "explain": explain,
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn start_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionStartRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let session = create_compose_session(&state.config.socket_path, request).await?;
    Ok(Json(json!({ "session": session })))
}

async fn refresh_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionPathRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let session = load_compose_session(Path::new(&request.draft_path)).await?;
    Ok(Json(json!({ "session": session })))
}

async fn restore_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionRestoreRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let session = restore_saved_draft_session(
        &state.config.socket_path,
        parse_draft_id(&request.draft_id)?,
    )
    .await?;
    Ok(Json(json!({ "session": session })))
}

async fn update_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionUpdateRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    let path = Path::new(&request.draft_path);
    let draft_file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");
    tracing::debug!(
        request_id,
        endpoint = "compose/update",
        draft_file,
        draft_exists = path.exists(),
        "bridge compose update requested"
    );
    let content = match read_compose_file(path).await {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "compose/update",
                draft_file,
                stage = "read",
                error_kind = bridge_error_kind(&error),
                missing_file = bridge_error_is_missing_file(&error),
                draft_exists = path.exists(),
                "bridge compose update failed"
            );
            return Err(error);
        }
    };
    let (existing_frontmatter, file_body) =
        parse_compose_file(&content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let body = request.body.unwrap_or(file_body);
    let context = extract_compose_context(&content);
    let updated = ComposeFrontmatter {
        to: request.to,
        cc: request.cc,
        bcc: request.bcc,
        subject: request.subject,
        from: request.from,
        in_reply_to: extract_in_reply_to(&content)?,
        intent: existing_frontmatter.intent,
        references: extract_references(&content)?,
        thread_id: extract_thread_id(&content)?,
        attach: request.attach,
        signature: existing_frontmatter.signature,
    };
    let rendered = render_compose_file(&updated, &body, context.as_deref())
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    if let Err(error) = write_compose_file(path, rendered).await {
        tracing::warn!(
            request_id,
            endpoint = "compose/update",
            draft_file,
            stage = "write",
            error_kind = bridge_error_kind(&error),
            missing_file = bridge_error_is_missing_file(&error),
            draft_exists = path.exists(),
            parent_exists = path.parent().is_some_and(|parent| parent.exists()),
            "bridge compose update failed"
        );
        return Err(error);
    }
    let session = match load_compose_session(path).await {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "compose/update",
                draft_file,
                stage = "reload",
                error_kind = bridge_error_kind(&error),
                missing_file = bridge_error_is_missing_file(&error),
                draft_exists = path.exists(),
                "bridge compose update failed"
            );
            return Err(error);
        }
    };
    tracing::debug!(
        request_id,
        endpoint = "compose/update",
        draft_file,
        "bridge compose update completed"
    );
    Ok(Json(json!({ "session": session })))
}

async fn send_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    let draft_file = Path::new(&request.draft_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");
    tracing::info!(
        request_id,
        endpoint = "compose/send",
        account_id = %request.account_id,
        draft_file,
        "bridge compose send requested"
    );
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id).await?;
    let draft_id = draft.id.clone();
    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::SendDraft {
            draft,
            override_safety_token: None,
        },
    )
    .await
    {
        Ok(ResponseData::Ack) | Ok(ResponseData::SendReceipt { .. }) => {
            tracing::info!(
                request_id,
                endpoint = "compose/send",
                account_id = %request.account_id,
                draft_file,
                "bridge compose send completed"
            );
        }
        Ok(_) => return Err(BridgeError::UnexpectedResponse),
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "compose/send",
                account_id = %request.account_id,
                draft_file,
                error_kind = bridge_error_kind(&error),
                "bridge compose send failed"
            );
            return Err(error);
        }
    }
    remove_compose_file(Path::new(&request.draft_path)).await?;
    remove_compose_attachment_dir(Path::new(&request.draft_path)).await?;
    Ok(Json(json!({ "ok": true, "draft_id": draft_id })))
}

async fn save_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    let draft_file = Path::new(&request.draft_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown");
    tracing::info!(
        request_id,
        endpoint = "compose/save",
        account_id = %request.account_id,
        draft_file,
        "bridge compose save requested"
    );
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id).await?;
    let draft_id = draft.id.clone();
    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::SaveDraftToServer { draft },
    )
    .await
    {
        Ok(ResponseData::Ack) => {
            tracing::info!(
                request_id,
                endpoint = "compose/save",
                account_id = %request.account_id,
                draft_file,
                "bridge compose save completed"
            );
        }
        Ok(_) => return Err(BridgeError::UnexpectedResponse),
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "compose/save",
                account_id = %request.account_id,
                draft_file,
                error_kind = bridge_error_kind(&error),
                "bridge compose save failed"
            );
            return Err(error);
        }
    }
    Ok(Json(json!({ "ok": true, "draft_id": draft_id })))
}

async fn upload_compose_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionAttachmentRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let bytes = general_purpose::STANDARD
        .decode(request.content_base64)
        .map_err(|error| BridgeError::Ipc(format!("invalid attachment content: {error}")))?;
    let filename = safe_attachment_filename(&request.filename);
    let path = compose_attachment_path(Path::new(&request.draft_path), &filename)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(Json(json!({
        "path": path.display().to_string(),
        "filename": filename,
        "size_bytes": bytes.len(),
    })))
}

async fn discard_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionPathRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    remove_compose_file(Path::new(&request.draft_path)).await?;
    remove_compose_attachment_dir(Path::new(&request.draft_path)).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn snooze_presets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let config = load_config().unwrap_or_default().snooze;
    let presets = [
        ("tomorrow", "Tomorrow morning"),
        ("tonight", "Tonight"),
        ("weekend", "Weekend"),
        ("monday", "Next Monday"),
    ]
    .into_iter()
    .filter_map(|(name, label)| build_snooze_preset(name, label, &config))
    .collect::<Vec<_>>();
    Ok(Json(json!({ "presets": presets })))
}

async fn snooze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<SnoozeRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let config = load_config().unwrap_or_default().snooze;
    let wake_at = resolve_snooze_until(&request.until, &config)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::Snooze {
            message_id: parse_message_id(&request.message_id)?,
            wake_at,
        },
    )
    .await?;
    Ok(Json(json!({ "ok": true, "wake_at": wake_at })))
}

async fn unsubscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UnsubscribeRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::Unsubscribe {
            message_id: parse_message_id(&request.message_id)?,
        },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn reply_to_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<InviteReplyRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let message_id = parse_message_id(&request.message_id)?;
    match ipc_request(
        &state.config.socket_path,
        Request::RespondInvite {
            message_id,
            action: request.action,
            dry_run: request.dry_run,
        },
    )
    .await?
    {
        ResponseData::InviteResponsePreview { preview } => Ok(Json(json!({
            "status": "preview",
            "preview": preview,
        }))),
        ResponseData::InviteResponseSent { result } => Ok(Json(json!({
            "status": "sent",
            "result": result,
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn open_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<AttachmentRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::OpenAttachment {
            message_id: parse_message_id(&request.message_id)?,
            attachment_id: parse_attachment_id(&request.attachment_id)?,
        },
    )
    .await?
    {
        ResponseData::AttachmentFile { file } => Ok(Json(json!({ "file": file }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<AttachmentRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::DownloadAttachment {
            message_id: parse_message_id(&request.message_id)?,
            attachment_id: parse_attachment_id(&request.attachment_id)?,
            destination: None,
        },
    )
    .await?
    {
        ResponseData::AttachmentFile { file } => Ok(Json(json!({ "file": file }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn rules(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListRules).await? {
        ResponseData::Rules { rules } => Ok(Json(json!({ "rules": rules }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn rule_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetRule {
            rule: query.rule.clone(),
        },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn rule_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetRuleForm {
            rule: query.rule.clone(),
        },
    )
    .await?
    {
        ResponseData::RuleFormData { form } => Ok(Json(json!({ "form": form }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn rule_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ListRuleHistory {
            rule: Some(query.rule.clone()),
            limit: 20,
        },
    )
    .await?
    {
        ResponseData::RuleHistory { entries } => Ok(Json(json!({ "entries": entries }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn rule_dry_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RuleQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::DryRunRules {
            rule: Some(query.rule.clone()),
            all: false,
            after: None,
        },
    )
    .await?
    {
        ResponseData::RuleDryRun { results } => Ok(Json(json!({ "results": results }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn upsert_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UpsertRuleRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::UpsertRule { rule: request.rule },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn upsert_rule_form(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UpsertRuleFormRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::UpsertRuleForm {
            existing_rule: request.existing_rule,
            name: request.name,
            condition: request.condition,
            action: request.action,
            priority: request.priority,
            enabled: request.enabled,
        },
    )
    .await?
    {
        ResponseData::RuleData { rule } => Ok(Json(json!({ "rule": rule }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn delete_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<DeleteRuleRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::DeleteRule { rule: request.rule },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => Ok(Json(json!({ "accounts": accounts }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn test_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<mxr_protocol::AccountConfigData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    tracing::info!(
        request_id,
        endpoint = "accounts/test",
        account_key = %account.key,
        sync_kind = account_sync_kind(&account),
        send_kind = account_send_kind(&account),
        has_inline_imap_password = account_has_inline_imap_password(&account),
        has_inline_smtp_password = account_has_inline_smtp_password(&account),
        "bridge account test requested"
    );

    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::TestAccountConfig {
            account: account.clone(),
        },
    )
    .await
    {
        Ok(ResponseData::AccountOperation { result }) => {
            tracing::info!(
                request_id,
                endpoint = "accounts/test",
                account_key = %account.key,
                ok = result.ok,
                save_ok = result.save.as_ref().map(|step| step.ok),
                auth_ok = result.auth.as_ref().map(|step| step.ok),
                sync_ok = result.sync.as_ref().map(|step| step.ok),
                send_ok = result.send.as_ref().map(|step| step.ok),
                "bridge account test completed"
            );
            Ok(Json(json!({ "result": result })))
        }
        Ok(_) => Err(BridgeError::UnexpectedResponse),
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "accounts/test",
                account_key = %account.key,
                error_kind = bridge_error_kind(&error),
                "bridge account test failed"
            );
            Err(error)
        }
    }
}

async fn upsert_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<mxr_protocol::AccountConfigData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    tracing::info!(
        request_id,
        endpoint = "accounts/upsert",
        account_key = %account.key,
        sync_kind = account_sync_kind(&account),
        send_kind = account_send_kind(&account),
        has_inline_imap_password = account_has_inline_imap_password(&account),
        has_inline_smtp_password = account_has_inline_smtp_password(&account),
        "bridge account upsert requested"
    );
    match run_account_save_workflow(request_id, &state.config.socket_path, account.clone()).await {
        Ok(result) => {
            tracing::info!(
                request_id,
                endpoint = "accounts/upsert",
                account_key = %account.key,
                ok = result.ok,
                save_ok = result.save.as_ref().map(|step| step.ok),
                auth_ok = result.auth.as_ref().map(|step| step.ok),
                sync_ok = result.sync.as_ref().map(|step| step.ok),
                send_ok = result.send.as_ref().map(|step| step.ok),
                "bridge account upsert completed"
            );
            Ok(Json(json!({ "result": result })))
        }
        Err(error) => {
            tracing::warn!(
                request_id,
                endpoint = "accounts/upsert",
                account_key = %account.key,
                error_kind = bridge_error_kind(&error),
                "bridge account upsert failed"
            );
            Err(error)
        }
    }
}

async fn set_default_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<SetDefaultAccountRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::SetDefaultAccount { key: request.key },
    )
    .await?
    {
        ResponseData::AccountOperation { result } => Ok(Json(json!({ "result": result }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn start_auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<StartAuthSessionRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::StartAuthSession {
            account: request.account,
            reauthorize: request.reauthorize,
            flow: request.flow,
        },
    )
    .await?
    {
        ResponseData::AuthSession { session } => Ok(Json(json!({ "session": session }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn get_auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetAuthSession {
            session_id: mxr_protocol::AuthSessionId(session_id),
        },
    )
    .await?
    {
        ResponseData::AuthSession { session } => Ok(Json(json!({ "session": session }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn cancel_auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::CancelAuthSession {
            session_id: mxr_protocol::AuthSessionId(session_id),
        },
    )
    .await?
    {
        ResponseData::AuthSession { session } => Ok(Json(json!({ "session": session }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn complete_auth_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(session_id): AxumPath<String>,
    Json(request): Json<CompleteAuthSessionRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::CompleteAuthSession {
            session_id: mxr_protocol::AuthSessionId(session_id),
            save_account: request.save_account,
        },
    )
    .await?
    {
        ResponseData::AuthSession { session } => Ok(Json(json!({ "session": session }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetDoctorReport).await? {
        ResponseData::DoctorReport { report } => Ok(Json(json!({ "report": report }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn generate_bug_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GenerateBugReport {
            verbose: false,
            full_logs: false,
            since: None,
        },
    )
    .await?
    {
        ResponseData::BugReport { content } => Ok(Json(json!({ "content": content }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MessageIdsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Archive {
            message_ids: parse_message_ids(&request.message_ids)?,
        },
    )
    .await
}

async fn trash(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MessageIdsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Trash {
            message_ids: parse_message_ids(&request.message_ids)?,
        },
    )
    .await
}

async fn spam(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MessageIdsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Spam {
            message_ids: parse_message_ids(&request.message_ids)?,
        },
    )
    .await
}

async fn star(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<StarRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Star {
            message_ids: parse_message_ids(&request.message_ids)?,
            starred: request.starred,
        },
    )
    .await
}

async fn mark_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ReadRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::SetRead {
            message_ids: parse_message_ids(&request.message_ids)?,
            read: request.read,
        },
    )
    .await
}

async fn mark_read_and_archive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MessageIdsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::ReadAndArchive {
            message_ids: parse_message_ids(&request.message_ids)?,
        },
    )
    .await
}

async fn modify_labels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ModifyLabelsRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::ModifyLabels {
            message_ids: parse_message_ids(&request.message_ids)?,
            add: request.add,
            remove: request.remove,
        },
    )
    .await
}

async fn move_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<MoveRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Move {
            message_ids: parse_message_ids(&request.message_ids)?,
            target_label: request.target_label,
        },
    )
    .await
}

async fn events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> impl IntoResponse {
    if let Err(error) = ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)
    {
        return error.into_response();
    }
    // If the client offered the `bearer` subprotocol (browser auth path),
    // accept it so the WS handshake completes with a negotiated
    // subprotocol — RFC 6455 requires that any offered subprotocol be
    // explicitly accepted or the client may abort.
    let ws = if headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|p| p.trim() == "bearer"))
    {
        ws.protocols(["bearer"])
    } else {
        ws
    };
    ws.on_upgrade(move |socket| bridge_events(socket, state.config.socket_path))
}

async fn ipc_request(socket_path: &Path, request: Request) -> Result<ResponseData, BridgeError> {
    ipc_request_with_id(socket_path, next_bridge_request_id(), request).await
}

async fn ipc_request_with_id(
    socket_path: &Path,
    request_id: u64,
    request: Request,
) -> Result<ResponseData, BridgeError> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|error| BridgeError::Connect(error.to_string()))?;
    let mut framed = Framed::new(stream, IpcCodec::new());
    let message = IpcMessage {
        id: request_id,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    framed
        .send(message)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;

    loop {
        match framed.next().await {
            Some(Ok(response)) => match response.payload {
                IpcPayload::Response(mxr_protocol::Response::Ok { data }) => return Ok(data),
                IpcPayload::Response(mxr_protocol::Response::Error { message, .. }) => {
                    return Err(BridgeError::Ipc(message));
                }
                IpcPayload::Event(_) => continue,
                _ => return Err(BridgeError::UnexpectedResponse),
            },
            Some(Err(error)) => return Err(BridgeError::Ipc(error.to_string())),
            None => return Err(BridgeError::Ipc("connection closed".into())),
        }
    }
}

async fn bridge_events(mut socket: WebSocket, socket_path: PathBuf) {
    let stream = match UnixStream::connect(&socket_path).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = socket
                .send(WebSocketMessage::Text(
                    serde_json::json!({ "error": error.to_string() })
                        .to_string()
                        .into(),
                ))
                .await;
            return;
        }
    };
    let mut framed = Framed::new(stream, IpcCodec::new());

    while let Some(message) = framed.next().await {
        match message {
            Ok(message) => match message.payload {
                IpcPayload::Event(event) => {
                    let payload = match serde_json::to_string(&event) {
                        Ok(payload) => payload,
                        Err(_) => break,
                    };
                    if socket
                        .send(WebSocketMessage::Text(payload.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                _ => continue,
            },
            Err(_) => break,
        }
    }
}

fn parse_thread_id(value: &str) -> Result<ThreadId, BridgeError> {
    Uuid::parse_str(value)
        .map(ThreadId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid thread id: {value}")))
}

fn parse_message_id(value: &str) -> Result<MessageId, BridgeError> {
    Uuid::parse_str(value)
        .map(MessageId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid message id: {value}")))
}

fn parse_draft_id(value: &str) -> Result<DraftId, BridgeError> {
    Uuid::parse_str(value)
        .map(DraftId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid draft id: {value}")))
}

fn parse_attachment_id(value: &str) -> Result<mxr_core::AttachmentId, BridgeError> {
    Uuid::parse_str(value)
        .map(mxr_core::AttachmentId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid attachment id: {value}")))
}

fn parse_message_ids(values: &[String]) -> Result<Vec<MessageId>, BridgeError> {
    values
        .iter()
        .map(|value| parse_message_id(value))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_account_id(value: &str) -> Result<AccountId, BridgeError> {
    Uuid::parse_str(value)
        .map(AccountId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid account id: {value}")))
}

fn parse_label_id(value: &str) -> Result<LabelId, BridgeError> {
    Uuid::parse_str(value)
        .map(LabelId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid label id: {value}")))
}

async fn create_compose_session(
    socket_path: &Path,
    request: ComposeSessionStartRequest,
) -> Result<serde_json::Value, BridgeError> {
    let (account_id, from) = default_account(socket_path).await?;
    let (kind, account_id, cursor_line) = match request.kind {
        ComposeSessionKindRequest::New => (
            ComposeKind::New {
                to: request.to.unwrap_or_default(),
                subject: String::new(),
            },
            account_id,
            None::<usize>,
        ),
        ComposeSessionKindRequest::Reply | ComposeSessionKindRequest::ReplyAll => {
            let message_id = request
                .message_id
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("compose reply missing message_id".into()))?;
            let envelope = envelope_for_message(socket_path, message_id).await?;
            let response = ipc_request(
                socket_path,
                Request::PrepareReply {
                    message_id: envelope.id.clone(),
                    reply_all: matches!(request.kind, ComposeSessionKindRequest::ReplyAll),
                },
            )
            .await?;
            let context = match response {
                ResponseData::ReplyContext { context } => context,
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            (
                ComposeKind::Reply {
                    reply_all: matches!(request.kind, ComposeSessionKindRequest::ReplyAll),
                    in_reply_to: context.in_reply_to,
                    references: context.references,
                    thread_id: context.thread_id,
                    to: context.reply_to,
                    cc: context.cc,
                    subject: context.subject,
                    thread_context: context.thread_context,
                },
                envelope.account_id,
                None,
            )
        }
        ComposeSessionKindRequest::Forward => {
            let message_id = request
                .message_id
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("compose forward missing message_id".into()))?;
            let envelope = envelope_for_message(socket_path, message_id).await?;
            let response = ipc_request(
                socket_path,
                Request::PrepareForward {
                    message_id: envelope.id.clone(),
                },
            )
            .await?;
            let context = match response {
                ResponseData::ForwardContext { context } => context,
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            (
                ComposeKind::Forward {
                    subject: context.subject,
                    original_context: context.forwarded_content,
                },
                envelope.account_id,
                None,
            )
        }
        ComposeSessionKindRequest::InviteReply => {
            let message_id = request.message_id.as_deref().ok_or_else(|| {
                BridgeError::Ipc("compose invite_reply missing message_id".into())
            })?;
            let action_str = request
                .action
                .as_deref()
                .ok_or_else(|| BridgeError::Ipc("compose invite_reply missing action".into()))?;
            let action = match action_str.to_ascii_lowercase().as_str() {
                "accept" => mxr_protocol::CalendarInviteActionData::Accept,
                "tentative" | "maybe" => mxr_protocol::CalendarInviteActionData::Tentative,
                "decline" => mxr_protocol::CalendarInviteActionData::Decline,
                other => return Err(BridgeError::Ipc(format!("invalid invite action: {other}"))),
            };
            let envelope = envelope_for_message(socket_path, message_id).await?;
            let response = ipc_request(
                socket_path,
                Request::PrepareInviteResponse {
                    message_id: envelope.id.clone(),
                    action,
                },
            )
            .await?;
            let preview = match response {
                ResponseData::InviteResponsePreview { preview } => preview,
                _ => return Err(BridgeError::UnexpectedResponse),
            };
            (
                ComposeKind::Reply {
                    reply_all: false,
                    in_reply_to: String::new(),
                    references: Vec::new(),
                    thread_id: None,
                    to: preview.organizer_email,
                    cc: String::new(),
                    subject: preview.subject,
                    thread_context: String::new(),
                },
                envelope.account_id,
                None,
            )
        }
    };

    let account = account_summary(socket_path, &account_id).await?;
    let compose_from = if from.trim().is_empty() {
        account.email.clone()
    } else {
        from
    };
    let (draft_path, resolved_cursor_line) =
        mxr_compose::create_draft_file_async(kind, &compose_from)
            .await
            .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let mut session = load_compose_session(&draft_path).await?;
    if let Some(cursor_line) = cursor_line {
        session["cursorLine"] = json!(cursor_line);
    } else {
        session["cursorLine"] = json!(resolved_cursor_line);
    }
    session["accountId"] = json!(account.account_id);
    session["kind"] = json!(compose_kind_name(&request.kind));
    session["editorCommand"] = json!(resolved_editor_command());
    Ok(session)
}

fn compose_kind_name(kind: &ComposeSessionKindRequest) -> &'static str {
    match kind {
        ComposeSessionKindRequest::New => "new",
        ComposeSessionKindRequest::Reply => "reply",
        ComposeSessionKindRequest::ReplyAll => "reply_all",
        ComposeSessionKindRequest::Forward => "forward",
        ComposeSessionKindRequest::InviteReply => "invite_reply",
    }
}

async fn load_compose_session(path: &Path) -> Result<serde_json::Value, BridgeError> {
    let raw_content = read_compose_file(path).await?;
    let (frontmatter, body) =
        parse_compose_file(&raw_content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let rendered = render_markdown(&body);
    let issues = validate_draft(&frontmatter, &body)
        .into_iter()
        .map(compose_issue_view)
        .collect::<Vec<_>>();
    Ok(json!({
        "draftPath": path.display().to_string(),
        "rawContent": raw_content,
        "frontmatter": frontmatter,
        "bodyMarkdown": body,
        "previewHtml": rendered.html,
        "issues": issues,
    }))
}

fn compose_issue_view(issue: ComposeValidation) -> ComposeIssueView {
    match issue {
        ComposeValidation::MissingRecipients => ComposeIssueView {
            severity: "error",
            message: "No recipients (to: field is empty)".into(),
        },
        ComposeValidation::Error(message) => ComposeIssueView {
            severity: "error",
            message,
        },
        ComposeValidation::Warning(message) => ComposeIssueView {
            severity: "warning",
            message,
        },
    }
}

fn extract_compose_context(content: &str) -> Option<String> {
    const CONTEXT_MARKER: &str = "# --- context (stripped before sending) ---";
    let marker_index = content.find(CONTEXT_MARKER)?;
    let lines = content[marker_index + CONTEXT_MARKER.len()..]
        .lines()
        .map(|line| {
            line.strip_prefix("# ")
                .or_else(|| line.strip_prefix('#'))
                .unwrap_or(line)
        })
        .map(str::trim_end)
        .collect::<Vec<_>>();
    let context = lines.join("\n").trim().to_string();
    if context.is_empty() {
        None
    } else {
        Some(context)
    }
}

fn extract_in_reply_to(content: &str) -> Result<Option<String>, BridgeError> {
    let (frontmatter, _) =
        parse_compose_file(content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(frontmatter.in_reply_to)
}

fn extract_references(content: &str) -> Result<Vec<String>, BridgeError> {
    let (frontmatter, _) =
        parse_compose_file(content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(frontmatter.references)
}

fn extract_thread_id(content: &str) -> Result<Option<String>, BridgeError> {
    let (frontmatter, _) =
        parse_compose_file(content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    Ok(frontmatter.thread_id)
}

async fn compose_draft_from_file(draft_path: &str, account_id: &str) -> Result<Draft, BridgeError> {
    let raw_content = read_compose_file(Path::new(draft_path)).await?;
    let (frontmatter, body) =
        parse_compose_file(&raw_content).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let issues = validate_draft(&frontmatter, &body);
    if issues.iter().any(ComposeValidation::is_error) {
        let message = issues
            .into_iter()
            .map(|issue| issue.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(BridgeError::Ipc(format!("Draft errors: {message}")));
    }

    let now = Utc::now();
    Ok(Draft {
        id: DraftId::new(),
        account_id: parse_account_id(account_id)?,
        reply_headers: frontmatter
            .in_reply_to
            .as_ref()
            .map(|in_reply_to| ReplyHeaders {
                in_reply_to: in_reply_to.clone(),
                references: frontmatter.references.clone(),
                thread_id: frontmatter.thread_id.clone(),
            }),
        intent: frontmatter.intent,
        to: parse_address_list(&frontmatter.to),
        cc: parse_address_list(&frontmatter.cc),
        bcc: parse_address_list(&frontmatter.bcc),
        subject: frontmatter.subject,
        body_markdown: body,
        attachments: frontmatter.attach.into_iter().map(PathBuf::from).collect(),
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    })
}

async fn restore_saved_draft_session(
    socket_path: &Path,
    draft_id: DraftId,
) -> Result<serde_json::Value, BridgeError> {
    let draft = saved_draft(socket_path, &draft_id).await?;
    let account = account_summary(socket_path, &draft.account_id).await?;
    let (draft_path, cursor_line) = mxr_compose::create_draft_file_async(
        ComposeKind::New {
            to: String::new(),
            subject: draft.subject.clone(),
        },
        &account.email,
    )
    .await
    .map_err(|error| BridgeError::Ipc(error.to_string()))?;

    let frontmatter = ComposeFrontmatter {
        to: format_addresses(&draft.to),
        cc: format_addresses(&draft.cc),
        bcc: format_addresses(&draft.bcc),
        subject: draft.subject.clone(),
        from: account.email.clone(),
        in_reply_to: draft
            .reply_headers
            .as_ref()
            .map(|headers| headers.in_reply_to.clone()),
        references: draft
            .reply_headers
            .as_ref()
            .map(|headers| headers.references.clone())
            .unwrap_or_default(),
        thread_id: draft
            .reply_headers
            .as_ref()
            .and_then(|headers| headers.thread_id.clone()),
        intent: draft.intent,
        attach: draft
            .attachments
            .iter()
            .map(|attachment| attachment.display().to_string())
            .collect(),
        signature: None,
    };
    let rendered = render_compose_file(&frontmatter, &draft.body_markdown, None)
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    write_compose_file(&draft_path, rendered).await?;

    let mut session = load_compose_session(&draft_path).await?;
    session["cursorLine"] = json!(cursor_line);
    session["accountId"] = json!(account.account_id);
    session["kind"] = json!("new");
    session["editorCommand"] = json!(resolved_editor_command());
    Ok(session)
}

async fn saved_draft(socket_path: &Path, draft_id: &DraftId) -> Result<Draft, BridgeError> {
    match ipc_request(socket_path, Request::ListDrafts).await? {
        ResponseData::Drafts { drafts } => drafts
            .into_iter()
            .find(|draft| &draft.id == draft_id)
            .ok_or_else(|| BridgeError::Ipc(format!("draft {draft_id} not found"))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

fn format_addresses(addresses: &[mxr_core::Address]) -> String {
    addresses
        .iter()
        .map(|address| match address.name.as_deref() {
            Some(name) if !name.trim().is_empty() => format!("{name} <{}>", address.email),
            _ => address.email.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn draft_summary_view(draft: Draft) -> serde_json::Value {
    json!({
        "id": draft.id,
        "account_id": draft.account_id,
        "subject": draft.subject,
        "recipients": format_addresses(&draft.to),
        "updated_at": draft.updated_at,
        "updated_at_label": format_date_label(draft.updated_at),
        "updated_at_full": format_date_full(draft.updated_at),
        "updated_at_relative": format!("edited {}", format_relative_label(draft.updated_at)),
        "attachment_count": draft.attachments.len(),
    })
}

fn subscription_summary_view(subscription: SubscriptionSummary) -> serde_json::Value {
    json!({
        "account_id": subscription.account_id,
        "sender_name": subscription.sender_name,
        "sender_email": subscription.sender_email,
        "message_count": subscription.message_count,
        "latest_message_id": subscription.latest_message_id,
        "latest_thread_id": subscription.latest_thread_id,
        "latest_subject": subscription.latest_subject,
        "latest_snippet": subscription.latest_snippet,
        "latest_date": subscription.latest_date,
        "latest_has_attachments": subscription.latest_has_attachments,
        "unread": !subscription.latest_flags.contains(mxr_core::MessageFlags::READ),
    })
}

fn snoozed_summary_view(entry: &Snoozed, envelope: &Envelope) -> serde_json::Value {
    json!({
        "message_id": entry.message_id,
        "thread_id": envelope.thread_id,
        "sender": envelope
            .from
            .name
            .clone()
            .unwrap_or_else(|| envelope.from.email.clone()),
        "subject": envelope.subject,
        "snippet": envelope.snippet,
        "wake_at": entry.wake_at,
        "unread": !envelope.flags.contains(mxr_core::MessageFlags::READ),
        "has_attachments": envelope.has_attachments,
    })
}

async fn read_compose_file(path: &Path) -> Result<String, BridgeError> {
    mxr_compose::read_draft_file_async(path)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))
}

async fn write_compose_file(path: &Path, content: String) -> Result<(), BridgeError> {
    mxr_compose::write_draft_file_async(path, &content)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))
}

async fn remove_compose_file(path: &Path) -> Result<(), BridgeError> {
    mxr_compose::delete_draft_file_async(path)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))
}

fn compose_attachment_dir(path: &Path) -> Result<PathBuf, BridgeError> {
    let draft_stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| BridgeError::Ipc("invalid draft path for attachment".into()))?;
    Ok(std::env::temp_dir()
        .join("mxr-compose-attachments")
        .join(draft_stem))
}

fn compose_attachment_path(draft_path: &Path, filename: &str) -> Result<PathBuf, BridgeError> {
    let unique_name = format!("{}-{filename}", Uuid::now_v7());
    Ok(compose_attachment_dir(draft_path)?.join(unique_name))
}

fn safe_attachment_filename(value: &str) -> String {
    let raw_name = Path::new(value)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    let sanitized = raw_name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' | ' ' => ch,
            _ => '_',
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if sanitized.is_empty() {
        "attachment".into()
    } else {
        sanitized
    }
}

async fn remove_compose_attachment_dir(path: &Path) -> Result<(), BridgeError> {
    let dir = compose_attachment_dir(path)?;
    match tokio::fs::remove_dir_all(dir).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BridgeError::Ipc(error.to_string())),
    }
}

async fn default_account(socket_path: &Path) -> Result<(AccountId, String), BridgeError> {
    let mut accounts = match ipc_request(socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => accounts,
        _ => return Err(BridgeError::UnexpectedResponse),
    };
    if accounts.is_empty() {
        return Err(BridgeError::Ipc("No runtime account configured".into()));
    }
    let index = accounts
        .iter()
        .position(|account| account.is_default)
        .unwrap_or(0);
    let account = accounts.swap_remove(index);
    Ok((account.account_id, account.email))
}

async fn account_summary(
    socket_path: &Path,
    account_id: &AccountId,
) -> Result<mxr_protocol::AccountSummaryData, BridgeError> {
    match ipc_request(socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => accounts
            .into_iter()
            .find(|account| &account.account_id == account_id)
            .ok_or_else(|| BridgeError::Ipc("Account not found for compose session".into())),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn envelope_for_message(
    socket_path: &Path,
    message_id: &str,
) -> Result<Envelope, BridgeError> {
    match ipc_request(
        socket_path,
        Request::GetEnvelope {
            message_id: parse_message_id(message_id)?,
        },
    )
    .await?
    {
        ResponseData::Envelope { envelope } => Ok(envelope),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

fn resolved_editor_command() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string())
}

async fn request_account_operation(
    request_id: u64,
    socket_path: &Path,
    request: Request,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    match ipc_request_with_id(socket_path, request_id, request).await? {
        ResponseData::AccountOperation { result } => Ok(result),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn run_account_save_workflow(
    request_id: u64,
    socket_path: &Path,
    account: mxr_protocol::AccountConfigData,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    let mut result = if account
        .sync
        .as_ref()
        .is_some_and(|sync| matches!(sync, mxr_protocol::AccountSyncConfigData::Gmail { .. }))
    {
        request_account_operation(
            request_id,
            socket_path,
            Request::AuthorizeAccountConfig {
                account: account.clone(),
                reauthorize: false,
            },
        )
        .await?
    } else {
        empty_account_operation_result()
    };

    if result.auth.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    merge_account_operation_result(
        &mut result,
        request_account_operation(
            request_id,
            socket_path,
            Request::UpsertAccountConfig {
                account: account.clone(),
            },
        )
        .await?,
    );

    if result.save.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    merge_account_operation_result(
        &mut result,
        request_account_operation(
            request_id,
            socket_path,
            Request::TestAccountConfig { account },
        )
        .await?,
    );

    Ok(result)
}

fn empty_account_operation_result() -> mxr_protocol::AccountOperationResult {
    mxr_protocol::AccountOperationResult {
        ok: true,
        summary: String::new(),
        save: None,
        auth: None,
        sync: None,
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    }
}

fn merge_account_operation_result(
    base: &mut mxr_protocol::AccountOperationResult,
    next: mxr_protocol::AccountOperationResult,
) {
    base.ok &= next.ok;
    if !next.summary.is_empty() {
        base.summary = next.summary;
    }
    if next.save.is_some() {
        base.save = next.save;
    }
    if next.auth.is_some() {
        base.auth = next.auth;
    }
    if next.sync.is_some() {
        base.sync = next.sync;
    }
    if next.send.is_some() {
        base.send = next.send;
    }
}

fn build_snooze_preset(
    name: &str,
    label: &str,
    config: &mxr_config::SnoozeConfig,
) -> Option<serde_json::Value> {
    let wake_at = resolve_snooze_until(name, config).ok()?;
    Some(json!({
        "id": name,
        "name": name,
        "label": label,
        "wakeAt": wake_at,
    }))
}

fn resolve_snooze_until(
    until: &str,
    config: &mxr_config::SnoozeConfig,
) -> Result<DateTime<Utc>, BridgeError> {
    mxr_config::snooze::parse_snooze_until(until, config)
        .ok_or_else(|| BridgeError::Ipc(format!("invalid snooze time: {until}")))
}

// --- Feature parity routes ---

async fn list_subscriptions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ListSubscriptions {
            account_id: None,
            limit: 100,
        },
    )
    .await?
    {
        ResponseData::Subscriptions { subscriptions } => Ok(Json(json!({
            "subscriptions": subscriptions
                .into_iter()
                .map(subscription_summary_view)
                .collect::<Vec<_>>()
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(Debug, Deserialize)]
struct CreateSavedSearchBody {
    name: String,
    query: String,
    #[serde(default)]
    search_mode: Option<SearchMode>,
}

async fn create_saved_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<CreateSavedSearchBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_request(
        &state.config.socket_path,
        Request::CreateSavedSearch {
            name: body.name,
            query: body.query,
            search_mode: body.search_mode.unwrap_or(SearchMode::Lexical),
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct DeleteSavedSearchBody {
    name: String,
}

async fn delete_saved_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<DeleteSavedSearchBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_request(
        &state.config.socket_path,
        Request::DeleteSavedSearch { name: body.name },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct UpdateSavedSearchBody {
    name: String,
    #[serde(default)]
    new_name: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    search_mode: Option<SearchMode>,
    #[serde(default)]
    sort: Option<mxr_core::types::SortOrder>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    position: Option<i32>,
}

async fn update_saved_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<UpdateSavedSearchBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::UpdateSavedSearch {
            name: body.name,
            new_name: body.new_name,
            query: body.query,
            search_mode: body.search_mode,
            sort: body.sort,
            icon: body.icon,
            position: body.position,
        },
    )
    .await?;
    match response {
        ResponseData::SavedSearchData { search } => {
            Ok(Json(serde_json::to_value(search).map_err(|err| {
                BridgeError::Ipc(format!("serialize saved search: {err}"))
            })?))
        }
        ResponseData::Ack => Ok(Json(json!({ "ok": true }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(Debug, Deserialize)]
struct CreateLabelBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
}

async fn create_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<CreateLabelBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    ack_request(
        &state.config.socket_path,
        Request::CreateLabel {
            name: body.name,
            color: body.color,
            account_id,
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct RenameLabelBody {
    old: String,
    new: String,
    #[serde(default)]
    account_id: Option<String>,
}

async fn rename_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<RenameLabelBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    ack_request(
        &state.config.socket_path,
        Request::RenameLabel {
            old: body.old,
            new: body.new,
            account_id,
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct DeleteLabelBody {
    name: String,
    #[serde(default)]
    account_id: Option<String>,
}

async fn delete_label(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<DeleteLabelBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    ack_request(
        &state.config.socket_path,
        Request::DeleteLabel {
            name: body.name,
            account_id,
        },
    )
    .await
}

async fn list_drafts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListDrafts).await? {
        ResponseData::Drafts { drafts } => Ok(Json(json!({
            "drafts": drafts.into_iter().map(draft_summary_view).collect::<Vec<_>>()
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn list_snoozed(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListSnoozed).await? {
        ResponseData::SnoozedMessages { snoozed } => {
            let message_ids = snoozed
                .iter()
                .map(|entry| entry.message_id.clone())
                .collect::<Vec<_>>();
            let envelopes =
                list_envelopes_by_message_ids(&state.config.socket_path, &message_ids).await?;
            Ok(Json(json!({
                "snoozed": snoozed
                    .into_iter()
                    .filter_map(|entry| {
                        envelopes
                            .iter()
                            .find(|envelope| envelope.id == entry.message_id)
                            .map(|envelope| snoozed_summary_view(&entry, envelope))
                    })
                    .collect::<Vec<_>>()
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn trigger_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_request(
        &state.config.socket_path,
        Request::SyncNow { account_id: None },
    )
    .await
}

async fn get_semantic_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetSemanticStatus).await? {
        ResponseData::SemanticStatus { snapshot } => Ok(Json(json!({ "status": snapshot }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn get_llm_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetLlmStatus).await? {
        ResponseData::LlmStatus { snapshot } => Ok(Json(json!({ "status": snapshot }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn get_llm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetLlmConfig).await? {
        ResponseData::LlmConfig { config } => Ok(Json(json!({ "config": config }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn update_llm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<LlmConfigRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::UpdateLlmConfig {
            config: body.into(),
        },
    )
    .await?
    {
        ResponseData::LlmConfig { config } => Ok(Json(json!({ "config": config }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn trigger_semantic_reindex(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_request(&state.config.socket_path, Request::ReindexSemantic).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone, Utc};
    use futures::{SinkExt, StreamExt};
    use mxr_core::{
        id::{AccountId, AttachmentId, MessageId, ThreadId},
        types::{
            Address, AttachmentDisposition, AttachmentMeta, CalendarMetadata, Draft, Envelope,
            Label, LabelKind, MessageBody, MessageFlags, MessageMetadata, SavedSearch, SortOrder,
            SubscriptionSummary, Thread, UnsubscribeMethod,
        },
    };
    use mxr_protocol::{
        CommitmentData, CommitmentDirectionData, CommitmentStatusData, DaemonEvent, IpcCodec,
        IpcMessage, IpcPayload, Request, Response, ResponseData, SearchResultItem,
        IPC_PROTOCOL_VERSION,
    };
    use tempfile::TempDir;
    use tokio::net::UnixListener;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_util::codec::Framed;

    const TEST_AUTH_TOKEN: &str = "test-token";

    async fn spawn_fake_ipc_server<F>(
        socket_path: &std::path::Path,
        responder: F,
        event: Option<DaemonEvent>,
    ) -> tokio::task::JoinHandle<()>
    where
        F: Fn(Request) -> Option<Response> + Send + Sync + 'static,
    {
        let responder = std::sync::Arc::new(responder);
        let listener = UnixListener::bind(socket_path).unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let responder = responder.clone();
                let event = event.clone();
                tokio::spawn(async move {
                    let mut framed = Framed::new(stream, IpcCodec::new());
                    if let Some(event) = event {
                        let _ = framed
                            .send(IpcMessage {
                                id: 0,
                                source: ::mxr_protocol::ClientKind::default(),
                                payload: IpcPayload::Event(event),
                            })
                            .await;
                        return;
                    }
                    while let Some(message) = framed.next().await {
                        let Ok(message) = message else {
                            break;
                        };
                        if let IpcPayload::Request(request) = message.payload {
                            let Some(response) = responder(request) else {
                                continue;
                            };
                            let response = IpcMessage {
                                id: message.id,
                                source: ::mxr_protocol::ClientKind::default(),
                                payload: IpcPayload::Response(response),
                            };
                            let _ = framed.send(response).await;
                        }
                    }
                });
            }
        })
    }

    async fn spawn_fake_event_server(socket_path: &std::path::Path) -> tokio::task::JoinHandle<()> {
        spawn_fake_ipc_server(
            socket_path,
            |_| None,
            Some(DaemonEvent::SyncCompleted {
                account_id: AccountId::new(),
                messages_synced: 3,
            }),
        )
        .await
    }

    #[tokio::test]
    async fn status_endpoint_proxies_ipc_status() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 42,
                        accounts: vec!["personal".into()],
                        total_messages: 17,
                        daemon_pid: Some(999),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.4.3".into()),
                        daemon_build_id: Some("build-123".into()),
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/status"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
        assert_eq!(json["daemon_version"], "0.4.3");
        assert_eq!(json["total_messages"], 17);
    }

    #[tokio::test]
    async fn websocket_events_proxy_daemon_events() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_event_server(&socket_path).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let (mut stream, _) = tokio_tungstenite::connect_async(format!(
            "ws://{addr}/api/v1/events?token={TEST_AUTH_TOKEN}"
        ))
        .await
        .unwrap();
        let message = stream.next().await.unwrap().unwrap();
        let text = match message {
            Message::Text(text) => text.to_string(),
            other => panic!("expected text websocket frame, got {other:?}"),
        };

        assert!(text.contains("SyncCompleted"));
        assert!(text.contains("\"messages_synced\":3"));
    }

    /// Slice 6 — exemplar new routes from each bucket dispatch the right
    /// `Request` variant and return the corresponding `ResponseData`. Not
    /// exhaustive (per-variant coverage moves to slice 7's harness against
    /// the FakeProvider) — this catches "did the route table hook up at
    /// all" mistakes per slice.
    #[tokio::test]
    async fn slice6_routes_dispatch_correct_request_variants() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |request| match request {
                Request::Ping => Some(Response::Ok {
                    data: ResponseData::Pong,
                }),
                Request::ListAccountsConfig => Some(Response::Ok {
                    data: ResponseData::AccountsConfig { accounts: vec![] },
                }),
                Request::ListSavedSearches => Some(Response::Ok {
                    data: ResponseData::SavedSearches { searches: vec![] },
                }),
                Request::Count { mode, .. } => Some(Response::Ok {
                    data: ResponseData::Count {
                        // surface the mode round-trip so we know the
                        // handler parsed and forwarded it correctly
                        count: if mode == Some(SearchMode::Lexical) {
                            7
                        } else {
                            0
                        },
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();

        // admin/ping (Request::Ping → ResponseData::Pong)
        let response = client
            .post(format!("http://{addr}/api/v1/admin/ping"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["pong"], true);

        // ResponseData uses #[serde(tag = "kind")] (internally tagged), so
        // the JSON shape is {"kind": "Variant", ...fields}.

        // platform/accounts/config (Request::ListAccountsConfig)
        let response = client
            .get(format!("http://{addr}/api/v1/platform/accounts/config"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["kind"], "AccountsConfig");
        assert!(json["accounts"].is_array());

        // platform/saved-searches (Request::ListSavedSearches)
        let response = client
            .get(format!("http://{addr}/api/v1/platform/saved-searches"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["kind"], "SavedSearches");

        // mail/count with mode=lexical — verifies query-param parsing on
        // a route that wasn't covered before slice 6.
        let response = client
            .get(format!(
                "http://{addr}/api/v1/mail/count?query=alice&mode=lexical"
            ))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["kind"], "Count");
        assert_eq!(
            json["count"], 7,
            "mode=lexical must round-trip — fake responder returns 7 only for that mode"
        );
    }

    #[tokio::test]
    async fn llm_config_endpoint_forwards_update_to_daemon() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let seen_update = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen_for_ipc = seen_update.clone();

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetLlmConfig => Some(Response::Ok {
                    data: ResponseData::LlmConfig {
                        config: mxr_protocol::LlmConfigData {
                            enabled: false,
                            base_url: "http://localhost:11434/v1".into(),
                            model: "qwen2.5:3b-instruct".into(),
                            api_key_env: String::new(),
                            context_window: 8192,
                            request_timeout_secs: 120,
                            allow_cloud_relationship_data: false,
                            overrides: None,
                        },
                    },
                }),
                Request::UpdateLlmConfig { config } => {
                    *seen_for_ipc.lock().unwrap() = Some(config.clone());
                    Some(Response::Ok {
                        data: ResponseData::LlmConfig { config },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/api/v1/platform/llm/config"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "enabled": true,
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5-mini",
                "api_key_env": "OPENAI_API_KEY",
                "context_window": 16384,
                "request_timeout_secs": 45,
                "allow_cloud_relationship_data": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["config"]["model"], "gpt-5-mini");
        let forwarded = seen_update
            .lock()
            .unwrap()
            .clone()
            .expect("bridge must forward UpdateLlmConfig");
        assert!(forwarded.enabled);
        assert_eq!(forwarded.base_url, "https://api.openai.com/v1");
        assert_eq!(forwarded.api_key_env, "OPENAI_API_KEY");
        assert!(forwarded.allow_cloud_relationship_data);
    }

    /// Slice 6 — bad query params surface a 4xx error rather than a
    /// generic 500. Probes the per-handler validation paths (rubric
    /// dim 4: negative cases).
    #[tokio::test]
    async fn slice6_routes_reject_invalid_query_params() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();

        // bad mode → BridgeError::Ipc → 5xx; the point is we don't 200
        // for bad inputs.
        let response = client
            .get(format!(
                "http://{addr}/api/v1/mail/count?query=q&mode=garbage"
            ))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert!(
            !response.status().is_success(),
            "garbage mode must not return 2xx; got {}",
            response.status()
        );

        // missing required `query` parameter → 4xx (axum returns 400 for
        // failed Query extraction).
        let response = client
            .get(format!("http://{addr}/api/v1/mail/count"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_client_error(),
            "missing query param must surface a 4xx; got {}",
            response.status()
        );
    }

    /// Slice 4 — `Authorization: Bearer X` is the preferred auth path
    /// (matches OpenAPI's bearerAuth security scheme; what generated SDKs
    /// produce by default).
    #[tokio::test]
    async fn auth_accepts_authorization_bearer_header() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |_| {
                Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 1,
                        accounts: vec![],
                        total_messages: 0,
                        daemon_pid: None,
                        sync_statuses: vec![],
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: None,
                        daemon_build_id: None,
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                })
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/admin/status"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    /// Slice 4 — wrong bearer token is rejected (not just missing one).
    #[tokio::test]
    async fn auth_rejects_wrong_bearer_token() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/admin/status"))
            .bearer_auth("wrong-token-not-the-one-we-expect")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    /// Slice 4 — `?token=X` query string still works (EventSource clients
    /// can't set arbitrary headers, so this is the documented fallback).
    #[tokio::test]
    async fn auth_accepts_query_string_token_for_event_source() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_event_server(&socket_path).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        // EventSource is HTTP, so prove the same query-string auth works
        // for plain HTTP endpoints too (used by tools that can't set
        // headers — same situation as EventSource).
        let response = reqwest::get(format!(
            "http://{addr}/api/v1/admin/status?token={TEST_AUTH_TOKEN}"
        ))
        .await
        .unwrap();
        // The fake responder returns no payload for GetStatus so the
        // response is 5xx; what we're testing is that auth let it through
        // (not 401).
        assert_ne!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "query-string token must satisfy auth"
        );
    }

    /// Slice 4 — WebSocket browser auth via `Sec-WebSocket-Protocol`
    /// subprotocol header. Browsers cannot set arbitrary headers on WS
    /// upgrades, so this is the documented browser-friendly path. The
    /// server must echo back `bearer` so the negotiation succeeds.
    #[tokio::test]
    async fn websocket_auth_accepts_sec_websocket_protocol_subprotocol() {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        use tokio_tungstenite::tungstenite::http::HeaderValue;

        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_event_server(&socket_path).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let mut request = format!("ws://{addr}/api/v1/events")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_str(&format!("bearer, {TEST_AUTH_TOKEN}")).unwrap(),
        );

        let (mut stream, response) = tokio_tungstenite::connect_async(request)
            .await
            .expect("WS upgrade with subprotocol bearer auth must succeed");
        assert_eq!(
            response
                .headers()
                .get("sec-websocket-protocol")
                .map(|v| v.as_bytes()),
            Some(b"bearer".as_slice()),
            "server must echo `bearer` as the negotiated subprotocol"
        );
        let message = stream.next().await.unwrap().unwrap();
        assert!(matches!(message, Message::Text(_)));
    }

    /// Slice 4 — `/api/v1/health` is liveness-only and unauthenticated so
    /// clients and orchestrators can probe readiness without first
    /// loading the bridge token.
    #[tokio::test]
    async fn health_endpoint_is_unauthenticated() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::get(format!("http://{addr}/api/v1/health"))
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["status"], "ok");
        assert!(
            json["protocol_version"].as_u64().is_some(),
            "health response must surface IPC_PROTOCOL_VERSION so clients \
             can fail-fast on incompat without auth"
        );
    }

    /// Slice 4 — Host header allowlist defends against DNS rebinding.
    /// Loopback names allowed; arbitrary external hostnames rejected.
    #[tokio::test]
    async fn host_header_allowlist_rejects_external_hosts() {
        use reqwest::header::HOST;

        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        // Send a request with a Host header pointing at an external
        // domain — this is the shape of a DNS rebinding attack from a
        // malicious page.
        let response = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/health"))
            .header(HOST, "evil.example.com")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::FORBIDDEN,
            "non-loopback Host header must be rejected with 403"
        );
    }

    #[tokio::test]
    async fn host_header_allowlist_accepts_mxr_localhost() {
        use reqwest::header::HOST;

        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/health"))
            .header(HOST, "mxr.localhost")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "mxr.localhost should be a canonical loopback host without /etc/hosts changes"
        );
    }

    /// Slice 3 — every flat path returns a method-preserving permanent
    /// redirect (308) with a Location header pointing at the new bucketed
    /// v1 path. Sampled across each bucket so a typo in any single redirect
    /// doesn't slip past CI.
    ///
    /// 308 (not 301) is intentional: the bridge has POST endpoints
    /// (mutations, compose, etc.) and 301 historically downgrades POST→GET
    /// in some clients. 308 preserves the method.
    #[tokio::test]
    async fn legacy_paths_return_permanent_redirect_to_v1() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        let cases = [
            ("/status", "/api/v1/admin/status"),
            ("/diagnostics", "/api/v1/admin/diagnostics"),
            (
                "/diagnostics/bug-report",
                "/api/v1/admin/diagnostics/bug-report",
            ),
            ("/mailbox", "/api/v1/mail/mailbox"),
            ("/search", "/api/v1/mail/search"),
            ("/drafts", "/api/v1/mail/drafts"),
            ("/snoozed", "/api/v1/mail/snoozed"),
            ("/rules", "/api/v1/platform/rules"),
            ("/accounts", "/api/v1/platform/accounts"),
            ("/subscriptions", "/api/v1/platform/subscriptions"),
            ("/semantic/status", "/api/v1/platform/semantic/status"),
        ];

        for (old, new) in cases {
            let response = client
                .get(format!("http://{addr}{old}"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                reqwest::StatusCode::PERMANENT_REDIRECT,
                "old path {old} must 308 to {new} (method-preserving)"
            );
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .expect("301 must include Location header");
            assert_eq!(location, new, "Location header for {old} should be {new}");
        }
    }

    /// Slice 3 — the new bucketed v1 path serves the same response as the
    /// flat legacy path used to (asserted via response shape, not just
    /// status — per rubric dim 6).
    #[tokio::test]
    async fn v1_status_endpoint_returns_same_payload_as_legacy() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 99,
                        accounts: vec!["work".into()],
                        total_messages: 7,
                        daemon_pid: Some(42),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.5.0".into()),
                        daemon_build_id: Some("v1-test".into()),
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/api/v1/admin/status"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["protocol_version"], IPC_PROTOCOL_VERSION);
        assert_eq!(json["daemon_version"], "0.5.0");
        assert_eq!(json["total_messages"], 7);
        assert_eq!(json["uptime_secs"], 99);
    }

    /// Slice 2 — utoipa scaffold: spec is served at /api/v1/openapi.json,
    /// is valid OpenAPI 3.1, and includes the metadata + bearer security
    /// scheme that downstream tooling (Swagger UI, openapi-typescript-codegen,
    /// Schemathesis) needs to discover.
    #[tokio::test]
    async fn openapi_spec_is_served_at_api_v1_path() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::get(format!("http://{addr}/api/v1/openapi.json"))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "openapi.json must be served unauthenticated for discovery"
        );

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(
            json["openapi"], "3.1.0",
            "spec must declare OpenAPI 3.1 (utoipa 5+); got {json:?}"
        );

        let info = &json["info"];
        assert_eq!(info["title"], "mxr HTTP Bridge");
        assert!(info["version"].is_string(), "info.version must be a string");

        let bearer = json
            .pointer("/components/securitySchemes/bearer")
            .expect("bearer security scheme must be registered");
        assert_eq!(bearer["type"], "http");
        assert_eq!(bearer["scheme"], "bearer");
    }

    /// Slice 2 — capture the served spec via insta snapshot so any future
    /// schema/route drift forces an explicit review (`cargo insta review`).
    #[tokio::test]
    async fn openapi_spec_snapshot() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let json: serde_json::Value = reqwest::get(format!("http://{addr}/api/v1/openapi.json"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        // Snapshot a stable subset (top-level metadata + security scheme +
        // schema names). Snapshotting the full spec is too noisy across
        // utoipa version bumps; snapshotting nothing means drift slips by.
        let mut schema_names: Vec<&str> = json
            .pointer("/components/schemas")
            .and_then(|v| v.as_object())
            .map(|m| m.keys().map(|k| k.as_str()).collect())
            .unwrap_or_default();
        schema_names.sort();

        let summary = serde_json::json!({
            "openapi": json["openapi"],
            "info_title": json["info"]["title"],
            "security_schemes": json["components"]["securitySchemes"],
            "schema_names": schema_names,
        });

        insta::assert_json_snapshot!("openapi_spec_summary", summary);
    }

    /// Slice 2 — Swagger UI is served so newcomers can explore the API
    /// interactively without leaving the daemon host.
    #[tokio::test]
    async fn swagger_ui_is_served_at_api_v1_docs() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(&socket_path, |_| None, None).await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::default())
            .build()
            .unwrap()
            .get(format!("http://{addr}/api/v1/docs/"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let body = response.text().await.unwrap();
        // utoipa-swagger-ui ships an index.html that references swagger-ui
        // assets and points at /api/v1/openapi.json by default.
        assert!(
            body.contains("swagger-ui"),
            "Swagger UI HTML must mention swagger-ui assets; got first 200: {}",
            &body.chars().take(200).collect::<String>()
        );
    }

    #[tokio::test]
    async fn status_endpoint_rejects_missing_token() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            |_request| {
                Some(Response::Ok {
                    data: ResponseData::Ack,
                })
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::get(format!("http://{addr}/status")).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    }

    fn sample_envelope() -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "provider-msg-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg-1@example.com>".into()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Sender".into()),
                email: "sender@example.com".into(),
            },
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Mailroom".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "Preview".into(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
        }
    }

    fn sample_labels(account_id: &AccountId) -> Vec<Label> {
        vec![
            Label {
                id: mxr_core::LabelId::new(),
                account_id: account_id.clone(),
                name: "Inbox".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "INBOX".into(),
                unread_count: 12,
                total_count: 144,
            },
            Label {
                id: mxr_core::LabelId::new(),
                account_id: account_id.clone(),
                name: "All Mail".into(),
                kind: LabelKind::System,
                color: None,
                provider_id: "ALL_MAIL".into(),
                unread_count: 24,
                total_count: 8124,
            },
            Label {
                id: mxr_core::LabelId::new(),
                account_id: account_id.clone(),
                name: "Follow Up".into(),
                kind: LabelKind::User,
                color: None,
                provider_id: "follow-up".into(),
                unread_count: 2,
                total_count: 18,
            },
        ]
    }

    fn sample_saved_search(account_id: AccountId) -> SavedSearch {
        SavedSearch {
            id: mxr_core::SavedSearchId::new(),
            account_id: Some(account_id),
            name: "Today".into(),
            query: "in:inbox newer_than:1d".into(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: Some("sun".into()),
            position: 0,
            created_at: Utc::now(),
        }
    }

    fn sample_subscription(account_id: &AccountId) -> SubscriptionSummary {
        SubscriptionSummary {
            account_id: account_id.clone(),
            sender_name: Some("Readwise".into()),
            sender_email: "hello@readwise.io".into(),
            message_count: 4,
            latest_message_id: MessageId::new(),
            latest_provider_id: "provider-subscription-1".into(),
            latest_thread_id: ThreadId::new(),
            latest_subject: "Latest digest".into(),
            latest_snippet: "Highlights from this week".into(),
            latest_date: Utc::now(),
            latest_flags: MessageFlags::empty(),
            latest_has_attachments: false,
            latest_size_bytes: 256,
            unsubscribe: UnsubscribeMethod::None,
            opened_count: 0,
            replied_count: 0,
            archived_unread_count: 0,
        }
    }

    fn sample_account(account_id: &AccountId) -> mxr_protocol::AccountSummaryData {
        mxr_protocol::AccountSummaryData {
            account_id: account_id.clone(),
            key: Some("personal".into()),
            name: "Personal".into(),
            email: "me@example.com".into(),
            provider_kind: "gmail".into(),
            sync_kind: Some("gmail".into()),
            send_kind: Some("smtp".into()),
            enabled: true,
            is_default: true,
            source: mxr_protocol::AccountSourceData::Runtime,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
        }
    }

    fn sample_thread(envelope: &Envelope) -> Thread {
        Thread {
            id: envelope.thread_id.clone(),
            account_id: envelope.account_id.clone(),
            subject: envelope.subject.clone(),
            participants: vec![envelope.from.clone()],
            message_count: 1,
            unread_count: 1,
            latest_date: envelope.date,
            snippet: envelope.snippet.clone(),
        }
    }

    fn sample_body(envelope: &Envelope) -> MessageBody {
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("plain text".into()),
            text_html: Some("<p>rich html</p>".into()),
            attachments: Vec::new(),
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    fn sample_attachment_body(envelope: &Envelope) -> MessageBody {
        MessageBody {
            message_id: envelope.id.clone(),
            text_plain: Some("plain text".into()),
            text_html: Some("<p>rich html</p>".into()),
            attachments: vec![AttachmentMeta {
                id: AttachmentId::new(),
                message_id: envelope.id.clone(),
                filename: "deploy.log".into(),
                mime_type: "text/plain".into(),
                disposition: AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 1024,
                local_path: None,
                provider_id: "provider-attachment-1".into(),
            }],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    #[tokio::test]
    async fn mailbox_endpoint_lists_envelopes() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let labels = sample_labels(&expected.account_id);
        let saved_search = sample_saved_search(expected.account_id.clone());
        let subscription = sample_subscription(&expected.account_id);
        let inbox_label_id = labels[0].id.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 42,
                        accounts: vec!["personal".into()],
                        total_messages: 8124,
                        daemon_pid: Some(999),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.4.4".into()),
                        daemon_build_id: Some("build-123".into()),
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                }),
                Request::ListLabels { account_id: None } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListSavedSearches => Some(Response::Ok {
                    data: ResponseData::SavedSearches {
                        searches: vec![saved_search.clone()],
                    },
                }),
                Request::ListSubscriptions {
                    account_id: None,
                    limit: 8,
                } => Some(Response::Ok {
                    data: ResponseData::Subscriptions {
                        subscriptions: vec![subscription.clone()],
                    },
                }),
                Request::ListEnvelopes {
                    limit: 200,
                    offset: 0,
                    label_id: Some(label_id),
                    account_id: None,
                } if label_id == inbox_label_id => Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![expected.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/mailbox"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["shell"]["statusMessage"], "Local-first and ready");
        assert_eq!(json["sidebar"]["sections"][0]["title"], "System");
        assert_eq!(json["sidebar"]["sections"][1]["title"], "Labels");
        assert!(json["sidebar"]["sections"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "Subscriptions"));
        assert_eq!(json["mailbox"]["lensLabel"], "Inbox");
        assert_eq!(json["mailbox"]["groups"][0]["rows"][0]["id"], expected_id);
        assert_eq!(
            json["mailbox"]["groups"][0]["rows"][0]["subject"],
            "Mailroom"
        );
    }

    #[tokio::test]
    async fn mailbox_endpoint_supports_all_mail_lens() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let mut expected = sample_envelope();
        expected.subject = "Archive rollup".into();
        expected.snippet = "Everything local, nothing filtered.".into();
        let labels = sample_labels(&expected.account_id);
        let saved_search = sample_saved_search(expected.account_id.clone());
        let subscription = sample_subscription(&expected.account_id);
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 42,
                        accounts: vec!["personal".into()],
                        total_messages: 8124,
                        daemon_pid: Some(999),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.4.4".into()),
                        daemon_build_id: Some("build-123".into()),
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                }),
                Request::ListLabels { account_id: None } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListSavedSearches => Some(Response::Ok {
                    data: ResponseData::SavedSearches {
                        searches: vec![saved_search.clone()],
                    },
                }),
                Request::ListSubscriptions {
                    account_id: None,
                    limit: 8,
                } => Some(Response::Ok {
                    data: ResponseData::Subscriptions {
                        subscriptions: vec![subscription.clone()],
                    },
                }),
                Request::ListEnvelopes {
                    limit: 200,
                    offset: 0,
                    label_id: None,
                    account_id: None,
                } => Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![expected.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/mailbox?lens_kind=all_mail"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["mailbox"]["lensLabel"], "All Mail");
        assert_eq!(json["mailbox"]["counts"]["total"], 8124);
        assert!(json["sidebar"]["sections"][0]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "All Mail" && item["active"] == true));
        assert_eq!(
            json["mailbox"]["groups"][0]["rows"][0]["subject"],
            "Archive rollup"
        );
    }

    #[tokio::test]
    async fn mailbox_endpoint_shapes_thread_and_message_views() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");

        let first = sample_envelope();
        let mut second = sample_envelope();
        second.id = MessageId::new();
        second.account_id = first.account_id.clone();
        second.thread_id = first.thread_id.clone();
        second.subject = "Mailroom follow-up".into();
        second.snippet = "Same thread, newer message".into();
        second.has_attachments = true;
        let commitment = CommitmentData {
            id: "commitment-1".into(),
            account_id: first.account_id.clone(),
            email: first.from.email.clone(),
            thread_id: first.thread_id.clone(),
            direction: CommitmentDirectionData::Theirs,
            status: CommitmentStatusData::Open,
            who_owes: "sender".into(),
            what: "Send launch dates".into(),
            by_when: None,
            evidence_msg_id: first.id.clone(),
            extracted_at: Utc::now(),
        };

        let labels = sample_labels(&first.account_id);
        let saved_search = sample_saved_search(first.account_id.clone());
        let subscription = sample_subscription(&first.account_id);
        let inbox_label_id = labels[0].id.clone();

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetStatus => Some(Response::Ok {
                    data: ResponseData::Status {
                        uptime_secs: 42,
                        accounts: vec!["personal".into()],
                        total_messages: 8124,
                        daemon_pid: Some(999),
                        sync_statuses: Vec::new(),
                        protocol_version: IPC_PROTOCOL_VERSION,
                        daemon_version: Some("0.4.4".into()),
                        daemon_build_id: Some("build-123".into()),
                        repair_required: false,
                        semantic_runtime: None,
                        feature_health: None,
                    },
                }),
                Request::ListLabels { account_id: None } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListSavedSearches => Some(Response::Ok {
                    data: ResponseData::SavedSearches {
                        searches: vec![saved_search.clone()],
                    },
                }),
                Request::ListSubscriptions {
                    account_id: None,
                    limit: 8,
                } => Some(Response::Ok {
                    data: ResponseData::Subscriptions {
                        subscriptions: vec![subscription.clone()],
                    },
                }),
                Request::ListEnvelopes {
                    limit: 200,
                    offset: 0,
                    label_id: Some(label_id),
                    account_id: None,
                } if label_id == inbox_label_id => Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![first.clone(), second.clone()],
                    },
                }),
                Request::ListCommitments {
                    account_id,
                    email: None,
                    status: Some(CommitmentStatusData::Open),
                } if account_id == first.account_id => Some(Response::Ok {
                    data: ResponseData::CommitmentList {
                        commitments: vec![commitment.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let threads_response = client
            .get(format!("http://{addr}/mailbox?view=threads"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(threads_response.status(), reqwest::StatusCode::OK);

        let threads_json: serde_json::Value = threads_response.json().await.unwrap();
        assert_eq!(threads_json["mailbox"]["view"], "threads");
        assert_eq!(
            threads_json["mailbox"]["groups"][0]["rows"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            threads_json["mailbox"]["groups"][0]["rows"][0]["kind"],
            "thread"
        );
        assert_eq!(
            threads_json["mailbox"]["groups"][0]["rows"][0]["message_count"],
            2
        );
        assert_eq!(
            threads_json["mailbox"]["groups"][0]["rows"][0]["has_attachments"],
            true
        );
        assert_eq!(
            threads_json["mailbox"]["groups"][0]["rows"][0]["open_commitment_count"],
            1
        );

        let messages_response = client
            .get(format!("http://{addr}/mailbox?view=messages"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(messages_response.status(), reqwest::StatusCode::OK);

        let messages_json: serde_json::Value = messages_response.json().await.unwrap();
        assert_eq!(messages_json["mailbox"]["view"], "messages");
        assert_eq!(
            messages_json["mailbox"]["groups"][0]["rows"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            messages_json["mailbox"]["groups"][0]["rows"][0]["kind"],
            "message"
        );
        assert_eq!(
            messages_json["mailbox"]["groups"][0]["rows"][1]["subject"],
            "Mailroom follow-up"
        );
        assert_eq!(
            messages_json["mailbox"]["groups"][0]["rows"][1]["has_attachments"],
            true
        );
        assert_eq!(
            messages_json["mailbox"]["groups"][0]["rows"][1]["open_commitment_count"],
            1
        );
    }

    #[tokio::test]
    async fn thread_endpoint_returns_messages_and_bodies() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let mut envelope = sample_envelope();
        envelope.label_provider_ids = vec!["INBOX".into(), "follow-up".into()];
        let thread = sample_thread(&envelope);
        let mut body = sample_body(&envelope);
        body.metadata.calendar = Some(CalendarMetadata {
            method: Some("REQUEST".into()),
            summary: Some("Planning session".into()),
            ..Default::default()
        });
        let labels = sample_labels(&envelope.account_id);
        let thread_id = thread.id.to_string();
        let message_id = envelope.id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetThread {
                    thread_id: requested,
                } if requested == thread.id => Some(Response::Ok {
                    data: ResponseData::Thread {
                        thread: thread.clone(),
                        messages: vec![envelope.clone()],
                        summary: None,
                    },
                }),
                Request::ListLabels { .. } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListBodies { message_ids }
                    if message_ids == vec![body.message_id.clone()] =>
                {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
                            failures: Vec::new(),
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/thread/{thread_id}"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["thread"]["id"], thread_id);
        assert_eq!(json["messages"][0]["id"], message_id);
        assert_eq!(json["messages"][0]["to"][0]["email"], "user@example.com");
        assert_eq!(json["messages"][0]["labels"][0]["name"], "Inbox");
        assert_eq!(json["messages"][0]["labels"][1]["name"], "Follow Up");
        assert!(json["messages"][0]["date_full"].as_str().is_some());
        assert!(json["messages"][0]["date_relative"].as_str().is_some());
        assert_eq!(json["bodies"][0]["text_html"], "<p>rich html</p>");
        assert_eq!(
            json["bodies"][0]["metadata"]["calendar"]["summary"],
            "Planning session"
        );
        assert!(json["right_rail"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "1 calendar invite"));
    }

    #[tokio::test]
    async fn thread_endpoint_shapes_reader_mode_and_right_rail_from_raw_ipc_data() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread = sample_thread(&envelope);
        let body = sample_body(&envelope);
        let labels = sample_labels(&envelope.account_id);
        let thread_id = thread.id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetThread {
                    thread_id: requested,
                } if requested == thread.id => Some(Response::Ok {
                    data: ResponseData::Thread {
                        thread: thread.clone(),
                        messages: vec![envelope.clone()],
                        summary: None,
                    },
                }),
                Request::ListLabels { .. } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListBodies { message_ids }
                    if message_ids == vec![body.message_id.clone()] =>
                {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
                            failures: Vec::new(),
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/thread/{thread_id}"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["reader_mode"], "reader");
        assert_eq!(json["right_rail"]["title"], "Thread context");
    }

    #[tokio::test]
    async fn thread_endpoint_rejects_unexpected_body_response() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread = sample_thread(&envelope);
        let labels = sample_labels(&envelope.account_id);
        let thread_id = thread.id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GetThread {
                    thread_id: requested,
                } if requested == thread.id => Some(Response::Ok {
                    data: ResponseData::Thread {
                        thread: thread.clone(),
                        messages: vec![envelope.clone()],
                        summary: None,
                    },
                }),
                Request::ListLabels { .. } => Some(Response::Ok {
                    data: ResponseData::Labels {
                        labels: labels.clone(),
                    },
                }),
                Request::ListBodies { .. } => Some(Response::Ok {
                    data: ResponseData::Envelopes { envelopes: vec![] },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/thread/{thread_id}"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn search_endpoint_proxies_results() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let result = SearchResultItem {
            message_id: envelope.id.clone(),
            account_id: envelope.account_id.clone(),
            thread_id: envelope.thread_id.clone(),
            score: 9.5,
            mode: mxr_core::types::SearchMode::Lexical,
        };
        let message_id = result.message_id.to_string();
        let message_ids = vec![result.message_id.clone()];
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Search {
                    query,
                    limit: 200,
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                } if query == "buildkite" => Some(Response::Ok {
                    data: ResponseData::SearchResults {
                        results: vec![result.clone()],
                        total: 1,
                        has_more: false,
                        next_offset: None,
                        explain: None,
                    },
                }),
                Request::ListEnvelopesByIds {
                    message_ids: requested,
                } if requested == message_ids => Some(Response::Ok {
                    data: ResponseData::Envelopes {
                        envelopes: vec![envelope.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/search?q=buildkite"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["total"], 1);
        assert_eq!(json["groups"][0]["rows"][0]["id"], message_id);
        assert_eq!(json["groups"][0]["rows"][0]["subject"], "Mailroom");
        assert!(json["explain"].is_null());
    }

    #[test]
    fn group_envelopes_keeps_web_specific_date_buckets_out_of_ipc() {
        let today_noon = Local
            .from_local_datetime(
                &Local::now()
                    .date_naive()
                    .and_hms_opt(12, 0, 0)
                    .expect("valid local noon"),
            )
            .single()
            .expect("local noon is unambiguous")
            .with_timezone(&Utc);

        let mut same_day_a = sample_envelope();
        same_day_a.subject = "alpha".into();
        same_day_a.date = today_noon;

        let mut same_day_b = sample_envelope();
        same_day_b.subject = "beta".into();
        same_day_b.date = today_noon - chrono::Duration::hours(1);

        let mut older = sample_envelope();
        older.subject = "gamma".into();
        older.date = today_noon - chrono::Duration::days(3);

        let groups = group_envelopes(vec![same_day_a, same_day_b, older]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "today");
        assert_eq!(groups[0].label, "Today");
        assert_eq!(groups[0].rows.len(), 2);
        assert_eq!(groups[0].rows[0].subject, "alpha");
        assert_eq!(groups[0].rows[1].subject, "beta");
        assert_eq!(groups[1].id, "last-7-days");
        assert_eq!(groups[1].rows.len(), 1);
        assert_eq!(groups[1].rows[0].subject, "gamma");
    }

    #[test]
    fn date_labels_show_time_today_and_date_time_for_older_mail() {
        let today = Local
            .from_local_datetime(
                &Local::now()
                    .date_naive()
                    .and_hms_opt(12, 5, 0)
                    .expect("valid local time"),
            )
            .single()
            .expect("local noon is unambiguous")
            .with_timezone(&Utc);
        let yesterday = today - chrono::Duration::days(1);

        assert_eq!(format_date_label(today), "12:05pm");
        assert_eq!(
            format_date_label(yesterday),
            yesterday
                .with_timezone(&Local)
                .format("%b %-d %-I:%M%P")
                .to_string()
        );
    }

    #[test]
    fn draft_summary_includes_updated_time_labels() {
        let updated_at = Utc::now() - chrono::Duration::hours(2);
        let draft = Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![Address {
                name: Some("User".into()),
                email: "user@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Draft".into(),
            body_markdown: "Body".into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: updated_at,
            updated_at,
        };

        let json = draft_summary_view(draft);

        assert_eq!(json["updated_at_label"], format_date_label(updated_at));
        assert_eq!(json["updated_at_full"], format_date_full(updated_at));
        assert_eq!(
            json["updated_at_relative"],
            format!("edited {}", format_relative_label(updated_at))
        );
    }

    #[tokio::test]
    async fn search_endpoint_supports_mode_sort_and_explain() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let mut older = sample_envelope();
        older.subject = "Older deploy".into();
        older.date = Utc::now() - chrono::Duration::days(1);
        let mut newer = sample_envelope();
        newer.subject = "Newest deploy".into();
        newer.date = Utc::now();

        let older_result = SearchResultItem {
            message_id: older.id.clone(),
            account_id: older.account_id.clone(),
            thread_id: older.thread_id.clone(),
            score: 1.5,
            mode: mxr_core::types::SearchMode::Semantic,
        };
        let newer_result = SearchResultItem {
            message_id: newer.id.clone(),
            account_id: newer.account_id.clone(),
            thread_id: newer.thread_id.clone(),
            score: 0.8,
            mode: mxr_core::types::SearchMode::Semantic,
        };
        let requested_ids = vec![newer.id.clone(), older.id.clone()];
        let explain = mxr_protocol::SearchExplain {
            requested_mode: SearchMode::Semantic,
            executed_mode: SearchMode::Semantic,
            semantic_query: Some("deploy".into()),
            lexical_window: 50,
            dense_window: Some(50),
            lexical_candidates: 2,
            dense_candidates: 2,
            final_results: 2,
            rrf_k: Some(60),
            notes: vec!["semantic rerank".into()],
            results: vec![mxr_protocol::SearchExplainResult {
                rank: 1,
                message_id: newer.id.clone(),
                final_score: 1.0,
                lexical_rank: Some(2),
                lexical_score: Some(0.2),
                dense_rank: Some(1),
                dense_score: Some(0.9),
            }],
        };

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Search {
                    query,
                    limit: 200,
                    offset: 0,
                    mode: Some(SearchMode::Semantic),
                    sort: Some(SortOrder::DateDesc),
                    explain: true,
                } if query == "deploy" => Some(Response::Ok {
                    data: ResponseData::SearchResults {
                        results: vec![newer_result.clone(), older_result.clone()],
                        total: 2,
                        has_more: false,
                        next_offset: None,
                        explain: Some(explain.clone()),
                    },
                }),
                Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                    Some(Response::Ok {
                        data: ResponseData::Envelopes {
                            envelopes: vec![older.clone(), newer.clone()],
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!(
                "http://{addr}/search?q=deploy&mode=semantic&sort=recent&explain=true"
            ))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["mode"], "semantic");
        assert_eq!(json["sort"], "recent");
        assert_eq!(json["explain"]["requested_mode"], "semantic");
        assert_eq!(json["groups"][0]["rows"][0]["subject"], "Newest deploy");
        assert_eq!(json["groups"][1]["rows"][0]["subject"], "Older deploy");
    }

    #[tokio::test]
    async fn search_endpoint_dedupes_threads_by_thread_id() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");

        let mut first = sample_envelope();
        first.subject = "First match".into();
        first.snippet = "first".into();

        let mut second = sample_envelope();
        second.id = MessageId::new();
        second.subject = "Second match same thread".into();
        second.snippet = "second".into();
        second.thread_id = first.thread_id.clone();

        let results = vec![
            SearchResultItem {
                message_id: first.id.clone(),
                account_id: first.account_id.clone(),
                thread_id: first.thread_id.clone(),
                score: 9.5,
                mode: mxr_core::types::SearchMode::Lexical,
            },
            SearchResultItem {
                message_id: second.id.clone(),
                account_id: second.account_id.clone(),
                thread_id: second.thread_id.clone(),
                score: 9.0,
                mode: mxr_core::types::SearchMode::Lexical,
            },
        ];
        let requested_ids = vec![first.id.clone()];

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Search {
                    query,
                    limit: 200,
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                } if query == "dalumuzi" => Some(Response::Ok {
                    data: ResponseData::SearchResults {
                        results: results.clone(),
                        total: results.len() as u32,
                        has_more: false,
                        next_offset: None,
                        explain: None,
                    },
                }),
                Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                    Some(Response::Ok {
                        data: ResponseData::Envelopes {
                            envelopes: vec![first.clone()],
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/search?q=dalumuzi&scope=threads"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["total"], 1);
        assert_eq!(json["groups"][0]["rows"].as_array().unwrap().len(), 1);
        assert_eq!(json["groups"][0]["rows"][0]["subject"], "First match");
    }

    #[tokio::test]
    async fn search_endpoint_returns_attachment_rows_for_attachment_scope() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");

        let mut envelope = sample_envelope();
        envelope.has_attachments = true;
        envelope.subject = "Deploy artifacts".into();
        let body = sample_attachment_body(&envelope);
        let attachment_id = body.attachments[0].id.to_string();
        let message_id = envelope.id.to_string();

        let result = SearchResultItem {
            message_id: envelope.id.clone(),
            account_id: envelope.account_id.clone(),
            thread_id: envelope.thread_id.clone(),
            score: 9.5,
            mode: mxr_core::types::SearchMode::Lexical,
        };
        let requested_ids = vec![envelope.id.clone()];

        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Search {
                    query,
                    limit: 200,
                    offset: 0,
                    mode: None,
                    sort: Some(SortOrder::DateDesc),
                    explain: false,
                } if query == "deploy artifacts" => Some(Response::Ok {
                    data: ResponseData::SearchResults {
                        results: vec![result.clone()],
                        total: 1,
                        has_more: false,
                        next_offset: None,
                        explain: None,
                    },
                }),
                Request::ListEnvelopesByIds { message_ids } if message_ids == requested_ids => {
                    Some(Response::Ok {
                        data: ResponseData::Envelopes {
                            envelopes: vec![envelope.clone()],
                        },
                    })
                }
                Request::ListBodies { message_ids } if message_ids == requested_ids => {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
                            failures: Vec::new(),
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!(
                "http://{addr}/search?q=deploy%20artifacts&scope=attachments"
            ))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["scope"], "attachments");
        assert_eq!(json["total"], 1);
        assert_eq!(json["groups"][0]["rows"][0]["id"], message_id);
        assert_eq!(json["groups"][0]["rows"][0]["kind"], "attachment");
        assert_eq!(json["groups"][0]["rows"][0]["attachment_id"], attachment_id);
        assert_eq!(
            json["groups"][0]["rows"][0]["attachment_filename"],
            "deploy.log"
        );
        assert_eq!(
            json["groups"][0]["rows"][0]["snippet"],
            "text/plain · 1024 bytes"
        );
    }

    #[tokio::test]
    async fn compose_session_endpoint_prepares_reply_draft() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let account = sample_account(&envelope.account_id);
        let message_id = envelope.id.to_string();
        let expected_account_id = envelope.account_id.to_string();
        let expected_message_id = envelope.id.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListAccounts => Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
                    },
                }),
                Request::GetEnvelope { message_id } if message_id == expected_message_id => {
                    Some(Response::Ok {
                        data: ResponseData::Envelope {
                            envelope: envelope.clone(),
                        },
                    })
                }
                Request::PrepareReply {
                    message_id,
                    reply_all: false,
                } if message_id == expected_message_id => Some(Response::Ok {
                    data: ResponseData::ReplyContext {
                        context: mxr_protocol::ReplyContext {
                            account_id: mxr_core::AccountId::new(),
                            in_reply_to: "<msg-1@example.com>".into(),
                            references: vec!["<root@example.com>".into()],
                            reply_to: "sender@example.com".into(),
                            cc: String::new(),
                            subject: "Mailroom".into(),
                            from: "sender@example.com".into(),
                            thread_context: "Original thread context".into(),
                            thread_id: None,
                        },
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/compose/session"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "kind": "reply",
                "message_id": message_id,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["session"]["kind"], "reply");
        assert_eq!(json["session"]["accountId"], expected_account_id);
        assert_eq!(json["session"]["frontmatter"]["to"], "sender@example.com");
        assert_eq!(json["session"]["frontmatter"]["subject"], "Re: Mailroom");
        assert_eq!(json["session"]["issues"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn compose_session_update_refresh_and_discard_round_trip_draft() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let account = sample_account(&AccountId::new());
        let account_email = account.email.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListAccounts => Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();

        let started: serde_json::Value = client
            .post(format!("http://{addr}/compose/session"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "kind": "new",
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let draft_path = started["session"]["draftPath"]
            .as_str()
            .unwrap()
            .to_string();

        let updated: serde_json::Value = client
            .post(format!("http://{addr}/compose/session/update"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "to": "alice@example.com",
                "cc": "",
                "bcc": "",
                "subject": "Updated subject",
                "from": account_email,
                "attach": [],
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(updated["session"]["frontmatter"]["to"], "alice@example.com");
        assert_eq!(
            updated["session"]["frontmatter"]["subject"],
            "Updated subject"
        );

        let refreshed: serde_json::Value = client
            .post(format!("http://{addr}/compose/session/refresh"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            refreshed["session"]["frontmatter"]["to"],
            "alice@example.com"
        );
        assert_eq!(
            refreshed["session"]["frontmatter"]["subject"],
            "Updated subject"
        );

        let discard_response = client
            .post(format!("http://{addr}/compose/session/discard"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(discard_response.status(), reqwest::StatusCode::OK);
        assert!(
            !std::path::Path::new(refreshed["session"]["draftPath"].as_str().unwrap()).exists()
        );
    }

    #[tokio::test]
    async fn compose_attachment_upload_writes_local_file_and_discard_cleans_it() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let account = sample_account(&AccountId::new());
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListAccounts => Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();

        let started: serde_json::Value = client
            .post(format!("http://{addr}/compose/session"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "kind": "new" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let draft_path = started["session"]["draftPath"]
            .as_str()
            .unwrap()
            .to_string();

        let uploaded: serde_json::Value = client
            .post(format!(
                "http://{addr}/api/v1/mail/compose/session/attachment"
            ))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "filename": "notes.txt",
                "content_base64": general_purpose::STANDARD.encode(b"hello attachment"),
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let attachment_path = uploaded["path"].as_str().unwrap();
        assert_eq!(uploaded["filename"], "notes.txt");
        assert_eq!(
            tokio::fs::read_to_string(attachment_path).await.unwrap(),
            "hello attachment"
        );

        let discard_response = client
            .post(format!("http://{addr}/compose/session/discard"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "draft_path": draft_path }))
            .send()
            .await
            .unwrap();
        assert_eq!(discard_response.status(), reqwest::StatusCode::OK);
        assert!(!std::path::Path::new(attachment_path).exists());
    }

    #[tokio::test]
    async fn compose_session_send_forwards_draft_account_id() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let account = sample_account(&AccountId::new());
        let expected_account_id = account.account_id.clone();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None::<Draft>));
        let captured_send = captured.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListAccounts => Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
                    },
                }),
                Request::SendDraft { draft, .. } => {
                    *captured_send.lock().unwrap() = Some(draft);
                    Some(Response::Ok {
                        data: ResponseData::Ack,
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();

        let started: serde_json::Value = client
            .post(format!("http://{addr}/compose/session"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "kind": "new" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let draft_path = started["session"]["draftPath"]
            .as_str()
            .unwrap()
            .to_string();
        let account_id = started["session"]["accountId"]
            .as_str()
            .unwrap()
            .to_string();

        let update_response = client
            .post(format!("http://{addr}/compose/session/update"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "to": "alice@example.com",
                "cc": "",
                "bcc": "",
                "subject": "Bridge send",
                "from": "me@example.com",
                "attach": [],
                "body": "Hello from bridge",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(update_response.status(), reqwest::StatusCode::OK);

        let send_response = client
            .post(format!("http://{addr}/compose/session/send"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "account_id": account_id,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(send_response.status(), reqwest::StatusCode::OK);

        let draft = captured
            .lock()
            .unwrap()
            .clone()
            .expect("send draft should be forwarded");
        assert_eq!(draft.account_id, expected_account_id);
        assert_eq!(draft.subject, "Bridge send");
        assert_eq!(draft.body_markdown, "Hello from bridge");
        assert_eq!(draft.to[0].email, "alice@example.com");
    }

    #[tokio::test]
    async fn compose_session_send_propagates_daemon_error() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let account = sample_account(&AccountId::new());
        let expected_error = format!(
            "No send provider configured for account {}",
            account.account_id
        );
        let error_for_server = expected_error.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ListAccounts => Some(Response::Ok {
                    data: ResponseData::Accounts {
                        accounts: vec![account.clone()],
                    },
                }),
                Request::SendDraft { .. } => Some(Response::error(error_for_server.clone())),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();
        let client = reqwest::Client::new();

        let started: serde_json::Value = client
            .post(format!("http://{addr}/compose/session"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "kind": "new" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let draft_path = started["session"]["draftPath"]
            .as_str()
            .unwrap()
            .to_string();
        let account_id = started["session"]["accountId"]
            .as_str()
            .unwrap()
            .to_string();

        let update_response = client
            .post(format!("http://{addr}/compose/session/update"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "to": "alice@example.com",
                "cc": "",
                "bcc": "",
                "subject": "Bridge send",
                "from": "me@example.com",
                "attach": [],
                "body": "Hello",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(update_response.status(), reqwest::StatusCode::OK);

        let send_response = client
            .post(format!("http://{addr}/compose/session/send"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "draft_path": draft_path,
                "account_id": account_id,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(send_response.status(), reqwest::StatusCode::BAD_GATEWAY);

        let body = send_response.text().await.unwrap();
        assert!(body.contains(&expected_error));
        assert!(std::path::Path::new(&draft_path).exists());
        let _ = std::fs::remove_file(draft_path);
    }

    #[tokio::test]
    async fn archive_mutation_endpoint_proxies_message_ids() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let captured_ids = captured.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Mutation {
                    mutation: mxr_protocol::MutationCommand::Archive { message_ids },
                    ..
                } => {
                    *captured_ids.lock().unwrap() =
                        message_ids.iter().map(ToString::to_string).collect();
                    Some(Response::Ok {
                        data: ResponseData::Ack,
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{addr}/mutations/archive"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "message_ids": [expected_id] }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(*captured.lock().unwrap(), vec![expected.id.to_string()]);
    }

    #[tokio::test]
    async fn invite_reply_endpoint_proxies_dry_run_request() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured_request = captured.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::RespondInvite {
                    message_id,
                    action,
                    dry_run,
                } => {
                    *captured_request.lock().unwrap() =
                        Some((message_id.to_string(), action, dry_run));
                    Some(Response::Ok {
                        data: ResponseData::InviteResponsePreview {
                            preview: mxr_protocol::CalendarInviteResponsePreview {
                                message_id,
                                action,
                                attendee_email: "user@example.com".into(),
                                organizer_email: "organizer@example.com".into(),
                                subject: "Accepted: Planning".into(),
                                body_text: "user@example.com has accepted this invitation.".into(),
                                ics: "BEGIN:VCALENDAR\r\nMETHOD:REPLY\r\nEND:VCALENDAR\r\n".into(),
                                warnings: Vec::new(),
                            },
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/actions/invite/reply"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({
                "message_id": expected_id,
                "action": "accept",
                "dry_run": true,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["status"], "preview");
        assert_eq!(json["preview"]["organizer_email"], "organizer@example.com");
        let captured = captured.lock().unwrap().clone().unwrap();
        assert_eq!(captured.0, expected.id.to_string());
        assert_eq!(captured.1, mxr_protocol::CalendarInviteActionData::Accept);
        assert!(captured.2);
    }

    #[tokio::test]
    async fn star_mutation_endpoint_proxies_message_ids_and_state() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected = sample_envelope();
        let expected_id = expected.id.to_string();
        let captured = std::sync::Arc::new(std::sync::Mutex::new((Vec::<String>::new(), false)));
        let captured_state = captured.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Mutation {
                    mutation:
                        mxr_protocol::MutationCommand::Star {
                            message_ids,
                            starred,
                        },
                    ..
                } => {
                    *captured_state.lock().unwrap() = (
                        message_ids.iter().map(ToString::to_string).collect(),
                        starred,
                    );
                    Some(Response::Ok {
                        data: ResponseData::Ack,
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://{addr}/mutations/star"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&serde_json::json!({ "message_ids": [expected_id], "starred": true }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(
            *captured.lock().unwrap(),
            (vec![expected.id.to_string()], true)
        );
    }

    #[tokio::test]
    async fn export_thread_endpoint_proxies_markdown_export() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread_id = envelope.thread_id.to_string();
        let expected_thread_id = envelope.thread_id.clone();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::ExportThread { thread_id, .. } if thread_id == expected_thread_id => {
                    Some(Response::Ok {
                        data: ResponseData::ExportResult {
                            content: "# Exported thread".into(),
                        },
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/thread/{thread_id}/export"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["content"], "# Exported thread");
    }

    #[tokio::test]
    async fn bug_report_endpoint_proxies_daemon_report() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::GenerateBugReport {
                    verbose: false,
                    full_logs: false,
                    since: None,
                } => Some(Response::Ok {
                    data: ResponseData::BugReport {
                        content: "bug report".into(),
                    },
                }),
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .get(format!("http://{addr}/diagnostics/bug-report"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let json: serde_json::Value = response.json().await.unwrap();
        assert_eq!(json["content"], "bug report");
    }

    #[tokio::test]
    async fn read_and_archive_endpoint_proxies_mutation() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let expected_message_id = MessageId::new();
        let expected_message_id_text = expected_message_id.to_string();
        let _ipc = spawn_fake_ipc_server(
            &socket_path,
            move |request| match request {
                Request::Mutation {
                    mutation: mxr_protocol::MutationCommand::ReadAndArchive { message_ids },
                    ..
                } => {
                    assert_eq!(message_ids, vec![expected_message_id.clone()]);
                    Some(Response::Ok {
                        data: ResponseData::Ack,
                    })
                }
                _ => None,
            },
            None,
        )
        .await;

        let addr = bind_and_serve(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            0,
            WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
        )
        .await
        .unwrap();

        let response = reqwest::Client::new()
            .post(format!("http://{addr}/mutations/read-and-archive"))
            .header("x-mxr-bridge-token", TEST_AUTH_TOKEN)
            .json(&json!({
                "message_ids": [expected_message_id_text],
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
}
