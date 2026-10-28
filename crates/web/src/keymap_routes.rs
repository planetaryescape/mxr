//! The key map over the bridge (blueprint chapter 35): the web app reads the
//! definition with the person's `keys.toml` applied when it starts, and its
//! settings page changes `keys.toml` through the daemon, previewing first.

use super::routes_v6::passthrough;
use super::*;
use mxr_protocol::KeysChangeData;

async fn get_keymap(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let response = ipc_request(&state.config.socket_path, Request::GetKeymap).await?;
    match response {
        ResponseData::Keymap { .. } => passthrough(response),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

/// Body of `POST /api/v1/platform/keymap`.
#[derive(Debug, Deserialize)]
struct KeysUpdateBody {
    change: KeysChangeData,
    /// True previews the change and its conflicts without writing.
    #[serde(default)]
    dry_run: bool,
}

async fn update_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<KeysUpdateBody>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let response = ipc_request(
        &state.config.socket_path,
        Request::UpdateKeys {
            change: body.change,
            dry_run: body.dry_run,
        },
    )
    .await?;
    match response {
        ResponseData::KeysUpdate { .. } => passthrough(response),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn extend_platform(router: Router<AppState>) -> Router<AppState> {
    router.route("/keymap", get(get_keymap).post(update_keys))
}
