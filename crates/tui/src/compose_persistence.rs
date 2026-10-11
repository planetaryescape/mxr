use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::{Draft, MxrError, SendIdentity, SendOutcomeState};
use mxr_protocol::{Request, Response, ResponseData};
use std::path::Path;
use tokio::sync::mpsc;

/// Saving or scheduling the retained file must prepare the same canonical edit as Send.
pub(crate) async fn prepare_reviewed_compose(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    mut draft: Draft,
    file: &Path,
) -> Result<(Draft, SendIdentity), MxrError> {
    let retained = crate::send_recovery_state::for_draft(&draft.id)
        .map_err(|error| MxrError::Ipc(format!("Cannot read send recovery: {error}")))?;
    if retained
        .as_ref()
        .is_some_and(|(account, _)| account != &draft.account_id)
    {
        return Err(MxrError::Ipc(
            "Reviewed draft belongs to another account".into(),
        ));
    }
    let identity =
        retained.map_or_else(|| SendIdentity::for_draft(&draft), |(_, identity)| identity);
    draft.revision = Some(identity.expected_revision);
    match ipc_call(
        bg,
        Request::GetSendOutcome {
            account_id: draft.account_id.clone(),
            operation_id: format!("draft:{}", draft.id),
        },
    )
    .await?
    {
        Response::Ok {
            data: ResponseData::SendOutcome {
                outcome: Some(outcome),
            },
        } if matches!(
            outcome.state,
            SendOutcomeState::Unknown | SendOutcomeState::Accepted
        ) =>
        {
            return Err(MxrError::Ipc("This draft may already have been sent; check its outcome before saving or scheduling. Do not resend.".into()));
        }
        Response::Ok {
            data: ResponseData::SendOutcome { .. },
        } => {}
        Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
        _ => return Err(MxrError::Ipc("Unexpected send outcome response".into())),
    }
    let (draft, mut identity) =
        crate::send_recovery::prepare_reviewed_send(bg, draft, identity).await?;
    // A new file has no local row yet. SaveDraft is idempotent for an already
    // prepared canonical row, and returns the authoritative initial revision.
    let draft = match ipc_call(bg, Request::SaveDraft { draft }).await? {
        Response::Ok {
            data: ResponseData::Draft { draft },
        } => draft,
        Response::Error { message, .. } => return Err(MxrError::Ipc(message)),
        _ => {
            return Err(MxrError::Ipc(
                "Unexpected canonical draft save response".into(),
            ))
        }
    };
    identity.expected_revision = draft
        .revision
        .ok_or_else(|| MxrError::Ipc("Saved draft has no reviewed revision".into()))?;
    #[cfg(not(test))]
    crate::send_recovery_state::retain_reviewed_compose(
        &draft.account_id,
        &draft.id,
        &identity,
        file,
    )
    .map_err(|error| MxrError::Ipc(format!("Cannot retain reviewed draft revision: {error}")))?;
    #[cfg(test)]
    let _ = file;
    Ok((draft, identity))
}

pub(crate) async fn prepare_compose_schedule(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    pending: &mut crate::app::PendingPlatformDispatch,
) -> Result<Option<(mxr_core::AccountId, SendIdentity, mxr_core::DraftId)>, MxrError> {
    let Some(compose) = &pending.compose else {
        return Ok(None);
    };
    let Request::ScheduleSend {
        draft_id, send_at, ..
    } = &pending.request
    else {
        return Ok(None);
    };
    let draft = pending
        .prelude
        .iter()
        .find_map(|request| match request {
            Request::SaveDraft { draft } => Some(draft.clone()),
            _ => None,
        })
        .ok_or_else(|| MxrError::Ipc("Missing reviewed schedule draft".into()))?;
    let draft_id = draft_id.clone();
    let send_at = *send_at;
    let (draft, identity) = prepare_reviewed_compose(bg, draft, &compose.draft_path).await?;
    pending.request = Request::ScheduleSend {
        draft_id: draft_id.clone(),
        send_at,
        expected_revision: draft.revision,
    };
    pending.prelude.clear();
    #[cfg(not(test))]
    crate::send_recovery_state::retain_schedule_query(&draft.account_id, &draft.id, &identity)
        .map_err(|error| {
            MxrError::Ipc(format!("Cannot retain scheduled outcome query: {error}"))
        })?;
    Ok(Some((draft.account_id, identity, draft_id)))
}
