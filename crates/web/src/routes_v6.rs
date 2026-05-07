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
    id::{AccountId, MessageId},
    types::{ResponseTimeDirection, SemanticProfile, StaleBallInCourt, StorageGroupBy},
    SearchMode,
};
use mxr_protocol::{Request, ResponseData};
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
        Some("i_replied") | Some("i-replied") | Some("incoming") => {
            ResponseTimeDirection::IReplied
        }
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
// router builders — extend the bucket sub-routers in lib.rs

pub fn extend_admin(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/ping", post(ping))
        .route("/shutdown", post(shutdown))
        .route("/events", get(list_events))
        .route("/logs", get(get_logs))
}

pub fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/mutations/undo", post(undo_mutation))
        .route("/count", get(count_messages))
        .route("/sync/status", get(sync_status))
        .route("/snoozed/{message_id}/wake", post(unsnooze))
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
        .route("/accounts/{key}", delete(remove_account))
        .route("/accounts/{key}/disable", post(disable_account))
        // account addresses
        .route(
            "/accounts/{account_id}/addresses",
            get(list_account_addresses),
        )
        .route("/accounts/{account_id}/addresses", post(add_account_address))
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
        .route(
            "/semantic/profiles/install",
            post(semantic_install_profile),
        )
        .route("/semantic/profiles/use", post(semantic_use_profile))
}

