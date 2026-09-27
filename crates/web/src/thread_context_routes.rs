//! The reader's context block: store facts for a conversation, and the
//! model-written gist and ask. Two routes so the facts never wait on a
//! model; both are thin passthroughs to the daemon.

use super::routes_v6::{dispatch, passthrough};
use super::*;
use std::str::FromStr;

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

fn parse_thread_id(raw: &str) -> Result<ThreadId, BridgeError> {
    ThreadId::from_str(raw)
        .map_err(|err| BridgeError::BadRequest(format!("invalid thread_id: {err}")))
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

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/threads/{thread_id}/context", get(thread_context))
        .route("/threads/{thread_id}/context/gist", get(thread_gist))
}
