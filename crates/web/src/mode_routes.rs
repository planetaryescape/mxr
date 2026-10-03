//! How each mode explains itself, and its first-encounter card's seen
//! state. Thin passthroughs to the daemon, which owns the copy table.

use super::routes_v6::{dispatch, passthrough};
use super::*;

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

/// Body of `POST /api/v1/mail/modes/{mode}/card`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub(crate) struct ModeCardBody {
    /// True retires the card in every client; false shows it again.
    #[serde(default = "seen_default")]
    seen: bool,
}

fn seen_default() -> bool {
    true
}

async fn set_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(mode): AxumPath<String>,
    Json(body): Json<ModeCardBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    let response = dispatch(
        &state,
        &headers,
        None,
        Request::SetModeGuideSeen {
            mode,
            seen: body.seen,
        },
    )
    .await?;
    passthrough(response)
}

pub(crate) fn extend_mail(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/modes/guide", get(get_guide))
        .route("/modes/{mode}/card", post(set_card))
}
