//! Sorting shows its work over the bridge (D119): Now's arrivals line, the
//! emails behind each count, Inbox's mode chips, moving an email or a
//! sender, undo, and the corrections log. Thin passthroughs: the daemon owns
//! the window, the counts and the copy.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_protocol::{ArrivalBucketData, ModeKindData};

#[derive(Debug, Deserialize)]
struct ArrivalsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// True when Now opens: starts a visit, so the window runs from the
    /// visit before.
    #[serde(default)]
    mark_seen: bool,
}

async fn get_arrivals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ArrivalsQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetArrivals {
            account_id,
            mark_seen: query.mark_seen,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct ArrivalListQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default, alias = "mode")]
    bucket: Option<String>,
    #[serde(default)]
    since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    limit: Option<u32>,
}

fn parse_bucket(raw: Option<&str>) -> Result<Option<ArrivalBucketData>, BridgeError> {
    raw.filter(|raw| !raw.is_empty())
        .map(|raw| {
            ArrivalBucketData::parse(raw).ok_or_else(|| {
                BridgeError::BadRequest(format!(
                    "unknown bucket `{raw}`; use messages, todo, updates, reading, archive, screened_out, spam or sorting"
                ))
            })
        })
        .transpose()
}

async fn list_arrivals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ArrivalListQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let bucket = parse_bucket(query.bucket.as_deref())?;
    if let (Some(since), Some(until)) = (query.since, query.until) {
        if until <= since {
            return Err(BridgeError::BadRequest("until must be after since".into()));
        }
    }
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListArrivals {
            account_id,
            bucket,
            since: query.since,
            until: query.until,
            limit: query.limit.unwrap_or(100),
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/arrivals/modes`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ArrivalModesBody {
    /// At most 200, answered in this order.
    message_ids: Vec<String>,
}

async fn arrival_modes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ArrivalModesBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.message_ids.is_empty() {
        return Err(BridgeError::BadRequest(
            "message_ids must not be empty".into(),
        ));
    }
    if body.message_ids.len() > 200 {
        return Err(BridgeError::BadRequest(
            "at most 200 message_ids at once".into(),
        ));
    }
    let message_ids = parse_message_ids(&body.message_ids)?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::GetArrivalModes { message_ids },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/messages/{message_id}/move`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MoveBody {
    /// messages, todo, updates, reading or archive.
    mode: String,
    /// Set the sender's mode for all their mail (`K`).
    #[serde(default)]
    sender: bool,
    #[serde(default)]
    dry_run: bool,
    /// `not_sure` when answering Now's question.
    #[serde(default)]
    source: Option<String>,
}

async fn move_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(message_id): AxumPath<String>,
    Json(body): Json<MoveBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = parse_message_id(&message_id)?;
    let mode = ModeKindData::parse(&body.mode).ok_or_else(|| {
        BridgeError::BadRequest(format!(
            "unknown mode `{}`; use messages, todo, updates, reading or archive",
            body.mode
        ))
    })?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::MoveMessage {
            message_id,
            mode,
            sender: body.sender,
            dry_run: body.dry_run,
            source: body.source,
        },
    )
    .await?;
    passthrough(response)
}

async fn undo_move(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(correction_id): AxumPath<i64>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(&state, &headers, None, Request::UndoMove { correction_id }).await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct CorrectionsQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

async fn list_corrections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CorrectionsQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListCorrections {
            account_id,
            limit: query.limit.unwrap_or(50),
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/arrivals", get(get_arrivals))
        .route("/arrivals/list", get(list_arrivals))
        .route("/arrivals/modes", post(arrival_modes))
        .route("/messages/{message_id}/move", post(move_message))
        .route("/moves/{correction_id}/undo", post(undo_move))
        .route("/corrections", get(list_corrections))
}
