//! Slice 6 — routes for the `Request` variants that didn't have an HTTP
//! surface in v0.4.x. Per CLAUDE.md "wire both clients or wire neither":
//! every protocol variant gets a route so HTTP clients have full parity
//! with the TUI/CLI.
//!
//! Handlers proxy the IPC `Request` to the daemon and return the matching
//! `ResponseData` variant as JSON. They're deliberately thin: shaping
//! belongs in clients (per the IPC bucket rules), and the OpenAPI schema
//! published in slice 2 already documents the ResponseData wire format.
//!
//! Long-running operations (`RebuildAnalytics`, `Unsubscribe`, semantic
//! reindex) emit `OperationStarted/Progress/Completed` events on the
//! WebSocket stream — see slice 7 integration tests for the contract.

use crate::{ensure_authorized, ipc_request, AppState, AuthQuery, BridgeError};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{delete, get, post},
    Json, Router,
};
use mxr_core::{
    id::{AccountId, DraftId, MessageId, ThreadId},
    types::{
        Address, Draft, ExportFormat, MessageFlags, ResponseTimeDirection, SemanticProfile,
        StaleBallInCourt, StorageGroupBy,
    },
    SearchMode,
};
use mxr_protocol::{
    AccountConfigData, CommitmentStatusData, DraftLengthHintData, DraftRefineKnobsData, Request,
    ResponseData, ScreenerDispositionData, SignatureContextData, VoiceRegisterData,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::str::FromStr;

// ---------------------------------------------------------------------------
// query helpers

fn parse_account_id(raw: &str) -> Result<AccountId, BridgeError> {
    AccountId::from_str(raw).map_err(|err| BridgeError::Ipc(format!("invalid account_id: {err}")))
}

async fn dispatch(
    state: &AppState,
    headers: &HeaderMap,
    token_query: Option<&str>,
    request: Request,
) -> Result<ResponseData, BridgeError> {
    ensure_authorized(headers, token_query, &state.config.auth_token)?;
    ipc_request(&state.config.socket_path, request).await
}

/// Pass through the raw ResponseData JSON for variants where the bridge
/// doesn't add shape on top of what the daemon already produces. The
/// OpenAPI spec from slice 2 already documents the variant layouts.
fn passthrough(response: ResponseData) -> Result<Json<Value>, BridgeError> {
    serde_json::to_value(&response)
        .map(Json)
        .map_err(|err| BridgeError::Ipc(format!("response serialize: {err}")))
}

// ---------------------------------------------------------------------------
// admin

async fn ping(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(&state, &headers, auth.token.as_deref(), Request::Ping).await?;
    match response {
        ResponseData::Pong => Ok(Json(json!({ "pong": true }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn shutdown(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(&state, &headers, auth.token.as_deref(), Request::Shutdown).await?;
    match response {
        ResponseData::Ack => Ok(Json(json!({ "shutdown": "scheduled" }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

#[derive(Debug, Deserialize, Default)]
struct EventsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default = "default_log_limit")]
    limit: u32,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    category_prefix: Option<String>,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    until: Option<i64>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    offset: u32,
}

fn default_log_limit() -> u32 {
    200
}

async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListEvents {
            limit: query.limit,
            level: query.level,
            category: query.category,
            category_prefix: query.category_prefix,
            since: query.since,
            until: query.until,
            search: query.search,
            offset: query.offset,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize, Default)]
struct LogsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default = "default_log_limit")]
    limit: u32,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    search: Option<String>,
}

async fn get_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LogsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetLogs {
            limit: query.limit,
            level: query.level,
            search: query.search,
        },
    )
    .await?;
    passthrough(response)
}

async fn list_event_categories(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenOnlyQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListEventCategories,
    )
    .await?;
    passthrough(response)
}

async fn count_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::CountEvents {
            level: query.level,
            category: query.category,
            category_prefix: query.category_prefix,
            since: query.since,
            until: query.until,
            search: query.search,
        },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// platform — analytics

#[derive(Debug, Deserialize, Default)]
struct WrappedQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    since_unix: i64,
    until_unix: i64,
    #[serde(default = "default_wrapped_label")]
    label: String,
}

fn default_wrapped_label() -> String {
    "wrapped".into()
}

