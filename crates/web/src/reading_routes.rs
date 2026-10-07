//! Reading over the bridge: the edition, one item for the reader, Later,
//! engagement, the article fetch, highlights and per-source settings. Thin
//! passthroughs to the daemon, which owns the extraction, the bands, the
//! fetch guards and every preview.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_core::id::AccountId;

#[derive(Debug, Deserialize)]
struct EditionQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// Record that Reading was opened, so the next visit's "since you were
    /// last here" starts from this one.
    #[serde(default)]
    mark_visit: bool,
}

async fn edition(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EditionQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetReadingEdition {
            account_id,
            mark_visit: query.mark_visit,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
}

async fn item(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(item_key): AxumPath<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetReadingItem { item_key },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/reading/later`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ReadingLaterBody {
    /// `<message id>:<index>` keys.
    item_keys: Vec<String>,
    /// True puts them on Later (or keeps them); false takes them off.
    #[serde(default = "true_default")]
    later: bool,
    #[serde(default)]
    dry_run: bool,
}

fn true_default() -> bool {
    true
}

async fn later(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReadingLaterBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.item_keys.is_empty() {
        return Err(BridgeError::BadRequest("item_keys must not be empty".into()));
    }
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetReadingLater {
            item_keys: body.item_keys,
            later: body.later,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/reading/engagement`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ReadingEngagementBody {
    item_key: String,
    #[serde(default)]
    opened: bool,
    /// Time read since the last report, in milliseconds.
    #[serde(default)]
    dwell_ms: u64,
    /// How far down, 0 to 1.
    #[serde(default)]
    progress: f64,
}

async fn engagement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReadingEngagementBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::RecordReadingEngagement {
            item_key: body.item_key,
            opened: body.opened,
            dwell_ms: body.dwell_ms,
            progress: body.progress,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/reading/items/{item_key}/article`.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub(crate) struct ReadingArticleBody {
    /// Fetch again even when a copy is saved.
    #[serde(default)]
    refresh: bool,
}

async fn article(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(item_key): AxumPath<String>,
    body: Option<Json<ReadingArticleBody>>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let refresh = body.is_some_and(|Json(body)| body.refresh);
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::FetchArticle { item_key, refresh },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/reading/highlights`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ReadingHighlightBody {
    item_key: String,
    quote: String,
    #[serde(default)]
    note: Option<String>,
    /// `issue` (default) or `article`.
    #[serde(default)]
    view: Option<String>,
}

async fn save_highlight(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReadingHighlightBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SaveHighlight {
            item_key: body.item_key,
            quote: body.quote,
            note: body.note,
            view: body.view,
        },
    )
    .await?;
    passthrough(response)
}

async fn highlights(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ExportReadingHighlights { account_id },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/reading/sources`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ReadingSourceBody {
    #[schema(value_type = String)]
    account_id: AccountId,
    sender_email: String,
    /// Show the sender's own layout for this source.
    #[serde(default)]
    original_layout: Option<bool>,
    /// Stop offering to unsubscribe from this source.
    #[serde(default)]
    dismiss_unsubscribe_offer: bool,
}

async fn set_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReadingSourceBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetReadingSource {
            account_id: body.account_id,
            sender_email: body.sender_email,
            original_layout: body.original_layout,
            dismiss_unsubscribe_offer: body.dismiss_unsubscribe_offer,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/reading", get(edition))
        .route("/reading/later", post(later))
        .route("/reading/engagement", post(engagement))
        .route("/reading/highlights", get(highlights).post(save_highlight))
        .route("/reading/sources", post(set_source))
        .route("/reading/items/{item_key}", get(item))
        .route("/reading/items/{item_key}/article", post(article))
}
