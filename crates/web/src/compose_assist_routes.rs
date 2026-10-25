//! Writing help over the bridge (blueprint 34): ask the daemon for a proposal
//! for the text in the compose box, and accept one into a stored draft. Thin
//! over `ComposeAssist` and `ApplyComposeProposal`; the daemon owns the plan,
//! the model call, the checks and the 30 minutes a proposal lives.
//!
//! The web editor applies a proposal to unsaved text itself, so `apply` is for
//! a stored draft whose text still hashes to the proposal's `base`.

use super::routes_v6::dispatch;
use super::*;
use mxr_protocol::{
    AssistActionData, AssistAdjustData, AssistantModelsData, ComposeProposalData, DraftTargetData,
    TextRangeData,
};
use std::str::FromStr;

/// Body of `POST /api/v1/mail/compose/assist`: the fields of the daemon's
/// `ComposeAssist` request.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ComposeAssistBody {
    /// What the help is for: a stored draft, a reply, a forward or a new message.
    draft: DraftTargetData,
    /// The text region of the compose box, without signature or forward header.
    /// Empty when nothing is typed.
    text: String,
    /// A byte range of `text` on character boundaries; the rest is kept as it is.
    #[serde(default)]
    selection: Option<TextRangeData>,
    /// Absent: the daemon picks write, finish or edit from the text.
    #[serde(default)]
    action: Option<AssistActionData>,
    #[serde(default)]
    instruction: Option<String>,
    #[serde(default)]
    adjust: AssistAdjustData,
    /// Optional configured model for this draft only.
    #[serde(default)]
    model: Option<String>,
    /// The id of the proposal on screen, when revising it: the model is shown
    /// that proposal and the instruction is applied to it. `text` stays the
    /// user's own text.
    #[serde(default)]
    revise: Option<String>,
}

/// Answer to `POST /api/v1/mail/compose/assist`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ComposeAssistResponse {
    /// Never touches the draft; accept it by applying `text` or `compose/apply`.
    proposal: ComposeProposalData,
}

/// The models available to the compose writer under its own policy.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ComposeModelsResponse {
    models: AssistantModelsData,
}

/// Body of `POST /api/v1/mail/compose/apply`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ComposeApplyBody {
    proposal_id: String,
    draft_id: String,
}

/// Answer to `POST /api/v1/mail/compose/apply`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub(crate) struct ComposeApplyResponse {
    draft_id: DraftId,
    /// Hex SHA-256 of the draft's new text region.
    text_hash: String,
}

async fn assist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ComposeAssistBody>,
) -> Result<Json<ComposeAssistResponse>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ComposeAssist {
            draft: body.draft,
            text: body.text,
            selection: body.selection,
            action: body.action,
            instruction: body.instruction,
            adjust: body.adjust,
            model: body.model,
            revise: body.revise,
        },
    )
    .await?;
    match response {
        ResponseData::ComposeProposal { proposal } => Ok(Json(ComposeAssistResponse { proposal })),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<ComposeModelsResponse>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::GetComposeModels,
    )
    .await?;
    match response {
        ResponseData::ComposeModels { models } => Ok(Json(ComposeModelsResponse { models })),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(body): Json<ComposeApplyBody>,
) -> Result<Json<ComposeApplyResponse>, BridgeError> {
    let draft_id = DraftId::from_str(&body.draft_id)
        .map_err(|err| BridgeError::BadRequest(format!("invalid draft_id: {err}")))?;
    let response = dispatch(
        &state,
        &headers,
        auth.token.as_deref(),
        Request::ApplyComposeProposal {
            proposal_id: body.proposal_id,
            draft_id,
        },
    )
    .await?;
    match response {
        ResponseData::ComposeApplied {
            draft_id,
            text_hash,
        } => Ok(Json(ComposeApplyResponse {
            draft_id,
            text_hash,
        })),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/compose/models", get(models))
        .route("/compose/assist", post(assist))
        .route("/compose/apply", post(apply))
}