async fn analytics_wrapped(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WrappedQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::Wrapped {
            account_id: account,
            since_unix: query.since_unix,
            until_unix: query.until_unix,
            label: query.label,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct StorageBreakdownQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    group_by: Option<String>,
    #[serde(default = "default_breakdown_limit")]
    limit: u32,
}

fn default_breakdown_limit() -> u32 {
    50
}

async fn analytics_storage_breakdown(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StorageBreakdownQuery>,
) -> Result<Json<Value>, BridgeError> {
    let group_by = match query.group_by.as_deref() {
        Some("sender") | None => StorageGroupBy::Sender,
        Some("mimetype") | Some("mime") => StorageGroupBy::Mimetype,
        Some("label") => StorageGroupBy::Label,
        Some(other) => {
            return Err(BridgeError::Ipc(format!("unknown group_by={other}")));
        }
    };
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListStorageBreakdown {
            account_id: account,
            group_by,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct LargestMessagesQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    since_days: Option<u32>,
    #[serde(default = "default_breakdown_limit")]
    limit: u32,
}

async fn analytics_largest_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LargestMessagesQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListLargestMessages {
            account_id: account,
            since_days: query.since_days,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct StaleThreadsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default = "default_perspective")]
    perspective: String,
    #[serde(default = "default_older_than")]
    older_than_days: u32,
    #[serde(default = "default_within_days")]
    within_days: u32,
    #[serde(default = "default_breakdown_limit")]
    limit: u32,
}

fn default_perspective() -> String {
    "user".into()
}
fn default_older_than() -> u32 {
    14
}
fn default_within_days() -> u32 {
    180
}

async fn analytics_stale_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StaleThreadsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let perspective = match query.perspective.as_str() {
        "mine" | "user" => StaleBallInCourt::Mine,
        "theirs" | "counterparty" => StaleBallInCourt::Theirs,
        other => {
            return Err(BridgeError::Ipc(format!("unknown perspective={other}")));
        }
    };
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListStaleThreads {
            account_id: account,
            perspective,
            older_than_days: query.older_than_days,
            within_days: query.within_days,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ContactAsymmetryQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default = "default_min_inbound")]
    min_inbound: u32,
    #[serde(default = "default_breakdown_limit")]
    limit: u32,
}

fn default_min_inbound() -> u32 {
    5
}

async fn analytics_contact_asymmetry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ContactAsymmetryQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListContactAsymmetry {
            account_id: account,
            min_inbound: query.min_inbound,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ContactDecayQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default = "default_threshold_days")]
    threshold_days: u32,
    #[serde(default = "default_max_lookback")]
    max_lookback_days: u32,
    #[serde(default = "default_breakdown_limit")]
    limit: u32,
}

fn default_threshold_days() -> u32 {
    30
}
fn default_max_lookback() -> u32 {
    365
}

async fn analytics_contact_decay(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ContactDecayQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListContactDecay {
            account_id: account,
            threshold_days: query.threshold_days,
            max_lookback_days: query.max_lookback_days,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ResponseTimeQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    counterparty: Option<String>,
    #[serde(default)]
    since_days: Option<u32>,
}

async fn analytics_response_time(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ResponseTimeQuery>,
) -> Result<Json<Value>, BridgeError> {
    let direction = match query.direction.as_deref() {
        None | Some("they_replied") | Some("they-replied") | Some("outgoing") => {
            ResponseTimeDirection::TheyReplied
        }
        Some("i_replied") | Some("i-replied") | Some("incoming") => ResponseTimeDirection::IReplied,
        Some(other) => {
            return Err(BridgeError::Ipc(format!("unknown direction={other}")));
        }
    };
    let account = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListResponseTime {
            account_id: account,
            direction,
            counterparty: query.counterparty,
            since_days: query.since_days,
        },
    )
    .await?;
    passthrough(response)
}

async fn analytics_refresh_contacts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RefreshContacts,
    )
    .await?;
    passthrough(response)
}

async fn analytics_rebuild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RebuildAnalytics,
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// platform — saved searches (list + run)

async fn list_saved_searches(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListSavedSearches,
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct RunSavedSearchBody {
    name: String,
    #[serde(default = "default_run_limit")]
    limit: u32,
}

fn default_run_limit() -> u32 {
    50
}

async fn run_saved_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<RunSavedSearchBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RunSavedSearch {
            name: body.name,
            limit: body.limit,
        },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// platform — account lifecycle

async fn list_accounts_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListAccountsConfig,
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize, Default)]
struct RemoveAccountQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    purge_local_data: Option<bool>,
    #[serde(default)]
    dry_run: Option<bool>,
}

