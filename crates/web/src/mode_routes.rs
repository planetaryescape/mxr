//! The modes over the bridge: Now, the rail, which modes hold a thread,
//! done here per mode, how each mode explains itself and which of its
//! hints are dismissed. Thin passthroughs to the daemon,
//! which owns the caps, the copy and every preview's selection.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_core::id::{MessageId, ThreadId};
use mxr_protocol::{ModeDoneSenderData, ModeKindData};

#[derive(Debug, Deserialize)]
struct AccountQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
}

async fn get_now(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetNow { account_id },
    )
    .await?;
    passthrough(response)
}

async fn get_rail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetRail { account_id },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct MembershipQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    thread_id: Option<ThreadId>,
    #[serde(default)]
    message_id: Option<MessageId>,
}

async fn get_membership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<MembershipQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if query.thread_id.is_none() && query.message_id.is_none() {
        return Err(BridgeError::BadRequest(
            "name a thread_id or a message_id".into(),
        ));
    }
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetModeMembership {
            message_id: query.message_id,
            thread_id: query.thread_id,
            thread_ids: Vec::new(),
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/modes/membership`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ModeMembershipBody {
    /// At most 100 threads, answered in this order.
    #[schema(value_type = Vec<String>)]
    thread_ids: Vec<ThreadId>,
}

async fn post_membership(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ModeMembershipBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.thread_ids.is_empty() {
        return Err(BridgeError::BadRequest(
            "thread_ids must not be empty".into(),
        ));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::GetModeMembership {
            message_id: None,
            thread_id: None,
            thread_ids: body.thread_ids,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/modes/{mode}/done`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ModeDoneBody {
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    thread_ids: Vec<ThreadId>,
    /// Preview only: the same plan, nothing changed.
    #[serde(default)]
    dry_run: bool,
    /// To do only: tick off just these rows.
    #[serde(default)]
    todo_ids: Vec<String>,
    /// Updates or Reading: every thread of this sender's there.
    #[serde(default)]
    sender: Option<ModeDoneSenderData>,
}

fn parse_mode(raw: &str) -> Result<ModeKindData, BridgeError> {
    ModeKindData::parse(raw).ok_or_else(|| {
        BridgeError::BadRequest(format!(
            "unknown mode `{raw}`; use messages, todo, updates, reading or archive"
        ))
    })
}

async fn set_done(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(mode): AxumPath<String>,
    Json(body): Json<ModeDoneBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let mode = parse_mode(&mode)?;
    if body.thread_ids.is_empty() && body.sender.is_none() {
        return Err(BridgeError::BadRequest(
            "name thread_ids or a sender".into(),
        ));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetModeDone {
            thread_ids: body.thread_ids,
            mode,
            dry_run: body.dry_run,
            todo_ids: body.todo_ids,
            sender: body.sender,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct GuideQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every mode that has shipped.
    #[serde(default)]
    mode: Option<String>,
}

async fn get_guide(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GuideQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let mode = query.mode.filter(|mode| !mode.is_empty());
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetModeGuide { mode },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/hints/{hint}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct HintSeenBody {
    /// True dismisses the hint in every client; false shows it again.
    #[serde(default = "seen_default")]
    seen: bool,
}

fn seen_default() -> bool {
    true
}

async fn set_hint_seen(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(hint): AxumPath<String>,
    Json(body): Json<HintSeenBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetHintSeen {
            hint,
            seen: body.seen,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/now", get(get_now))
        .route("/rail", get(get_rail))
        .route("/modes/guide", get(get_guide))
        .route(
            "/modes/membership",
            get(get_membership).post(post_membership),
        )
        .route("/hints/{hint}", post(set_hint_seen))
        .route("/modes/{mode}/done", post(set_done))
}
