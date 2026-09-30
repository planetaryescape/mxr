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
struct GistQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    refresh: bool,
}

async fn thread_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(thread_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let thread_id = parse_thread_id(&thread_id)?;
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadContext { thread_id },
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
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetThreadGist {
            thread_id,
            refresh: query.refresh,
        },
    )
    .await?;
    passthrough(response)
}

#[derive(Debug, Deserialize)]
struct GistsBody {
    thread_ids: Vec<String>,
    #[serde(default)]
    generate: bool,
}

/// List-row gists for many conversations: cached ones at once, missing
/// ones (people only) queued when `generate`, each announced on the event
/// stream as `ThreadGistReady`.
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
            generate: body.generate,
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
}
