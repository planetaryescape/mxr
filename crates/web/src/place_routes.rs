//! Reading and Paper trail: list a place, say why a message is where it is,
//! move a sender to a kind, pin exceptions, and sweep. Thin passthroughs to
//! the daemon, which owns every rule and the sweep's selection.

use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_protocol::{MailPlaceData, SenderKindData};

// Mirror the protocol's serde defaults so an omitted field behaves the same
// over HTTP as over IPC.
const DEFAULT_PLACE_LIMIT: u32 = 50;
const DEFAULT_MESSAGES_PER_BUNDLE: u32 = 20;

fn parse_place(raw: &str) -> Result<MailPlaceData, BridgeError> {
    match raw {
        "reading" => Ok(MailPlaceData::Reading),
        "paper-trail" | "paper_trail" => Ok(MailPlaceData::PaperTrail),
        other => Err(BridgeError::BadRequest(format!(
            "unknown place `{other}`; use reading or paper-trail"
        ))),
    }
}

fn parse_optional_account(raw: Option<&str>) -> Result<Option<AccountId>, BridgeError> {
    raw.filter(|value| !value.is_empty())
        .map(parse_account_id)
        .transpose()
}

#[derive(Debug, Deserialize)]
struct PlaceQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default, alias = "sender_email")]
    sender: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    messages_per_bundle: Option<u32>,
    #[serde(default)]
    message_offset: Option<u32>,
}

async fn list_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(place): AxumPath<String>,
    Query(query): Query<PlaceQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let place = parse_place(&place)?;
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListPlace {
            place,
            account_id,
            sender_email: query.sender.filter(|sender| !sender.trim().is_empty()),
            limit: query.limit.unwrap_or(DEFAULT_PLACE_LIMIT),
            offset: query.offset.unwrap_or(0),
            messages_per_bundle: query
                .messages_per_bundle
                .unwrap_or(DEFAULT_MESSAGES_PER_BUNDLE),
            message_offset: query.message_offset.unwrap_or(0),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SweepBody {
    #[serde(default, alias = "account")]
    account_id: Option<String>,
    #[serde(default)]
    sender_email: Option<String>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    preview_token: Option<String>,
}

async fn sweep_place(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(place): AxumPath<String>,
    Json(body): Json<SweepBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let place = parse_place(&place)?;
    let account_id = parse_optional_account(body.account_id.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SweepPlace {
            place,
            account_id,
            sender_email: body.sender_email.filter(|sender| !sender.trim().is_empty()),
            dry_run: body.dry_run,
            preview_token: body.preview_token,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

async fn message_kind(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(message_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let message_id = parse_message_id(&message_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetMessageKind { message_id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct PinBody {
    message_ids: Vec<String>,
    pinned: bool,
}

async fn pin_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PinBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.message_ids.is_empty() {
        return Err(BridgeError::BadRequest(
            "message_ids must not be empty".into(),
        ));
    }
    let message_ids = parse_message_ids(&body.message_ids)?;
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::PinMessages {
            message_ids,
            pinned: body.pinned,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct SenderKindBody {
    account_id: String,
    sender_email: String,
    /// `null` or omitted: back to automatic.
    #[serde(default)]
    kind: Option<SenderKindData>,
}

async fn set_sender_kind(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SenderKindBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_account_id(&body.account_id)?;
    if body.sender_email.trim().is_empty() {
        return Err(BridgeError::BadRequest(
            "sender_email must not be empty".into(),
        ));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetSenderKind {
            account_id,
            sender_email: body.sender_email,
            kind: body.kind,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/places/{place}", get(list_place))
        .route("/places/{place}/sweep", post(sweep_place))
        .route("/messages/{message_id}/kind", get(message_kind))
        .route("/messages/pin", post(pin_messages))
        .route("/senders/kind", post(set_sender_kind))
}
