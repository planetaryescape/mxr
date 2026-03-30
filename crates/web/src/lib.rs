#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

mod chrome;
mod envelope_list;

use axum::{
    extract::ws::{Message as WebSocketMessage, WebSocket, WebSocketUpgrade},
    extract::{Path as AxumPath, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrome::{ack_mutation, ack_request, build_bridge_chrome, load_mailbox_selection};
use chrono::{DateTime, Local, TimeZone, Utc};
use envelope_list::{
    dedupe_search_results_by_thread, group_envelopes, message_row_view, reorder_envelopes,
    thread_reader_mode,
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
    types::{Draft, Envelope, Label, MessageBody, ReplyHeaders, SearchMode, SortOrder},
};
use mxr_mail_parse::parse_address_list;
use mxr_protocol::{IpcCodec, IpcMessage, IpcPayload, Request, ResponseData};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::net::TcpListener;
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use tower_http::cors::CorsLayer;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub socket_path: PathBuf,
    pub auth_token: String,
}

impl WebServerConfig {
    pub fn new(socket_path: PathBuf, auth_token: String) -> Self {
        Self {
            socket_path,
            auth_token,
        }
    }
}

pub fn app(_config: WebServerConfig) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/shell", get(shell))
        .route("/mailbox", get(mailbox))
        .route("/search", get(search))
        .route("/thread/{thread_id}", get(thread))
        .route("/thread/{thread_id}/export", get(export_thread))
        .route("/compose/session", post(start_compose_session))
        .route("/compose/session/refresh", post(refresh_compose_session))
        .route("/compose/session/update", post(update_compose_session))
        .route("/compose/session/send", post(send_compose_session))
        .route("/compose/session/save", post(save_compose_session))
        .route("/compose/session/discard", post(discard_compose_session))
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
        .route("/diagnostics", get(diagnostics))
        .route("/diagnostics/bug-report", get(generate_bug_report))
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
        .route("/attachments/open", post(open_attachment))
        .route("/attachments/download", post(download_attachment))
        .route("/subscriptions", get(list_subscriptions))
        .route("/saved-searches/create", post(create_saved_search))
        .route("/saved-searches/delete", post(delete_saved_search))
        .route("/labels/create", post(create_label))
        .route("/labels/rename", post(rename_label))
        .route("/labels/delete", post(delete_label))
        .route("/drafts", get(list_drafts))
        .route("/snoozed", get(list_snoozed))
        .route("/sync", post(trigger_sync))
        .route("/semantic/status", get(get_semantic_status))
        .route("/semantic/reindex", post(trigger_semantic_reindex))
        .route("/events", get(events))
        .with_state(AppState::new(_config))
        .layer(CorsLayer::permissive())
}

