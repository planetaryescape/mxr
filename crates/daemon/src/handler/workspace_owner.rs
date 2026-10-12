//! Validate captured command ownership before any draft or provider effects.
use crate::state::AppState;
use mxr_protocol::{ArchiveWorkspaceData, IpcErrorKind, Request, Response};

pub(super) fn required(request: &Request) -> bool {
    request.requires_workspace_owner()
}

pub(super) async fn validate(
    state: &AppState,
    request: &Request,
    expected: Option<&ArchiveWorkspaceData>,
) -> Result<(), Box<Response>> {
    if !required(request) && expected.is_none() {
        return Ok(());
    }
    let Some(expected) = expected else {
        return Err(refused("workspace_owner_required", "This command has no reviewed workspace identity. Your private work is retained; explicitly reopen or preview it before continuing."));
    };
    if uuid::Uuid::parse_str(&expected.workspace_id).is_err()
        || uuid::Uuid::parse_str(&expected.recovery_generation).is_err()
    {
        return Err(refused("workspace_owner_invalid", "Workspace identity fields must be UUIDs; retain your private work and reopen the original workspace."));
    }
    let current = state.store.archive_workspace().await.map_err(|error| {
        refused(
            "workspace_owner_unavailable",
            format!("Cannot verify workspace ownership: {error}"),
        )
    })?;
    if current
        != (
            expected.workspace_id.clone(),
            expected.recovery_generation.clone(),
        )
    {
        return Err(refused("workspace_conflict", "Selected workspace or recovery generation changed. Your private work and pending outcomes remain retained; return to the original workspace to recover. This command was not applied."));
    }
    Ok(())
}

fn refused(code: &str, message: impl Into<String>) -> Box<Response> {
    Box::new(Response::Error {
        message: message.into(),
        kind: IpcErrorKind::InvalidRequest,
        code: code.into(),
        retryable: false,
        details: None,
    })
}