async fn remove_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(query): Query<RemoveAccountQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::RemoveAccountConfig {
            key,
            purge_local_data: query.purge_local_data.unwrap_or(false),
            dry_run: query.dry_run.unwrap_or(false),
        },
    )
    .await?;
    passthrough(response)
}

async fn disable_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DisableAccountConfig { key },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// platform — account addresses (account_id is a UUID, hence path param parsing)

async fn list_account_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = parse_account_id(&account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListAccountAddresses { account_id: id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AddAddressBody {
    email: String,
    #[serde(default)]
    primary: bool,
}

async fn add_account_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<AddAddressBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = parse_account_id(&account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::AddAccountAddress {
            account_id: id,
            email: body.email,
            primary: body.primary,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct EmailBody {
    email: String,
}

async fn remove_account_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<EmailBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = parse_account_id(&account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RemoveAccountAddress {
            account_id: id,
            email: body.email,
        },
    )
    .await?;
    passthrough(response)
}

async fn set_primary_account_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<EmailBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = parse_account_id(&account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetPrimaryAccountAddress {
            account_id: id,
            email: body.email,
        },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// platform — semantic profile management

#[derive(Debug, Deserialize)]
struct EnableSemanticBody {
    enabled: bool,
}

async fn semantic_enable(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<EnableSemanticBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::EnableSemantic {
            enabled: body.enabled,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ProfileBody {
    /// `SemanticProfile` is a serde-tagged enum; clients send the same
    /// JSON shape they would over IPC (e.g. `{"profile":"bge-small-en-v1.5"}`).
    profile: SemanticProfile,
}

async fn semantic_install_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ProfileBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::InstallSemanticProfile {
            profile: body.profile,
        },
    )
    .await?;
    passthrough(response)
}

async fn semantic_use_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ProfileBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::UseSemanticProfile {
            profile: body.profile,
        },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// mail — message-level helpers and undo

#[derive(Debug, Deserialize)]
struct UndoMutationBody {
    mutation_id: String,
}

async fn undo_mutation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<UndoMutationBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::UndoMutation {
            mutation_id: body.mutation_id,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct CountQuery {
    #[serde(default)]
    token: Option<String>,
    query: String,
    #[serde(default)]
    mode: Option<String>,
}

async fn count_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CountQuery>,
) -> Result<Json<Value>, BridgeError> {
    let mode = match query.mode.as_deref() {
        None => None,
        Some("lexical") => Some(SearchMode::Lexical),
        Some("hybrid") => Some(SearchMode::Hybrid),
        Some("semantic") => Some(SearchMode::Semantic),
        Some(other) => {
            return Err(BridgeError::Ipc(format!("unknown mode={other}")));
        }
    };
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::Count {
            query: query.query,
            mode,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SyncStatusQuery {
    #[serde(default)]
    token: Option<String>,
    /// Required — Request::GetSyncStatus takes a non-Optional account_id.
    account_id: String,
}

async fn sync_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SyncStatusQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetSyncStatus { account_id: id },
    )
    .await?;
    passthrough(response)
}

async fn unsnooze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::Unsnooze { message_id: id },
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// reply-later, auto-reminders, send-later, snippets, sender, screener,
// summarize, draft-assist — bridge surface for the v0.5+ delight features.

#[derive(Debug, Deserialize)]
struct SetReplyLaterBody {
    flag: bool,
}

async fn set_reply_later(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetReplyLaterBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetReplyLater {
            message_id: id,
            flag: body.flag,
        },
    )
    .await?;
    passthrough(response)
}

async fn list_reply_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListReplyQueue,
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetAutoReminderBody {
    sent_message_id: String,
    remind_at: chrono::DateTime<chrono::Utc>,
}

async fn set_auto_reminder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetAutoReminderBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&body.sent_message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid sent_message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetAutoReminder {
            sent_message_id: id,
            remind_at: body.remind_at,
        },
    )
    .await?;
    passthrough(response)
}

async fn cancel_auto_reminder(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::CancelAutoReminder {
            sent_message_id: id,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ScheduleSendBody {
    draft_id: String,
    send_at: chrono::DateTime<chrono::Utc>,
}

async fn schedule_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ScheduleSendBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = DraftId::from_str(&body.draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ScheduleSend {
            draft_id: id,
            send_at: body.send_at,
        },
    )
    .await?;
    passthrough(response)
}

async fn cancel_scheduled_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = DraftId::from_str(&draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::CancelScheduledSend { draft_id: id },
    )
    .await?;
    passthrough(response)
}

async fn list_snippets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListSnippets,
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetSnippetBody {
    name: String,
    body: String,
    #[serde(default)]
    vars: Vec<String>,
}

async fn set_snippet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetSnippetBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetSnippet {
            name: body.name,
            body: body.body,
            vars: body.vars,
        },
    )
    .await?;
    passthrough(response)
}

