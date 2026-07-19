#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests use panic and unwrap for direct fixture failures"
    )
)]

mod chrome;
mod envelope_list;
mod legacy;
mod middleware;
mod openapi;
mod request_types;
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
use mxr_client::{ClientError, IpcConnection};
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
use mxr_protocol::{IpcPayload, Request, ResponseData};
use request_types::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::net::TcpListener;
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
    /// loopback defaults. Empty by default; reserved for future
    /// non-loopback remote bridge mode.
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
mod admin_handlers;
mod auth;
mod router;
use admin_handlers::{diagnostics, generate_bug_report, shell, status};
use auth::*;
pub use router::{app, bind_and_serve, bind_listener, serve, DEFAULT_BRIDGE_PORT};
#[derive(Clone)]
struct AppState {
    config: WebServerConfig,
}

impl AppState {
    fn new(config: WebServerConfig) -> Self {
        Self { config }
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
    // Saved-search and subscription-overview lenses can't paginate: their IPC
    // variants (RunSavedSearch, ListSubscriptions) take no offset. A subscription
    // drilldown (sender_email present) runs through Request::Search, which does.
    let supports_pagination = matches!(
        lens.kind,
        MailboxLensKind::Inbox | MailboxLensKind::AllMail | MailboxLensKind::Label
    ) || (lens.kind == MailboxLensKind::Subscription
        && lens.sender_email.is_some());
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
    if scope == "triage" {
        return triage_response(state, query).await;
    }
    let thread_scope = scope == "threads";
    let attachment_scope = scope == "attachments";

    match ipc_request(
        &state.config.socket_path,
        Request::Search {
            query: query.q,
            limit: query.limit,
            offset: query.offset,
            account_id: None,
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

async fn triage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    triage_response(state, query).await
}

async fn triage_response(
    state: AppState,
    query: SearchQuery,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if query.q.trim().is_empty() {
        return Ok(Json(json!({
            "scope": "triage",
            "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
            "mode": query.mode.unwrap_or_default(),
            "total": 0,
            "has_more": false,
            "next_offset": serde_json::Value::Null,
            "groups": [],
            "llm_calls": 0,
        })));
    }

    let mode = query.mode;
    let response = ipc_request(
        &state.config.socket_path,
        Request::TriageSearch {
            query: query.q,
            limit: query.limit,
            offset: query.offset,
            account_id: None,
            mode,
            sort: Some(SortOrder::DateDesc),
        },
    )
    .await?;
    let ResponseData::TriageResults {
        mut messages,
        total,
        has_more,
        next_offset,
        llm_calls,
        prompt_version,
    } = response
    else {
        return Err(BridgeError::UnexpectedResponse);
    };

    if let Some(verdict) = query.verdict.as_deref() {
        let verdict = verdict.to_ascii_uppercase();
        messages.retain(|message| message.verdict_token == verdict);
    }
    if query.sort.as_deref() == Some("verdict") {
        messages.sort_by_key(|message| message.verdict);
    }

    let message_ids = messages
        .iter()
        .map(|message| message.message_id.clone())
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
            ResponseData::Envelopes { envelopes } => reorder_envelopes(envelopes, &message_ids),
            _ => return Err(BridgeError::UnexpectedResponse),
        }
    };
    let triage_by_message = messages
        .iter()
        .map(|message| (message.message_id.to_string(), message))
        .collect::<HashMap<_, _>>();
    let rows = envelopes
        .into_iter()
        .map(|envelope| {
            let mut row = message_row_view_with_labels(&envelope, &[]);
            if let Some(message) = triage_by_message.get(&envelope.id.to_string()) {
                row.triage_verdict = Some(message.verdict_token.clone());
                row.triage_reason = Some(message.reason.clone());
                row.triage_line = Some(message.verdict_line.clone());
            }
            (envelope.date, row)
        })
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "scope": "triage",
        "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
        "mode": mode.unwrap_or_default(),
        "total": total,
        "has_more": has_more,
        "next_offset": next_offset,
        "groups": group_row_views(rows),
        "llm_calls": llm_calls,
        "prompt_version": prompt_version,
    })))
}

