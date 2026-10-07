//! Updates over the bridge: the digest, letting go of it, and tuning a
//! source. Thin passthroughs to the daemon, which owns the facts, the
//! cut, the sections and the let-go selection.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_core::id::AccountId;
use mxr_protocol::UpdateSourceSettingData;

#[derive(Debug, Deserialize)]
struct DigestQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// A past cut (RFC3339); the latest when omitted.
    #[serde(default)]
    cut: Option<chrono::DateTime<chrono::Utc>>,
    /// Record that Updates was opened.
    #[serde(default)]
    mark_seen: bool,
    /// List every update past its window.
    #[serde(default)]
    expired: bool,
}

async fn digest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DigestQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetUpdatesDigest {
            account_id,
            cut: query.cut,
            mark_seen: query.mark_seen,
            expired: query.expired,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/updates/let-go`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct UpdatesLetGoBody {
    /// Omitted: every account.
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    account_id: Option<AccountId>,
    /// The cut the digest showed; the latest when omitted.
    #[serde(default)]
    cut: Option<chrono::DateTime<chrono::Utc>>,
    /// One source only.
    #[serde(default)]
    source_key: Option<String>,
    /// From the digest or the dry run: the run refuses if the cut changed.
    #[serde(default)]
    selection_token: Option<String>,
    /// Preview only; nothing changes.
    #[serde(default)]
    dry_run: bool,
}

async fn let_go(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdatesLetGoBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::LetGoDigest {
            account_id: body.account_id,
            cut: body.cut,
            source_key: body.source_key,
            selection_token: body.selection_token,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/updates/sources`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct UpdateSourceBody {
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    account_id: Option<AccountId>,
    /// A source key as the digest names it, or a sender address.
    source: String,
    setting: UpdateSourceSettingData,
    #[serde(default)]
    dry_run: bool,
}

async fn set_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UpdateSourceBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetUpdateSource {
            account_id: body.account_id,
            source: body.source,
            setting: body.setting,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/updates", get(digest))
        .route("/updates/let-go", post(let_go))
        .route("/updates/sources", post(set_source))
}