async fn delete_snippet(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DeleteSnippet { name },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SenderProfileQuery {
    #[serde(default)]
    token: Option<String>,
    account_id: String,
    email: String,
}

async fn get_sender_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SenderProfileQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetSenderProfile {
            account_id,
            email: query.email,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ContactsAutocompleteQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default = "default_autocomplete_limit")]
    limit: u32,
}

fn default_autocomplete_limit() -> u32 {
    10
}

/// Filtered prefix-search over the user's known senders. Returns up to `limit`
/// candidates whose email or display name contain the query (case-insensitive).
async fn contacts_autocomplete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ContactsAutocompleteQuery>,
) -> Result<Json<Value>, BridgeError> {
    // Pull more than the requested limit so client-side filtering has headroom.
    let scan_limit = query.limit.saturating_mul(20).max(50).min(500);
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListSenders {
            limit: scan_limit,
            since_unix: None,
        },
    )
    .await?;
    let q = query.q.unwrap_or_default().to_lowercase();
    let limit = query.limit as usize;
    let raw = serde_json::to_value(response).unwrap_or(Value::Null);
    let mut matches: Vec<Value> = Vec::new();
    if let Some(senders) = raw.get("senders").and_then(|v| v.as_array()) {
        for sender in senders {
            let email = sender
                .get("email")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let name = sender
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !q.is_empty()
                && !email.to_lowercase().contains(&q)
                && !name.to_lowercase().contains(&q)
            {
                continue;
            }
            matches.push(sender.clone());
            if matches.len() >= limit {
                break;
            }
        }
    }
    Ok(Json(json!({ "contacts": matches })))
}

#[derive(Debug, Deserialize)]
struct RelationshipProfileQuery {
    #[serde(default)]
    token: Option<String>,
    account_id: String,
    email: String,
}

