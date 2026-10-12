//! Immutable HTTP command context, retained across connection-per-request IPC.
use crate::{auth::BridgeError, AppState};
use axum::{extract::{Request, State}, middleware::Next, response::{IntoResponse, Response}};
use mxr_protocol::ArchiveWorkspaceData;

tokio::task_local! {
    static OWNER: Option<ArchiveWorkspaceData>;
}

pub(super) fn captured() -> Option<ArchiveWorkspaceData> {
    OWNER.try_with(Clone::clone).ok().flatten()
}

pub(super) async fn middleware(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let owner = match request.headers().get("x-mxr-workspace") {
        Some(header) => match serde_json::from_slice::<ArchiveWorkspaceData>(header.as_bytes()) {
            Ok(owner) => Some(owner),
            Err(_) => return BridgeError::BadRequest("Invalid captured workspace identity".into()).into_response(),
        },
        None => None,
    };
    let authenticated = crate::auth::token_matches(crate::auth::extract_token(request.headers()), &state.config.auth_token);
    let mut actual = None;
    if authenticated {
        let mut connection = match mxr_client::IpcConnection::connect(&state.config.socket_path, mxr_protocol::ClientKind::Web).await {
            Ok(connection) => connection,
            Err(error) => return crate::map_bridge_error(error).into_response(),
        };
        match connection.request(mxr_protocol::Request::GetArchiveWorkspace).await {
            Ok(mxr_protocol::ResponseData::ArchiveWorkspace { workspace }) => actual = Some(workspace),
            Ok(_) => return BridgeError::UnexpectedResponse.into_response(),
            Err(error) => return crate::map_bridge_error(error).into_response(),
        }
        // The body is still unread. A stale request cannot create files or upload bytes.
        if let Some(expected) = &owner {
            if let Err(error) = connection.verify_workspace(expected).await {
                let mut response = crate::map_bridge_error(error).into_response();
                if let Some(actual) = &actual {
                    if let Ok(header) = axum::http::HeaderValue::from_str(&serde_json::to_string(actual).unwrap_or_default()) {
                        response.headers_mut().insert("x-mxr-workspace", header);
                    }
                }
                return response;
            }
        }
    } else if owner.is_some() {
        return BridgeError::Unauthorized.into_response();
    }

    let mut response = OWNER.scope(owner, next.run(request)).await;
    if let Some(actual) = actual {
        if let Ok(header) = axum::http::HeaderValue::from_str(&serde_json::to_string(&actual).unwrap_or_default()) {
            response.headers_mut().insert("x-mxr-workspace", header);
        }
    }
    response
}

pub(super) async fn sidecar_owner(path: &std::path::Path) -> Result<Option<ArchiveWorkspaceData>, BridgeError> {
    match tokio::fs::read(crate::draft_session_metadata_path(path)).await {
        Ok(bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| BridgeError::Ipc(error.to_string()))?;
            value.get("compose_workspace").cloned().map(serde_json::from_value).transpose().map_err(|error| BridgeError::Ipc(error.to_string()))
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(BridgeError::Ipc(error.to_string())),
    }
}

pub(super) async fn require_sidecar(path: &std::path::Path) -> Result<(), BridgeError> {
    let original = sidecar_owner(path).await?;
    if original.is_none() || original != captured() {
        return Err(BridgeError::ArchiveConflict {
            message: "This editor belongs to another workspace or has no retained owner. Your edits stay private; reopen or return to the original workspace to recover.".into(),
            kind: mxr_protocol::IpcErrorKind::InvalidRequest,
            code: "workspace_conflict".into(), details: None,
        });
    }
    Ok(())
}