pub async fn serve(listener: TcpListener, config: WebServerConfig) -> std::io::Result<()> {
    axum::serve(listener, app(config)).await
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

fn ensure_authorized(
    headers: &HeaderMap,
    query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    let header_token = headers
        .get("x-mxr-bridge-token")
        .and_then(|value| value.to_str().ok());
    let provided = header_token.or(query_token);
    if provided == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
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
}

#[derive(Debug, Deserialize)]
struct ComposeSessionStartRequest {
    kind: ComposeSessionKindRequest,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ComposeSessionPathRequest {
    draft_path: String,
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
    Ok(Json(json!({
        "shell": chrome.shell,
        "sidebar": chrome.sidebar,
        "mailbox": {
            "lensLabel": mailbox.lens_label,
            "counts": mailbox.counts,
            "groups": group_envelopes(mailbox.envelopes),
        }
    })))
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
        ResponseData::Thread { thread, messages } => {
            let bodies = match ipc_request(
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
                ResponseData::Bodies { bodies } => bodies,
                _ => return Err(BridgeError::UnexpectedResponse),
            };

            let attachment_count = bodies
                .iter()
                .map(|body| body.attachments.len())
                .sum::<usize>();

            Ok(Json(json!({
                "thread": thread,
                "messages": messages.iter().map(message_row_view).collect::<Vec<_>>(),
                "bodies": bodies,
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
            "groups": [],
            "explain": serde_json::Value::Null,
        })));
    }

    let sort = match query.sort.as_deref() {
        Some("relevant") => SortOrder::Relevance,
        Some("oldest") => SortOrder::DateAsc,
        _ => SortOrder::DateDesc,
    };

    let thread_scope = query.scope.as_deref().unwrap_or("threads") == "threads";

    match ipc_request(
        &state.config.socket_path,
        Request::Search {
            query: query.q,
            limit: query.limit,
            offset: 0,
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
        } => {
            let effective_results = if thread_scope {
                dedupe_search_results_by_thread(results)
            } else {
                results
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

            Ok(Json(json!({
                "scope": query.scope.unwrap_or_else(|| "threads".to_string()),
                "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
                "mode": query.mode.unwrap_or_default(),
                "total": effective_results.len(),
                "has_more": has_more,
                "groups": group_envelopes(envelopes),
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
    let session = load_compose_session(Path::new(&request.draft_path))?;
    Ok(Json(json!({ "session": session })))
}

async fn update_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionUpdateRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let path = Path::new(&request.draft_path);
    let content =
        std::fs::read_to_string(path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let (_existing_frontmatter, file_body) =
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
        references: extract_references(&content)?,
        attach: request.attach,
    };
    let rendered = render_compose_file(&updated, &body, context.as_deref())
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    std::fs::write(path, rendered).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let session = load_compose_session(path)?;
    Ok(Json(json!({ "session": session })))
}

async fn send_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id)?;
    let _ = ack_request(&state.config.socket_path, Request::SendDraft { draft }).await?;
    let _ = std::fs::remove_file(&request.draft_path);
    Ok(Json(json!({ "ok": true })))
}

async fn save_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::SaveDraftToServer { draft },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn discard_compose_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionPathRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = std::fs::remove_file(&request.draft_path);
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
        build_snooze_preset("tomorrow", "Tomorrow morning", &config),
        build_snooze_preset("tonight", "Tonight", &config),
        build_snooze_preset("weekend", "Weekend", &config),
        build_snooze_preset("monday", "Next Monday", &config),
    ];
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
    match ipc_request(
        &state.config.socket_path,
        Request::TestAccountConfig { account },
    )
    .await?
    {
        ResponseData::AccountOperation { result } => Ok(Json(json!({ "result": result }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn upsert_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<mxr_protocol::AccountConfigData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let result = run_account_save_workflow(&state.config.socket_path, account).await?;
    Ok(Json(json!({ "result": result })))
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
    ws.on_upgrade(move |socket| bridge_events(socket, state.config.socket_path))
}

async fn ipc_request(socket_path: &Path, request: Request) -> Result<ResponseData, BridgeError> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|error| BridgeError::Connect(error.to_string()))?;
    let mut framed = Framed::new(stream, IpcCodec::new());
    let message = IpcMessage {
        id: 1,
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
                IpcPayload::Response(mxr_protocol::Response::Error { message }) => {
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
            request
                .to
                .map(|to| ComposeKind::NewWithTo { to })
                .unwrap_or(ComposeKind::New),
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
                    in_reply_to: context.in_reply_to,
                    references: context.references,
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
    };

    let account = account_summary(socket_path, &account_id).await?;
    let compose_from = if from.trim().is_empty() {
        account.email.clone()
    } else {
        from
    };
    let (draft_path, resolved_cursor_line) = mxr_compose::create_draft_file(kind, &compose_from)
        .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    let mut session = load_compose_session(&draft_path)?;
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
    }
}

fn load_compose_session(path: &Path) -> Result<serde_json::Value, BridgeError> {
    let raw_content =
        std::fs::read_to_string(path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
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

fn compose_draft_from_file(draft_path: &str, account_id: &str) -> Result<Draft, BridgeError> {
    let raw_content =
        std::fs::read_to_string(draft_path).map_err(|error| BridgeError::Ipc(error.to_string()))?;
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
            }),
        to: parse_address_list(&frontmatter.to),
        cc: parse_address_list(&frontmatter.cc),
        bcc: parse_address_list(&frontmatter.bcc),
        subject: frontmatter.subject,
        body_markdown: body,
        attachments: frontmatter.attach.into_iter().map(PathBuf::from).collect(),
        created_at: now,
        updated_at: now,
    })
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
    socket_path: &Path,
    request: Request,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    match ipc_request(socket_path, request).await? {
        ResponseData::AccountOperation { result } => Ok(result),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn run_account_save_workflow(
    socket_path: &Path,
    account: mxr_protocol::AccountConfigData,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    let mut result = if account
        .sync
        .as_ref()
        .is_some_and(|sync| matches!(sync, mxr_protocol::AccountSyncConfigData::Gmail { .. }))
    {
        request_account_operation(
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
        request_account_operation(socket_path, Request::TestAccountConfig { account }).await?,
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
) -> serde_json::Value {
    let wake_at = resolve_snooze_until(name, config).unwrap_or_else(|_| Utc::now());
    json!({
        "id": name,
        "label": label,
        "wakeAt": wake_at,
    })
}

fn resolve_snooze_until(
    until: &str,
    config: &mxr_config::SnoozeConfig,
) -> Result<DateTime<Utc>, BridgeError> {
    use chrono::{Datelike, Duration, Weekday};

    let now = Local::now();
    let lower = until.trim().to_ascii_lowercase();
    let wake_at = match lower.as_str() {
        "tomorrow" | "tomorrow_morning" => {
            let tomorrow = now.date_naive() + Duration::days(1);
            local_datetime_utc(tomorrow, u32::from(config.morning_hour), now.timezone())
        }
        "tonight" => {
            let today = now.date_naive();
            let tonight = local_datetime_utc(today, u32::from(config.evening_hour), now.timezone());
            if tonight <= Utc::now() {
                tonight + Duration::days(1)
            } else {
                tonight
            }
        }
        "weekend" => {
            let target_day = match config.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            };
            let days_until = (target_day.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until == 0 { 7 } else { days_until };
            let weekend = now.date_naive() + Duration::days(days);
            local_datetime_utc(weekend, u32::from(config.weekend_hour), now.timezone())
        }
        "monday" | "next_monday" => {
            let days_until_monday = (Weekday::Mon.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until_monday == 0 {
                7
            } else {
                days_until_monday
            };
            let monday = now.date_naive() + chrono::Duration::days(days);
            local_datetime_utc(monday, u32::from(config.morning_hour), now.timezone())
        }
        _ => DateTime::parse_from_rfc3339(until)
            .map_err(|_| BridgeError::Ipc(format!("invalid snooze time: {until}")))?
            .with_timezone(&Utc),
    };
    Ok(wake_at)
}

fn local_datetime_utc(date: chrono::NaiveDate, hour: u32, timezone: Local) -> DateTime<Utc> {
    let time = chrono::NaiveTime::from_hms_opt(hour, 0, 0).expect("validated snooze hour");
    let candidate = date.and_time(time);
    timezone
        .from_local_datetime(&candidate)
        .single()
        .or_else(|| timezone.from_local_datetime(&candidate).earliest())
        .or_else(|| timezone.from_local_datetime(&candidate).latest())
        .expect("snooze local datetime should resolve")
        .with_timezone(&Utc)
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
        ResponseData::Subscriptions { subscriptions } => {
            Ok(Json(json!({ "subscriptions": subscriptions })))
        }
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
        ResponseData::Drafts { drafts } => Ok(Json(json!({ "drafts": drafts }))),
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
        ResponseData::SnoozedMessages { snoozed } => Ok(Json(json!({ "snoozed": snoozed }))),
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
    use chrono::Utc;
    use futures::{SinkExt, StreamExt};
    use mxr_core::{
        id::{AccountId, MessageId, ThreadId},
        types::{
            Address, Envelope, Label, LabelKind, MessageBody, MessageFlags, MessageMetadata,
            SavedSearch, SortOrder, SubscriptionSummary, Thread, UnsubscribeMethod,
        },
    };
    use mxr_protocol::{
        DaemonEvent, IpcCodec, IpcMessage, IpcPayload, Request, Response, ResponseData,
        SearchResultItem, IPC_PROTOCOL_VERSION,
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

        let (mut stream, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/events?token={TEST_AUTH_TOKEN}"))
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
    async fn thread_endpoint_returns_messages_and_bodies() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread = sample_thread(&envelope);
        let body = sample_body(&envelope);
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
                    },
                }),
                Request::ListBodies { message_ids }
                    if message_ids == vec![body.message_id.clone()] =>
                {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
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
        assert_eq!(json["bodies"][0]["text_html"], "<p>rich html</p>");
    }

    #[tokio::test]
    async fn thread_endpoint_shapes_reader_mode_and_right_rail_from_raw_ipc_data() {
        let temp = TempDir::new().unwrap();
        let socket_path = temp.path().join("mxr.sock");
        let envelope = sample_envelope();
        let thread = sample_thread(&envelope);
        let body = sample_body(&envelope);
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
                    },
                }),
                Request::ListBodies { message_ids }
                    if message_ids == vec![body.message_id.clone()] =>
                {
                    Some(Response::Ok {
                        data: ResponseData::Bodies {
                            bodies: vec![body.clone()],
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
                        has_more: false,
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
        let mut same_day_a = sample_envelope();
        same_day_a.subject = "alpha".into();
        same_day_a.date = Utc::now();

        let mut same_day_b = sample_envelope();
        same_day_b.subject = "beta".into();
        same_day_b.date = Utc::now() - chrono::Duration::hours(1);

        let mut older = sample_envelope();
        older.subject = "gamma".into();
        older.date = Utc::now() - chrono::Duration::days(3);

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
                        has_more: false,
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
                        has_more: false,
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
                            in_reply_to: "<msg-1@example.com>".into(),
                            references: vec!["<root@example.com>".into()],
                            reply_to: "sender@example.com".into(),
                            cc: String::new(),
                            subject: "Mailroom".into(),
                            from: "sender@example.com".into(),
                            thread_context: "Original thread context".into(),
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
                Request::Mutation(mxr_protocol::MutationCommand::Archive { message_ids }) => {
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
                Request::Mutation(mxr_protocol::MutationCommand::Star {
                    message_ids,
                    starred,
                }) => {
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
                Request::Mutation(mxr_protocol::MutationCommand::ReadAndArchive {
                    message_ids,
                }) => {
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