async fn get_relationship_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<RelationshipProfileQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetRelationshipProfile {
            account_id,
            email: query.email,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct RebuildRelationshipBody {
    account_id: String,
    email: String,
}

async fn rebuild_relationship_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<RebuildRelationshipBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&body.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RebuildRelationshipProfile {
            account_id,
            email: body.email,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct CommitmentsQuery {
    #[serde(default)]
    token: Option<String>,
    account_id: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    status: Option<CommitmentStatusData>,
}

async fn list_commitments(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CommitmentsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListCommitments {
            account_id,
            email: query.email,
            status: query.status,
        },
    )
    .await?;
    passthrough(response)
}

async fn resolve_commitment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(commitment_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ResolveCommitment { commitment_id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ScreenerQueueQuery {
    #[serde(default)]
    token: Option<String>,
    account_id: String,
    #[serde(default = "default_screener_limit")]
    limit: u32,
}

fn default_screener_limit() -> u32 {
    100
}

async fn list_screener_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScreenerQueueQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListScreenerQueue {
            account_id,
            limit: query.limit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AccountQuery {
    #[serde(default)]
    token: Option<String>,
    account_id: String,
}

async fn list_screener_decisions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListScreenerDecisions { account_id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetScreenerDecisionBody {
    account_id: String,
    sender_email: String,
    disposition: ScreenerDispositionData,
    #[serde(default)]
    route_label: Option<String>,
}

async fn set_screener_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetScreenerDecisionBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&body.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetScreenerDecision {
            account_id,
            sender_email: body.sender_email,
            disposition: body.disposition,
            route_label: body.route_label,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ClearScreenerDecisionBody {
    account_id: String,
    sender_email: String,
}

async fn clear_screener_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ClearScreenerDecisionBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&body.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ClearScreenerDecision {
            account_id,
            sender_email: body.sender_email,
        },
    )
    .await?;
    passthrough(response)
}

async fn summarize_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = ThreadId::from_str(&thread_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid thread_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SummarizeThread { thread_id: id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct DraftAssistBody {
    thread_id: String,
    instruction: String,
}

async fn draft_assist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<DraftAssistBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = ThreadId::from_str(&body.thread_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid thread_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DraftAssist {
            thread_id: id,
            instruction: body.instruction,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct DraftNewBody {
    account_id: String,
    to: Address,
    purpose: String,
    #[serde(default)]
    register: Option<VoiceRegisterData>,
    #[serde(default)]
    length_hint: Option<DraftLengthHintData>,
}

async fn draft_new(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<DraftNewBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&body.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DraftNew {
            account_id,
            to: body.to,
            purpose: body.purpose,
            register: body.register,
            length_hint: body.length_hint,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct DraftRefineBody {
    draft_id: String,
    knobs: DraftRefineKnobsData,
}

async fn draft_refine(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<DraftRefineBody>,
) -> Result<Json<Value>, BridgeError> {
    let draft_id = DraftId::from_str(&body.draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DraftRefine {
            draft_id,
            knobs: body.knobs,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct HumanizerTextBody {
    text: String,
    #[serde(default)]
    max_iterations: Option<u8>,
}

async fn humanizer_score(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<HumanizerTextBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::HumanizerScore { text: body.text },
    )
    .await?;
    passthrough(response)
}

async fn humanizer_rewrite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<HumanizerTextBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::HumanizerRewrite {
            text: body.text,
            max_iterations: body.max_iterations,
        },
    )
    .await?;
    passthrough(response)
}

async fn get_user_voice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetUserVoice { account_id },
    )
    .await?;
    passthrough(response)
}

async fn rebuild_user_voice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = parse_account_id(&query.account_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::RebuildUserVoice { account_id },
    )
    .await?;
    passthrough(response)
}

async fn semantic_backfill(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::BackfillSemantic,
    )
    .await?;
    passthrough(response)
}

// ---------------------------------------------------------------------------
// mail — body/headers/flags, export-search, draft IPC, signatures

async fn get_message_body(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::GetBody { message_id: id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize, Default)]
struct HtmlImagesQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    allow_remote: bool,
}

async fn get_html_image_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(query): Query<HtmlImagesQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetHtmlImageAssets {
            message_id: id,
            allow_remote: query.allow_remote,
        },
    )
    .await?;
    passthrough(response)
}

async fn get_message_headers_ipc(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::GetHeaders { message_id: id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetFlagsBody {
    flags: u32,
}

async fn set_message_flags(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetFlagsBody>,
) -> Result<Json<Value>, BridgeError> {
    let id = MessageId::from_str(&message_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid message_id: {err}")))?;
    let flags = MessageFlags::from_bits(body.flags).ok_or_else(|| {
        BridgeError::Ipc(format!(
            "invalid MessageFlags bits 0x{:x} (unknown bits set)",
            body.flags
        ))
    })?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetFlags {
            message_id: id,
            flags,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ExportSearchBody {
    query: String,
    format: ExportFormat,
}

async fn export_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ExportSearchBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ExportSearch {
            query: body.query,
            format: body.format,
        },
    )
    .await?;
    passthrough(response)
}

async fn list_orphaned_drafts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListOrphanedDrafts,
    )
    .await?;
    passthrough(response)
}

async fn reset_orphaned_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = DraftId::from_str(&draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ResetOrphanedDraft { draft_id: id },
    )
    .await?;
    passthrough(response)
}

async fn send_stored_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = DraftId::from_str(&draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SendStoredDraft {
            draft_id: id,
            override_safety_token: None,
        },
    )
    .await?;
    passthrough(response)
}

async fn save_draft_local(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(draft): Json<Draft>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SaveDraft { draft },
    )
    .await?;
    passthrough(response)
}

async fn delete_draft_stored(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(draft_id): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let id = DraftId::from_str(&draft_id)
        .map_err(|err| BridgeError::Ipc(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DeleteDraft { draft_id: id },
    )
    .await?;
    passthrough(response)
}

async fn list_signatures(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListSignatures,
    )
    .await?;
    passthrough(response)
}

async fn list_signature_defaults(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ListSignatureDefaults,
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetSignatureBody {
    name: String,
    body: String,
}

async fn set_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetSignatureBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetSignature {
            name: body.name,
            body: body.body,
        },
    )
    .await?;
    passthrough(response)
}

async fn delete_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::DeleteSignature { name },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SetSignatureDefaultBody {
    name: String,
    kind: SignatureContextData,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    from_email: Option<String>,
}

