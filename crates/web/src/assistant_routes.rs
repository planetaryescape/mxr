//! The assistant over the bridge. Thin passthroughs: the daemon runs the
//! turn, gates every change and keeps the transcript. Sending and deciding
//! return at once; `AssistantUpdated` events on `/api/v1/events` say when
//! to refetch.

use super::routes_v6::{dispatch, passthrough};
use super::*;

#[derive(Debug, Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

async fn availability(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetAssistantAvailability,
    )
    .await?;
    passthrough(response)
}

async fn models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetAssistantModels,
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/assistant/model`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AssistantModelBody {
    /// A model id from the list `GET .../assistant/models` returns.
    model: String,
    /// Check the choice and say what would change, without changing it.
    #[serde(default)]
    dry_run: bool,
}

async fn set_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AssistantModelBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetAssistantModel {
            model: body.model,
            dry_run: body.dry_run,
        },
    )
    .await?;
    passthrough(response)
}

async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::ListAssistantConversations { limit: query.limit },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/assistant/conversations`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AssistantSendBody {
    /// Continue this conversation; omitted starts a new one.
    #[serde(default)]
    conversation_id: Option<String>,
    message: String,
    /// What the user has open, so "this" and "these" resolve.
    #[serde(default)]
    context: mxr_protocol::AssistantViewContextData,
}

async fn send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<AssistantSendBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::AssistantSend {
            conversation_id: body.conversation_id,
            message: body.message,
            context: body.context,
        },
    )
    .await?;
    passthrough(response)
}

async fn get_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<String>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetAssistantConversation { conversation_id },
    )
    .await?;
    passthrough(response)
}

async fn get_step_rows(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((conversation_id, call_id)): AxumPath<(String, String)>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        query.token.as_deref(),
        Request::GetAssistantStepRows {
            conversation_id,
            call_id,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/assistant/conversations/{id}/decide`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AssistantDecideBody {
    decisions: Vec<mxr_protocol::AssistantDecisionData>,
}

async fn decide(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<String>,
    Json(body): Json<AssistantDecideBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::AssistantDecide {
            conversation_id,
            decisions: body.decisions,
        },
    )
    .await?;
    passthrough(response)
}

async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::AssistantCancel { conversation_id },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/assistant/conversations/{id}/auto-approve`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct AssistantAutoApproveBody {
    enabled: bool,
}

async fn auto_approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<String>,
    Json(body): Json<AssistantAutoApproveBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetAssistantAutoApprove {
            conversation_id,
            enabled: body.enabled,
        },
    )
    .await?;
    passthrough(response)
}

/// Body of `POST /api/v1/mail/assistant/conversations/{id}/delete`.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub(crate) struct AssistantDeleteBody {
    /// Say what would be deleted, without deleting it.
    #[serde(default)]
    dry_run: bool,
}

async fn delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(conversation_id): AxumPath<String>,
    body: Option<Json<AssistantDeleteBody>>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let dry_run = body.is_some_and(|Json(body)| body.dry_run);
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::DeleteAssistantConversation {
            conversation_id,
            dry_run,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/assistant/availability", get(availability))
        .route("/assistant/models", get(models))
        .route("/assistant/model", post(set_model))
        .route("/assistant/conversations", get(list).post(send))
        .route("/assistant/conversations/{id}", get(get_conversation))
        .route(
            "/assistant/conversations/{id}/steps/{call_id}/rows",
            get(get_step_rows),
        )
        .route("/assistant/conversations/{id}/decide", post(decide))
        .route("/assistant/conversations/{id}/cancel", post(cancel))
        .route("/assistant/conversations/{id}/delete", post(delete))
        .route(
            "/assistant/conversations/{id}/auto-approve",
            post(auto_approve),
        )
}
