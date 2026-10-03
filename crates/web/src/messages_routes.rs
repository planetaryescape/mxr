//! Messages over the bridge: people as rows with their topics, a person's
//! page, Got it, and manual merges. Under `/people` because `/messages`
//! is the per-message API. Thin passthroughs: the daemon owns the bands,
//! the new text and every preview.

use super::place_routes::parse_optional_account;
use super::routes_v6::{dispatch, passthrough};
use super::*;
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::MessagesTurnData;

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    token: Option<String>,
    /// Omitted: every account.
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    #[serde(default)]
    turn: Option<MessagesTurnData>,
    #[serde(default)]
    limit: Option<u32>,
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListMessages {
            account_id,
            turn: query.turn,
            limit: query.limit.unwrap_or(50).min(500),
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct PersonQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
    /// A row id (`person:<email>`, `group:<thread>`) or an address.
    person: String,
    #[serde(default)]
    topic: Option<ThreadId>,
}

async fn person(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PersonQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetPerson {
            account_id,
            person: query.person,
            topic: query.topic,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/people/ack`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AckBody {
    #[schema(value_type = String)]
    thread_id: ThreadId,
    /// Preview only: the exact text, nothing sent.
    #[serde(default)]
    dry_run: bool,
    /// The previewed text: the send refuses anything else.
    #[serde(default)]
    expect_text: Option<String>,
}

async fn ack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AckBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::AckMessage {
            thread_id: body.thread_id,
            dry_run: body.dry_run,
            expect_text: body.expect_text,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/people/merge`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct MergeBody {
    #[schema(value_type = String)]
    account_id: AccountId,
    /// The person's main address.
    into: String,
    addresses: Vec<String>,
    #[serde(default)]
    dry_run: bool,
}

async fn merge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MergeBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::MergePeople {
            account_id: body.account_id,
            into: body.into,
            addresses: body.addresses,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/people/split`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct SplitBody {
    #[schema(value_type = String)]
    account_id: AccountId,
    address: String,
    #[serde(default)]
    dry_run: bool,
}

async fn split(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SplitBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SplitPerson {
            account_id: body.account_id,
            address: body.address,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct AccountQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default, alias = "account_id")]
    account: Option<String>,
}

async fn suggestions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AccountQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let account_id = parse_optional_account(query.account.as_deref())?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListMergeSuggestions { account_id },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/people", get(list))
        .route("/people/page", get(person))
        .route("/people/ack", post(ack))
        .route("/people/merge", post(merge))
        .route("/people/split", post(split))
        .route("/people/merge-suggestions", get(suggestions))
}