async fn set_signature_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<SetSignatureDefaultBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::SetSignatureDefault {
            name: body.name,
            kind: body.kind,
            account_id,
            from_email: body.from_email,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ClearSignatureDefaultBody {
    kind: SignatureContextData,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    from_email: Option<String>,
}

async fn clear_signature_default(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ClearSignatureDefaultBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ClearSignatureDefault {
            kind: body.kind,
            account_id,
            from_email: body.from_email,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ResolveSignatureBody {
    #[serde(default)]
    name: Option<String>,
    kind: SignatureContextData,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    from_email: Option<String>,
}

async fn resolve_signature(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ResolveSignatureBody>,
) -> Result<Json<Value>, BridgeError> {
    let account_id = body
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ResolveSignature {
            name: body.name,
            kind: body.kind,
            account_id,
            from_email: body.from_email,
        },
    )
    .await?;
    passthrough(response)
}

async fn repair_account_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<AccountConfigData>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::RepairAccountConfig { account },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AuthorizeAccountBody {
    account: AccountConfigData,
    #[serde(default)]
    reauthorize: bool,
}

async fn authorize_account_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<AuthorizeAccountBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::AuthorizeAccountConfig {
            account: body.account,
            reauthorize: body.reauthorize,
        },
    )
    .await?;
    passthrough(response)
}

// ===========================================================================
// Activity log routes (Phase 6 — `docs/activity-log.md`).
// Strictly local: routes are bridge-gated like the rest. No new auth surface.
// ===========================================================================

#[derive(Debug, Deserialize, Default)]
struct ActivityListQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    until: Option<i64>,
    #[serde(default)]
    account: Option<String>,
    #[serde(default)]
    source: Vec<String>,
    #[serde(default)]
    action: Vec<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    target_kind: Option<String>,
    #[serde(default)]
    target_id: Option<String>,
    #[serde(default)]
    tier: Vec<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    include_redacted: bool,
    #[serde(default = "default_activity_limit")]
    limit: u32,
    #[serde(default)]
    cursor: Option<String>,
}

fn default_activity_limit() -> u32 {
    50
}

fn parse_source(s: &str) -> Option<mxr_protocol::ClientKind> {
    match s {
        "tui" => Some(mxr_protocol::ClientKind::Tui),
        "cli" => Some(mxr_protocol::ClientKind::Cli),
        "web" => Some(mxr_protocol::ClientKind::Web),
        "daemon" => Some(mxr_protocol::ClientKind::Daemon),
        _ => None,
    }
}

fn parse_tier(s: &str) -> Option<mxr_protocol::ActivityTier> {
    match s {
        "ephemeral" => Some(mxr_protocol::ActivityTier::Ephemeral),
        "standard" => Some(mxr_protocol::ActivityTier::Standard),
        "important" => Some(mxr_protocol::ActivityTier::Important),
        _ => None,
    }
}

fn query_to_filter(q: &ActivityListQuery) -> mxr_protocol::ActivityFilter {
    mxr_protocol::ActivityFilter {
        since: q.since,
        until: q.until,
        account_id: q.account.clone(),
        sources: q.source.iter().filter_map(|s| parse_source(s)).collect(),
        actions: q.action.clone(),
        action_prefix: q.prefix.clone(),
        target_kind: q.target_kind.clone(),
        target_id: q.target_id.clone(),
        tiers: q.tier.iter().filter_map(|t| parse_tier(t)).collect(),
        query: q.query.clone(),
        include_redacted: q.include_redacted,
    }
}

fn parse_cursor_str(s: &str) -> Option<mxr_protocol::ActivityCursor> {
    let (ts, id) = s.split_once(',')?;
    Some(mxr_protocol::ActivityCursor {
        ts: ts.trim().parse().ok()?,
        id: id.trim().parse().ok()?,
    })
}

async fn list_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ActivityListQuery>,
) -> Result<Json<Value>, BridgeError> {
    let filter = query_to_filter(&q);
    let cursor = q.cursor.as_deref().and_then(parse_cursor_str);
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::ListActivity {
            filter,
            limit: q.limit,
            cursor,
        },
    )
    .await?;
    passthrough(response)
}