async fn search_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    let group_by = query
        .group_by
        .unwrap_or(mxr_protocol::SearchAggregationGroupBy::From);
    if query.q.trim().is_empty() {
        return Ok(Json(json!({
            "query": query.q,
            "group_by": group_by.as_str(),
            "total": 0,
            "groups": [],
        })));
    }
    match ipc_request(
        &state.config.socket_path,
        Request::SearchAggregation {
            query: query.q,
            account_id: None,
            mode: query.mode,
            group_by,
            limit: Some(query.limit),
        },
    )
    .await?
    {
        ResponseData::SearchAggregation {
            query,
            group_by,
            total,
            groups,
        } => Ok(Json(json!({
            "query": query,
            "group_by": group_by.as_str(),
            "total": total,
            "groups": groups,
        }))),
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
            parent_exists = path.parent().is_some_and(std::path::Path::exists),
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
            override_safety_token: request.override_safety_token.clone(),
        },
    )
    .await
    {
        Ok(ResponseData::Ack | ResponseData::SendReceipt { .. }) => {
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
    remove_invite_reply_sidecar(Path::new(&request.draft_path)).await?;
    Ok(Json(json!({ "ok": true, "draft_id": draft_id })))
}

/// Run the pre-send safety gate against the current compose session
/// without sending. Mirrors the report `SendDraft` would enforce.
async fn check_compose_session_safety(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id).await?;
    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::CheckDraftSafety {
            draft,
            context: Default::default(),
        },
    )
    .await
    {
        Ok(ResponseData::DraftSafetyReportResponse { report }) => {
            Ok(Json(json!({ "report": report })))
        }
        Ok(_) => Err(BridgeError::UnexpectedResponse),
        Err(error) => Err(error),
    }
}

