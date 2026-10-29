//! The reader's context block: store facts for a conversation, and the
//! model-written gist and ask. Two routes so the facts never wait on a
//! model, and a batch route for list rows; all are thin passthroughs to
//! the daemon.

use super::routes_v6::{dispatch, passthrough};
use super::*;

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContextQuery {
    #[serde(default)]
    token: Option<String>,
    /// The account the row belongs to, for a thread id two accounts hold.
    #[serde(default)]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GistQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    refresh: bool,
    #[serde(default)]
    account_id: Option<String>,
}

async fn thread_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(thread_id): AxumPath<String>,
    Query(query): Query<ContextQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let thread_id = parse_thread_id(&thread_id)?;
    let account_id = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadContext {
            thread_id,
            account_id,
        },
    )
    .await?;
    passthrough(response)
}

async fn thread_gist(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(thread_id): AxumPath<String>,
    Query(query): Query<GistQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let thread_id = parse_thread_id(&thread_id)?;
    let account_id = query
        .account_id
        .as_deref()
        .map(parse_account_id)
        .transpose()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadGist {
            thread_id,
            refresh: query.refresh,
            account_id,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct GistsBody {
    #[serde(default)]
    thread_ids: Vec<String>,
    /// The same conversations with the account each row belongs to.
    #[serde(default)]
    thread_accounts: Vec<mxr_protocol::ThreadAccountRef>,
    #[serde(default)]
    generate: bool,
    #[serde(default)]
    automated: bool,
}

/// List-row gists for many conversations: cached ones at once, missing
/// ones (people only, and automated senders when `automated`) queued when
/// `generate`, each announced on the event stream as `ThreadGistReady`.
async fn thread_gists(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<GistsBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let thread_ids = body
        .thread_ids
        .iter()
        .map(|id| parse_thread_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadGists {
            thread_ids,
            thread_accounts: body.thread_accounts,
            generate: body.generate,
            automated: body.automated,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/threads/previews`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ThreadPreviewsBody {
    /// At most 100 threads, answered in this order; unknown ids are left out.
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    thread_ids: Vec<String>,
    /// The same conversations with the account each row belongs to; a pair
    /// is previewed for that account only.
    #[serde(default)]
    thread_accounts: Vec<mxr_protocol::ThreadAccountRef>,
}

/// Headline facts for many threads in one read, with no bodies: what a
/// hover card needs.
async fn thread_previews(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
    Json(body): Json<ThreadPreviewsBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    if body.thread_ids.is_empty() && body.thread_accounts.is_empty() {
        return Err(BridgeError::BadRequest(
            "thread_ids must not be empty".into(),
        ));
    }
    let thread_ids = body
        .thread_ids
        .iter()
        .map(|id| parse_thread_id(id))
        .collect::<Result<Vec<_>, _>>()?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadPreviews {
            thread_ids,
            thread_accounts: body.thread_accounts,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/threads/{thread_id}/context", get(thread_context))
        .route("/threads/{thread_id}/context/gist", get(thread_gist))
        .route("/gists", post(thread_gists))
        .route("/threads/previews", post(thread_previews))
}