async fn count_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ActivityListQuery>,
) -> Result<Json<Value>, BridgeError> {
    let filter = query_to_filter(&q);
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::CountActivity { filter },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ActivityStatsQuery {
    #[serde(default)]
    token: Option<String>,
    since: i64,
    until: i64,
    group_by: String,
}

async fn activity_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ActivityStatsQuery>,
) -> Result<Json<Value>, BridgeError> {
    let group_by = match q.group_by.as_str() {
        "action" => mxr_protocol::ActivityStatGroupBy::Action,
        "day" => mxr_protocol::ActivityStatGroupBy::Day,
        "source" => mxr_protocol::ActivityStatGroupBy::Source,
        "target_kind" | "target-kind" | "targetkind" => {
            mxr_protocol::ActivityStatGroupBy::TargetKind
        }
        "hour" => mxr_protocol::ActivityStatGroupBy::Hour,
        other => return Err(BridgeError::Ipc(format!("unknown group_by '{other}'"))),
    };
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::ActivityStats {
            since: q.since,
            until: q.until,
            group_by,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ExportActivityBody {
    #[serde(default)]
    token: Option<String>,
    filter: mxr_protocol::ActivityFilter,
    format: String,
    #[serde(default)]
    path: Option<String>,
}

async fn export_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ExportActivityBody>,
) -> Result<Json<Value>, BridgeError> {
    let format = match body.format.as_str() {
        "csv" => mxr_protocol::ActivityExportFormat::Csv,
        "json" => mxr_protocol::ActivityExportFormat::Json,
        "ndjson" => mxr_protocol::ActivityExportFormat::Ndjson,
        other => return Err(BridgeError::Ipc(format!("unknown format '{other}'"))),
    };
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::ExportActivity {
            filter: body.filter,
            format,
            path: body.path,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct RedactBody {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    ids: Vec<i64>,
    #[serde(default)]
    filter: Option<mxr_protocol::ActivityFilter>,
    #[serde(default)]
    dry_run: bool,
}

async fn redact_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RedactBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::RedactActivity {
            ids: body.ids,
            filter: body.filter,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct PruneBody {
    #[serde(default)]
    token: Option<String>,
    before_ts: i64,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn prune_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PruneBody>,
) -> Result<Json<Value>, BridgeError> {
    let tier = body.tier.as_deref().and_then(parse_tier);
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::PruneActivity {
            before_ts: body.before_ts,
            tier,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct PauseBody {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    until_ts: Option<i64>,
}

async fn pause_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PauseBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::PauseActivity {
            until_ts: body.until_ts,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ResumeBody {
    #[serde(default)]
    token: Option<String>,
}

async fn resume_activity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ResumeBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::ResumeActivity,
    )
    .await?;
    passthrough(response)
}

async fn list_saved_activity_filters(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<TokenOnlyQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::ListSavedActivityFilters,
    )
    .await?;
    passthrough(response)
}

async fn get_saved_activity_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(q): Query<TokenOnlyQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::GetSavedActivityFilter { slug },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct UpsertSavedBody {
    #[serde(default)]
    token: Option<String>,
    slug: String,
    name: String,
    filter: mxr_protocol::ActivityFilter,
}

async fn upsert_saved_activity_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpsertSavedBody>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        body.token.as_deref(),
        Request::UpsertSavedActivityFilter {
            slug: body.slug,
            name: body.name,
            filter: body.filter,
        },
    )
    .await?;
    passthrough(response)
}

async fn delete_saved_activity_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Query(q): Query<TokenOnlyQuery>,
) -> Result<Json<Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        q.token.as_deref(),
        Request::DeleteSavedActivityFilter { slug },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize, Default)]
struct TokenOnlyQuery {
    #[serde(default)]
    token: Option<String>,
}

// ---------------------------------------------------------------------------
// router builders — extend the bucket sub-routers in lib.rs

pub fn extend_admin(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/ping", post(ping))
        .route("/shutdown", post(shutdown))
        .route("/events", get(list_events))
        .route("/events/count", get(count_events))
        .route("/events/categories", get(list_event_categories))
        .route("/logs", get(get_logs))
        // ---- activity log (Phase 6) ----
        .route("/activity", get(list_activity))
        .route("/activity/count", get(count_activity))
        .route("/activity/stats", get(activity_stats))
        .route("/activity/export", post(export_activity))
        .route("/activity/redact", post(redact_activity))
        .route("/activity/prune", post(prune_activity))
        .route("/activity/pause", post(pause_activity))
        .route("/activity/resume", post(resume_activity))
        .route("/activity/saved", get(list_saved_activity_filters).post(upsert_saved_activity_filter))
        .route(
            "/activity/saved/{slug}",
            get(get_saved_activity_filter).delete(delete_saved_activity_filter),
        )
}

