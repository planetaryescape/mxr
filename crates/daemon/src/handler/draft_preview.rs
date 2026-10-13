//! Portable preview uses the reviewed draft and declarations without publishing resources.
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::{Draft, DraftAttachment};
use mxr_protocol::{DraftSafetyContextData, ResponseData};

pub(super) fn require_portable(draft: &Draft) -> Result<(), HandlerError> {
    if !draft.attachments.is_empty() || !draft.inline_assets.is_empty() {
        return Err(HandlerError::PortableDraft);
    }
    Ok(())
}

pub(super) async fn preview(
    state: &AppState,
    draft: &Draft,
    context: &DraftSafetyContextData,
    pending: &[DraftAttachment],
) -> HandlerResult {
    require_portable(draft)?;
    let _guard = state.acquire_draft_operation(&draft.id).await;
    let mut requires_save = true;
    if let Some(stored) = state.store.get_draft(&draft.id).await? {
        require_portable(&stored)?;
        requires_save = super::mutations::send_command_hash(draft)?
            != super::mutations::send_command_hash(&stored)?;
        if draft.revision != stored.revision {
            return Err(HandlerError::DraftConflict {
                expected: draft.revision,
                current: stored.revision,
            });
        }
    }
    // Resolve only managed resources, and keep the provider-local clone off the wire.
    super::draft_attachments::resolve(state, draft).await?;
    let mut safety_draft = super::mutations::materialize_text_alternative(draft).into_owned();
    let mut ids: std::collections::HashSet<_> = draft
        .managed_attachments
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|m| &m.id)
        .collect();
    for metadata in pending {
        super::draft_attachments::preview(metadata)?;
        if metadata.draft_id != draft.id
            || metadata.account_id != draft.account_id
            || !ids.insert(&metadata.id)
        {
            return Err(HandlerError::InvalidRequest(
                "Preview attachment declaration has a conflicting owner or resource ID".into(),
            ));
        }
        safety_draft
            .managed_attachments
            .get_or_insert_default()
            .push(metadata.clone());
    }
    let mut context = context.clone();
    // A dry-run must not run optional model work or mint persisted safety overrides.
    context.allow_llm = false;
    let report = super::mutations::run_safety_pipeline(state, &safety_draft, &context).await?;
    let ResponseData::ResolvedSendFrom { from } =
        super::mutations::resolve_send_from(state, &draft.account_id, draft.from.as_ref()).await?
    else {
        return Err("Unexpected sender resolution".into());
    };
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
    Ok(ResponseData::DraftPreview {
        requires_save: requires_save || !pending.is_empty(),
        draft: draft.clone(),
        from,
        workspace: mxr_protocol::ArchiveWorkspaceData {
            workspace_id,
            recovery_generation,
        },
        send_identity: mxr_core::SendIdentity::for_draft(draft),
        pending_attachments: pending.to_vec(),
        report,
    })
}
