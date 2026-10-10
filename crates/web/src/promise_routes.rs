//! Promises on send: find what an outgoing compose session promises while
//! its undo window runs, and keep one as a dated commitment once it has
//! sent. Thin passthroughs to `DetectPromises` and `RecordPromise`.

use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_protocol::PromiseSourceData;
use std::str::FromStr;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct DetectComposePromisesRequest {
    /// The compose session file, as the other compose routes take it.
    draft_path: String,
    account_id: String,
    /// The browser's IANA zone, so "by Friday" resolves where the user is.
    #[serde(default)]
    time_zone: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct RecordPromiseRequest {
    /// The sent message the promise is in.
    message_id: String,
    /// What was promised, such as "send the deck".
    what: String,
    /// The chosen time, exactly as the time preview returned it.
    due_at: DateTime<Utc>,
    /// Return what would be stored without writing it.
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

async fn detect_compose_promises(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(request): Json<DetectComposePromisesRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    // Read-only: the id never leaves this request, like the safety check's.
    let draft = compose_draft_from_file(
        &request.draft_path,
        &request.account_id,
        None,
        ComposeDraftValidation::Send,
    )
    .await?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::DetectPromises {
            source: PromiseSourceData::Draft {
                draft: Box::new(draft),
            },
            now: None,
            time_zone: request.time_zone,
        },
    )
    .await?;
    passthrough(response)
}

async fn record_promise(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(request): Json<RecordPromiseRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = MessageId::from_str(&request.message_id)
        .map_err(|err| BridgeError::BadRequest(format!("invalid message_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::RecordPromise {
            message_id,
            what: request.what,
            due_at: request.due_at,
            dry_run: request.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/compose/session/promises", post(detect_compose_promises))
        .route("/commitments", post(record_promise))
}