pub fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/messages/{message_id}/body", get(get_message_body))
        .route(
            "/messages/{message_id}/html-images",
            get(get_html_image_assets),
        )
        .route(
            "/messages/{message_id}/headers",
            get(get_message_headers_ipc),
        )
        .route("/messages/{message_id}/flags", post(set_message_flags))
        .route("/export-search", post(export_search))
        .route("/drafts/orphaned", get(list_orphaned_drafts))
        .route("/drafts/save-local", post(save_draft_local))
        .route(
            "/drafts/{draft_id}/reset-orphan",
            post(reset_orphaned_draft),
        )
        .route("/drafts/{draft_id}/send-stored", post(send_stored_draft))
        .route("/drafts/{draft_id}/stored", delete(delete_draft_stored))
        .route("/signatures", get(list_signatures).post(set_signature))
        .route("/signature-defaults", get(list_signature_defaults))
        .route("/signatures/resolve", post(resolve_signature))
        .route("/signatures/default/clear", post(clear_signature_default))
        .route("/signatures/default", post(set_signature_default))
        .route("/signatures/{name}", delete(delete_signature))
        .route("/mutations/undo", post(undo_mutation))
        .route("/count", get(count_messages))
        .route("/sync/status", get(sync_status))
        .route("/snoozed/{message_id}/wake", post(unsnooze))
        // reply-later
        .route("/reply-later/{message_id}", post(set_reply_later))
        .route("/reply-later", get(list_reply_queue))
        // auto-reminders
        .route("/reminders", post(set_auto_reminder))
        .route("/reminders/{message_id}", delete(cancel_auto_reminder))
        // send-later (scheduled drafts)
        .route("/scheduled-sends", post(schedule_send))
        .route("/scheduled-sends/{draft_id}", delete(cancel_scheduled_send))
        // snippets
        .route("/snippets", get(list_snippets).post(set_snippet))
        .route("/snippets/{name}", delete(delete_snippet))
        // sender view + contact autocomplete
        .route("/sender", get(get_sender_profile))
        .route("/contacts/autocomplete", get(contacts_autocomplete))
        .route("/relationship", get(get_relationship_profile))
        .route("/relationship/rebuild", post(rebuild_relationship_profile))
        .route("/commitments", get(list_commitments))
        .route(
            "/commitments/{commitment_id}/resolve",
            post(resolve_commitment),
        )
        // screener
        .route("/screener/queue", get(list_screener_queue))
        .route(
            "/screener/decisions",
            get(list_screener_decisions)
                .post(set_screener_decision)
                .delete(clear_screener_decision),
        )
        // LLM features
        .route("/threads/{thread_id}/summarize", post(summarize_thread))
        .route("/threads/draft-assist", post(draft_assist))
        .route("/drafts/new", post(draft_new))
        .route("/drafts/refine", post(draft_refine))
        .route("/humanizer/score", post(humanizer_score))
        .route("/humanizer/rewrite", post(humanizer_rewrite))
}

pub fn extend_platform(router: Router<AppState>) -> Router<AppState> {
    router
        // analytics
        .route("/analytics/wrapped", get(analytics_wrapped))
        .route(
            "/analytics/storage-breakdown",
            get(analytics_storage_breakdown),
        )
        .route(
            "/analytics/largest-messages",
            get(analytics_largest_messages),
        )
        .route("/analytics/stale-threads", get(analytics_stale_threads))
        .route(
            "/analytics/contact-asymmetry",
            get(analytics_contact_asymmetry),
        )
        .route("/analytics/contact-decay", get(analytics_contact_decay))
        .route("/analytics/response-time", get(analytics_response_time))
        .route(
            "/analytics/refresh-contacts",
            post(analytics_refresh_contacts),
        )
        .route("/analytics/rebuild", post(analytics_rebuild))
        // saved searches list + run
        .route("/saved-searches", get(list_saved_searches))
        .route("/saved-searches/run", post(run_saved_search))
        // accounts: config / lifecycle
        .route("/accounts/config", get(list_accounts_config))
        .route("/accounts/authorize", post(authorize_account_config))
        .route("/accounts/repair", post(repair_account_config))
        .route("/accounts/{key}", delete(remove_account))
        .route("/accounts/{key}/disable", post(disable_account))
        // account addresses
        .route(
            "/accounts/{account_id}/addresses",
            get(list_account_addresses),
        )
        .route(
            "/accounts/{account_id}/addresses",
            post(add_account_address),
        )
        .route(
            "/accounts/{account_id}/addresses/remove",
            post(remove_account_address),
        )
        .route(
            "/accounts/{account_id}/addresses/primary",
            post(set_primary_account_address),
        )
        // semantic
        .route("/semantic/enable", post(semantic_enable))
        .route("/semantic/backfill", post(semantic_backfill))
        .route("/semantic/profiles/install", post(semantic_install_profile))
        .route("/semantic/profiles/use", post(semantic_use_profile))
        .route("/voice", get(get_user_voice))
        .route("/voice/rebuild", post(rebuild_user_voice))
}
