//! Relationship and archive-insight routes: the desk, owed replies, whois, cadence
//! watch/drift, send-time recommendation and ask-the-archive.
//!
//! Thin IPC passthroughs like `routes_v6`. Most of these daemon requests
//! require an account; when the `account` query/body field is absent the
//! bridge resolves the default account the same way compose does.

use super::routes_v6::passthrough;
use super::*;
use mxr_protocol::ArchiveAskFiltersData;

// Mirror the protocol's serde defaults (private to `mxr-protocol`) so an
// omitted `limit` behaves the same over HTTP as over IPC.
const DEFAULT_OWED_REPLY_LIMIT: u32 = 50;
const DEFAULT_DESK_LANE_LIMIT: u32 = 25;
const DEFAULT_WHOIS_LIMIT: u32 = 10;
const DEFAULT_ARCHIVE_ASK_LIMIT: u32 = 8;

async fn resolve_account(socket_path: &Path, raw: Option<&str>) -> Result<AccountId, BridgeError> {
    match raw {
        Some(raw) => parse_account_id(raw),
        None => default_account(socket_path).await.map(|(id, _)| id),
    }
}

#[derive(Debug, Deserialize)]
struct OwedQuery {
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    older_than_days: Option<u32>,
    #[serde(default)]
    within_days: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
}

async fn owed_replies(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OwedQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, query.account.as_deref()).await?;
    let response = ipc_request(
        socket_path,
        Request::ListOwedReplies {
            account_id,
            older_than_days: query.older_than_days,
            within_days: query.within_days,
            limit: query.limit.unwrap_or(DEFAULT_OWED_REPLY_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct DeskQuery {
    /// Omitted: every account, unlike the single-account insight routes.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    lane_limit: Option<u32>,
}

async fn desk(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DeskQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let account_id = query.account.as_deref().map(parse_account_id).transpose()?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::GetDesk {
            account_id,
            lane_limit: query.lane_limit.unwrap_or(DEFAULT_DESK_LANE_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct DeskThreadsBody {
    thread_ids: Vec<ThreadId>,
    #[serde(default)]
    dry_run: bool,
}

fn require_threads(body: &DeskThreadsBody) -> Result<(), BridgeError> {
    if body.thread_ids.is_empty() {
        return Err(BridgeError::BadRequest(
            "thread_ids must not be empty".into(),
        ));
    }
    Ok(())
}

async fn desk_dismiss(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeskThreadsBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    require_threads(&body)?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::DismissDeskThreads {
            thread_ids: body.thread_ids,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

async fn desk_restore(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeskThreadsBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    require_threads(&body)?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::RestoreDeskThreads {
            thread_ids: body.thread_ids,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct WhoisQuery {
    query: String,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

async fn whois(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WhoisQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    if query.query.trim().is_empty() {
        return Err(BridgeError::BadRequest(
            "whois query must not be empty".into(),
        ));
    }
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, query.account.as_deref()).await?;
    let response = ipc_request(
        socket_path,
        Request::ExplainEntity {
            account_id,
            query: query.query,
            limit: query.limit.unwrap_or(DEFAULT_WHOIS_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SendTimeQuery {
    /// Comma-separated recipient addresses.
    #[serde(alias = "to")]
    recipients: String,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    proposed_at: Option<DateTime<Utc>>,
}

async fn send_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SendTimeQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let recipients = query
        .recipients
        .split(',')
        .map(str::trim)
        .filter(|recipient| !recipient.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if recipients.is_empty() {
        return Err(BridgeError::BadRequest(
            "send-time needs at least one recipient".into(),
        ));
    }
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, query.account.as_deref()).await?;
    let response = ipc_request(
        socket_path,
        Request::SendTimeRecommendation {
            account_id,
            recipients,
            proposed_at: query.proposed_at,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ArchiveAskBody {
    question: String,
    #[serde(default)]
    filters: ArchiveAskFiltersData,
    #[serde(default)]
    limit: Option<u32>,
}

async fn archive_ask(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ArchiveAskBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    if body.question.trim().is_empty() {
        return Err(BridgeError::BadRequest("question must not be empty".into()));
    }
    let response = ipc_request(
        &state.config.socket_path,
        Request::ArchiveAsk {
            question: body.question,
            filters: body.filters,
            limit: body.limit.unwrap_or(DEFAULT_ARCHIVE_ASK_LIMIT),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AccountScopeQuery {
    #[serde(default, alias = "account_id")]
    account: Option<String>,
}

async fn cadence_drift(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountScopeQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, query.account.as_deref()).await?;
    passthrough(ipc_request(socket_path, Request::ListCadenceDrift { account_id }).await?)
}

async fn cadence_watch_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountScopeQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, query.account.as_deref()).await?;
    passthrough(ipc_request(socket_path, Request::ListCadenceWatch { account_id }).await?)
}

#[derive(Debug, Deserialize)]
struct WatchCadenceBody {
    #[serde(default, alias = "account")]
    account_id: Option<String>,
    email: String,
    #[serde(default)]
    expected_days: Option<f64>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    allow_list_sender: bool,
}

async fn cadence_watch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WatchCadenceBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, body.account_id.as_deref()).await?;
    ack_request(
        socket_path,
        Request::WatchCadence {
            account_id,
            email: body.email,
            expected_days: body.expected_days,
            note: body.note,
            allow_list_sender: body.allow_list_sender,
        },
    )
    .await
}

#[derive(Debug, Deserialize)]
struct UnwatchCadenceBody {
    #[serde(default, alias = "account")]
    account_id: Option<String>,
    email: String,
}

async fn cadence_unwatch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UnwatchCadenceBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let socket_path = &state.config.socket_path;
    let account_id = resolve_account(socket_path, body.account_id.as_deref()).await?;
    ack_request(
        socket_path,
        Request::UnwatchCadence {
            account_id,
            email: body.email,
        },
    )
    .await
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/owed", get(owed_replies))
        .route("/desk", get(desk))
        .route("/desk/dismiss", post(desk_dismiss))
        .route("/desk/restore", post(desk_restore))
        .route("/whois", get(whois))
        .route("/send-time", get(send_time))
        .route("/archive-ask", post(archive_ask))
}

pub(crate) fn extend_platform(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/analytics/cadence-drift", get(cadence_drift))
        .route(
            "/cadence/watch",
            get(cadence_watch_list).post(cadence_watch),
        )
        .route("/cadence/unwatch", post(cadence_unwatch))
}
