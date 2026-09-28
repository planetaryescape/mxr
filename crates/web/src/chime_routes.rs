//! The shared notification chime setting, for the web's sound palette.
//!
//! One setting drives every client: the daemon plays it natively for the
//! TUI and CLI, and the web plays the same sound names in the browser. These
//! routes read the setting and change fields of it; previews play
//! client-side.

use super::routes_v6::passthrough;
use super::*;
use mxr_protocol::NotificationChimesPatchData;

async fn get_chimes(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    let response = ipc_request(&state.config.socket_path, Request::GetNotificationChimes).await?;
    match response {
        ResponseData::NotificationChimes { .. } => passthrough(response),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

async fn update_chimes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(patch): Json<NotificationChimesPatchData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, None, &state.config.auth_token)?;
    // A patch, applied by the daemon under its config lock: saves from two
    // tabs, or from the web and `mxr chimes`, never undo each other.
    let response = ipc_request(
        &state.config.socket_path,
        Request::PatchNotificationChimes {
            patch: Box::new(patch),
        },
    )
    .await?;
    match response {
        ResponseData::NotificationChimes { .. } => passthrough(response),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn extend_platform(router: Router<AppState>) -> Router<AppState> {
    router.route("/notifications/chimes", get(get_chimes).post(update_chimes))
}