/// Suggest "maybe include" recipients for the current compose session.
async fn suggest_compose_collaborators(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<ComposeSessionSendRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let request_id = bridge_request_id(&headers);
    let draft = compose_draft_from_file(&request.draft_path, &request.account_id).await?;
    match ipc_request_with_id(
        &state.config.socket_path,
        request_id,
        Request::SuggestCollaborators { draft, limit: 5 },
    )
    .await
    {
        Ok(ResponseData::SuggestedCollaborators { suggestions }) => {
            Ok(Json(json!({ "suggestions": suggestions })))
        }
        Ok(_) => Err(BridgeError::UnexpectedResponse),
        Err(error) => Err(error),
    }
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
    remove_invite_reply_sidecar(Path::new(&request.draft_path)).await?;
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

async fn unsubscribe_purge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UnsubscribePurgeRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let account_id = request
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    match ipc_request(
        &state.config.socket_path,
        Request::UnsubscribePurge {
            address: request.address,
            account_id,
            dry_run: request.dry_run,
            archive_on_no_method: request.archive_on_no_method,
        },
    )
    .await?
    {
        ResponseData::UnsubscribePurgeResult { result } => Ok(Json(json!({
            "ok": !matches!(
                result.status,
                mxr_protocol::UnsubscribePurgeStatusData::Failed
                    | mxr_protocol::UnsubscribePurgeStatusData::NoMethod
            ),
            "result": result,
        }))),
        other => Ok(Json(
            json!({ "ok": false, "unexpected": format!("{other:?}") }),
        )),
    }
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

async fn route_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<RouteRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    ack_mutation(
        &state.config.socket_path,
        mxr_protocol::MutationCommand::Route {
            message_ids: parse_message_ids(&request.message_ids)?,
            to_label: request.to_label,
            from_queue_label: request.from_queue_label,
            archive: request.archive,
            dry_run: request.dry_run,
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
    if let Err(error) = ensure_authorized_with_query_token(
        &headers,
        auth.token.as_deref(),
        &state.config.auth_token,
    ) {
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
    // The bridge stays connection-per-request (its isolation model) but now
    // constructs the connection through `mxr-client`. `with_start_id` keeps the
    // externally-chosen correlation id on the wire so daemon logs still line up
    // with the bridge's `request_id`. The `ClientKind::Web` tag is preserved so
    // daemon activity attributes web traffic correctly.
    let mut connection = IpcConnection::connect(socket_path, mxr_protocol::ClientKind::Web)
        .await
        .map_err(map_bridge_error)?
        .with_start_id(request_id);
    connection.request(request).await.map_err(map_bridge_error)
}

/// Map a `mxr-client` failure onto the bridge's HTTP error vocabulary,
/// preserving the exact strings the bridge produced before (connect failures
/// carry the raw io-error text `bridge_error_is_missing_file` inspects; a clean
/// close stays "connection closed").
fn map_bridge_error(error: ClientError) -> BridgeError {
    match error {
        ClientError::Connect { source, .. } => BridgeError::Connect(source.to_string()),
        ClientError::Daemon { message, .. } => BridgeError::Ipc(message),
        ClientError::Closed => BridgeError::Ipc("connection closed".into()),
        ClientError::Io(source) => BridgeError::Ipc(source.to_string()),
        // A non-response frame kept mapping to UnexpectedResponse before, so
        // `bridge_error_kind` still reports "unexpected_response".
        ClientError::UnexpectedFrame {
            is_response: false, ..
        } => BridgeError::UnexpectedResponse,
        // A wrong-id response is unreachable now that `with_start_id` pins the
        // wire id (and the old code accepted any response id regardless, so
        // there is no prior behavior worth preserving here).
        ClientError::UnexpectedFrame {
            frame_id,
            expected_id,
            ..
        } => BridgeError::Ipc(format!(
            "unexpected response id {frame_id} while awaiting {expected_id}"
        )),
        ClientError::Timeout(duration) => BridgeError::Ipc(format!(
            "IPC request timed out after {} seconds",
            duration.as_secs()
        )),
        // Transport-level connect failure (generic-connector path). The bridge
        // dials via the path constructor, so this is unreachable here; the arm
        // keeps the match exhaustive.
        ClientError::Transport(error) => BridgeError::Connect(error.to_string()),
    }
}

async fn bridge_events(mut socket: WebSocket, socket_path: PathBuf) {
    let mut connection =
        match IpcConnection::connect(&socket_path, mxr_protocol::ClientKind::Web).await {
            Ok(connection) => connection,
            Err(error) => {
                // Preserve the pre-refactor payload: the bare io-error string,
                // not the wrapped `ClientError::Connect` display.
                let detail = match error {
                    ClientError::Connect { source, .. } => source.to_string(),
                    other => other.to_string(),
                };
                let _ = socket
                    .send(WebSocketMessage::Text(
                        serde_json::json!({ "error": detail }).to_string().into(),
                    ))
                    .await;
                return;
            }
        };

    while let Ok(message) = connection.next_event().await {
        let IpcPayload::Event(event) = message.payload else {
            continue;
        };
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
    // Set by the invite-with-comment arm so the iTIP REPLY payload (ICS,
    // PARTSTAT, source message) survives to send time; persisted alongside the
    // draft file so the stateless send request can rebuild the Draft's
    // `inline_calendar_reply` instead of sending a plain email with no ATTENDEE.
    let mut invite_reply: Option<mxr_core::types::InlineCalendarReply> = None;
    // Each arm yields the sender identity to seed: `New` uses the default
    // account's address; reply/forward/invite use the daemon-computed default
    // From *for the message's own account* (an owned address), never the
    // global default account's — otherwise a reply on a non-default account
    // would seed an address that account doesn't own and be rejected on send.
    let (kind, account_id, cursor_line, from_seed) = match request.kind {
        ComposeSessionKindRequest::New => (
            ComposeKind::New {
                to: request.to.unwrap_or_default(),
                subject: String::new(),
            },
            account_id,
            None::<usize>,
            from,
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
            let from_seed = context.from.clone();
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
                from_seed,
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
            let from_seed = context.from.clone();
            (
                ComposeKind::Forward {
                    subject: context.subject,
                    original_context: context.forwarded_content,
                },
                envelope.account_id,
                None,
                from_seed,
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
            // RSVP-with-comment sends From the matched ATTENDEE alias and must
            // carry the iTIP REPLY payload so From/ATTENDEE agree and the
            // PARTSTAT is updated after send (parity with the TUI path).
            let from_seed = preview.attendee_email.clone();
            invite_reply = Some(mxr_core::types::InlineCalendarReply {
                source_message_id: envelope.id.clone(),
                attendee_email: preview.attendee_email.clone(),
                partstat: match action {
                    mxr_protocol::CalendarInviteActionData::Accept => {
                        mxr_core::types::CalendarPartstat::Accepted
                    }
                    mxr_protocol::CalendarInviteActionData::Tentative => {
                        mxr_core::types::CalendarPartstat::Tentative
                    }
                    mxr_protocol::CalendarInviteActionData::Decline => {
                        mxr_core::types::CalendarPartstat::Declined
                    }
                },
                // Store only the trusted triple (source, attendee, partstat) —
                // the ICS is rebuilt server-side at the send choke point, so no
                // ICS bytes are persisted in (or trusted from) the sidecar.
                ics_body: String::new(),
            });
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
                from_seed,
            )
        }
    };

    let account = account_summary(socket_path, &account_id).await?;
    let compose_from = if from_seed.trim().is_empty() {
        account.email.clone()
    } else {
        from_seed
    };
    let (draft_path, resolved_cursor_line) =
        mxr_compose::create_draft_file_async(kind, &compose_from)
            .await
            .map_err(|error| BridgeError::Ipc(error.to_string()))?;
    if let Some(reply) = &invite_reply {
        write_invite_reply_sidecar(&draft_path, reply).await?;
    }
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
        from: mxr_compose::draft_codec::parse_from_field(&frontmatter.from)
            .map_err(|error| BridgeError::Ipc(error.to_string()))?,
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
        // Rebuild the iTIP REPLY payload for an invite-with-comment session so
        // the outbound builder emits the ATTENDEE part and the daemon updates
        // PARTSTAT after send (parity with the TUI path).
        inline_calendar_reply: read_invite_reply_sidecar(Path::new(draft_path)).await?,
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

/// Sidecar holding a compose session's iTIP REPLY payload, next to the draft
/// file (`…/mxr-draft-<id>.md.invite.json`). The compose file format has no
/// place for the ICS/PARTSTAT, so we persist it here and rebuild the Draft's
/// `inline_calendar_reply` at send time.
fn invite_reply_sidecar_path(draft_path: &Path) -> PathBuf {
    let mut name = draft_path.as_os_str().to_os_string();
    name.push(".invite.json");
    PathBuf::from(name)
}

async fn write_invite_reply_sidecar(
    draft_path: &Path,
    reply: &mxr_core::types::InlineCalendarReply,
) -> Result<(), BridgeError> {
    let json = serde_json::to_vec(reply).map_err(|error| BridgeError::Ipc(error.to_string()))?;
    tokio::fs::write(invite_reply_sidecar_path(draft_path), json)
        .await
        .map_err(|error| BridgeError::Ipc(error.to_string()))
}

async fn read_invite_reply_sidecar(
    draft_path: &Path,
) -> Result<Option<mxr_core::types::InlineCalendarReply>, BridgeError> {
    match tokio::fs::read(invite_reply_sidecar_path(draft_path)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| BridgeError::Ipc(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(BridgeError::Ipc(error.to_string())),
    }
}

async fn remove_invite_reply_sidecar(draft_path: &Path) -> Result<(), BridgeError> {
    match tokio::fs::remove_file(invite_reply_sidecar_path(draft_path)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BridgeError::Ipc(error.to_string())),
    }
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
            account_id: None,
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

#[derive(serde::Deserialize)]
struct InvitesQuery {
    token: Option<String>,
    limit: Option<u32>,
}

async fn list_invites(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<InvitesQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, q.token.as_deref(), &state.config.auth_token)?;
    let limit = q.limit.unwrap_or(200);
    match ipc_request(
        &state.config.socket_path,
        Request::ListInvites {
            account_id: None,
            limit,
        },
    )
    .await?
    {
        ResponseData::Invites { invites } => Ok(Json(json!({ "invites": invites }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(serde::Deserialize)]
struct DeliveriesQuery {
    token: Option<String>,
    filter: Option<String>,
}

async fn list_deliveries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<DeliveriesQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, q.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ListDeliveries {
            account_id: None,
            filter: q.filter,
        },
    )
    .await?
    {
        ResponseData::Deliveries { deliveries } => Ok(Json(json!({ "deliveries": deliveries }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn get_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(delivery_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GetDelivery {
            delivery_id: parse_delivery_id(&delivery_id)?,
        },
    )
    .await?
    {
        ResponseData::Delivery { delivery } => Ok(Json(json!({ "delivery": delivery }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn resolve_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(delivery_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ResolveDelivery {
            delivery_id: parse_delivery_id(&delivery_id)?,
        },
    )
    .await?
    {
        ResponseData::Delivery { delivery } => Ok(Json(json!({ "delivery": delivery }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn dismiss_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    AxumPath(delivery_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::DismissDelivery {
            delivery_id: parse_delivery_id(&delivery_id)?,
        },
    )
    .await?
    {
        ResponseData::Ack => Ok(Json(json!({ "ok": true }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(serde::Deserialize)]
struct ScanDeliveriesRequest {
    since_days: Option<u32>,
    #[serde(default)]
    dry_run: bool,
}

async fn scan_deliveries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(req): Json<ScanDeliveriesRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::ScanDeliveries {
            account_id: None,
            since_days: req.since_days,
            dry_run: req.dry_run,
        },
    )
    .await?
    {
        ResponseData::DeliveryScan { summary } => Ok(Json(json!({ "summary": summary }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

fn parse_delivery_id(value: &str) -> Result<mxr_core::DeliveryId, BridgeError> {
    Uuid::parse_str(value)
        .map(mxr_core::DeliveryId::from_uuid)
        .map_err(|_| BridgeError::Ipc(format!("invalid delivery id: {value}")))
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
            config: Box::new(body.into()),
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
mod tests;
