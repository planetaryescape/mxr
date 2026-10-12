use super::{HandlerResult, ResponseData};
use crate::state::AppState;
use mxr_core::{DraftAttachmentId, DraftId, RecoveryActionData};
use mxr_protocol::ArchiveWorkspaceData;
use std::sync::Arc;

async fn owner(
    state: &AppState,
    expected: &ArchiveWorkspaceData,
) -> Result<(), super::HandlerError> {
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
    if expected.workspace_id != workspace_id || expected.recovery_generation != recovery_generation
    {
        return Err(
            "Recovery owner changed; retain the original owner and inspect the selected workspace"
                .into(),
        );
    }
    Ok(())
}
pub(super) async fn status(state: &AppState) -> HandlerResult {
    Ok(ResponseData::RecoveryStatus {
        recovery: state.store.recovery_status().await?,
    })
}

pub(super) async fn activate(
    state: &Arc<AppState>,
    expected: &ArchiveWorkspaceData,
    source_stopped: bool,
    dry_run: bool,
) -> HandlerResult {
    owner(state, expected).await?;
    let status = state.store.recovery_status().await?;
    if !dry_run {
        if !source_stopped {
            return Err("Explicitly confirm the source automation owner is stopped; old intentions remain retired and original drafts remain held".into());
        }
        if status.restore_pending {
            state.store.activate_recovery().await?;
            state
                .restore_pending
                .store(false, std::sync::atomic::Ordering::Release);
            state
                .reload_accounts_from_disk()
                .await
                .map_err(super::HandlerError::Message)?;
        }
    }
    Ok(ResponseData::RecoveryAction {
        action: RecoveryActionData {
            status: state.store.recovery_status().await?,
            prepared_draft: None,
            dry_run,
        },
    })
}

pub(super) async fn prepare(
    state: &AppState,
    id: &DraftId,
    revision: i64,
    expected: &ArchiveWorkspaceData,
    dry_run: bool,
) -> HandlerResult {
    owner(state, expected).await?;
    state
        .ensure_effects_allowed()
        .map_err(super::HandlerError::Message)?;
    let _guard = state.acquire_draft_operation(id).await;
    let status = state.store.recovery_status().await?;
    let held = status
        .held_drafts
        .iter()
        .find(|h| &h.draft_id == id)
        .ok_or("Draft is not a held restored original")?;
    if let Some(prepared) = &held.prepared_draft_id {
        return Ok(ResponseData::RecoveryAction {
            action: RecoveryActionData {
                prepared_draft: state.store.get_draft(prepared).await?,
                status,
                dry_run,
            },
        });
    }
    let mut draft = state
        .store
        .get_draft(id)
        .await?
        .ok_or("Original draft is unavailable; its evidence remains held")?;
    if draft.revision != Some(revision) {
        return Err("Original draft revision changed; preview the current content again".into());
    }
    let new_id = DraftId::from_provider_id(
        "restored-new-review",
        &format!("{}:{id}", expected.recovery_generation),
    );
    let _new_guard = state.acquire_draft_operation(&new_id).await;
    if state.store.get_draft(&new_id).await?.is_some() {
        return Err("Preparation ID already exists without its original hold linkage; inspect retained work".into());
    }
    draft.id = new_id.clone();
    draft.revision = Some(1);
    draft.created_at = chrono::Utc::now();
    draft.updated_at = draft.created_at;
    if let Some(resources) = &mut draft.managed_attachments {
        for resource in resources {
            let original = resource.clone();
            resource.id = DraftAttachmentId::from_provider_id(
                "restored-new-review",
                &format!("{new_id}:{}", original.id),
            );
            resource.draft_id = new_id.clone();
            {
                let source = state
                    .store
                    .draft_attachment_dir()
                    .join(original.id.as_str())
                    .join("final")
                    .join(&original.filename);
                let target = state
                    .store
                    .draft_attachment_dir()
                    .join(resource.id.as_str())
                    .join("final")
                    .join(&resource.filename);
                let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
                use sha2::Digest;
                if bytes.len() as u64 != original.length_bytes
                    || sha2::Sha256::digest(&bytes)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                        != original.sha256
                {
                    return Err(
                        "Original managed attachment bytes changed; preparation stopped".into(),
                    );
                }
                if !dry_run {
                    std::fs::create_dir_all(target.parent().ok_or("New attachment has no parent")?)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
                    if state
                        .store
                        .get_draft_attachment(&resource.id)
                        .await?
                        .is_none()
                    {
                        state.store.insert_draft_attachment(resource).await?;
                    }
                    state.store.finalize_draft_attachment(&resource.id).await?;
                }
            }
        }
    }
    let legacy_root = state
        .store
        .draft_attachment_dir()
        .join(new_id.as_str())
        .join("local");
    for (index, path) in draft
        .attachments
        .iter_mut()
        .chain(draft.inline_assets.iter_mut().map(|a| &mut a.path))
        .enumerate()
    {
        let target = legacy_root.join(format!(
            "{index}-{}",
            path.file_name()
                .and_then(|n| n.to_str())
                .ok_or("Invalid attachment filename")?
        ));
        if path
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Original attachment must not be a symlink".into());
        }
        let bytes = std::fs::read(&*path).map_err(|e| e.to_string())?;
        if !dry_run {
            std::fs::create_dir_all(&legacy_root).map_err(|e| e.to_string())?;
            std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
        }
        *path = target;
    }
    if !dry_run {
        draft = state.store.publish_recovered_draft(id, &draft).await?;
    }
    Ok(ResponseData::RecoveryAction {
        action: RecoveryActionData {
            status: state.store.recovery_status().await?,
            prepared_draft: Some(draft),
            dry_run,
        },
    })
}

pub(super) fn allowed_while_paused(request: &mxr_protocol::Request) -> bool {
    use mxr_protocol::Request::*;
    matches!(
        request,
        ListMessages { .. }
            | ListPlace { .. }
            | GetModeGuide { .. }
            | GetRail { .. }
            | GetArrivals { .. }
            | ListArrivals { .. }
            | GetArrivalModes { .. }
            | ListTodos { .. }
            | GetTodoRunway { .. }
            | ListRecords { .. }
            | GetRecord { .. }
            | GetReadingEdition { .. }
            | GetReadingItem { .. }
            | GetUpdatesDigest { .. }
            | GetPerson { .. }
            | ListInvites { .. }
            | ListSnippets
            | GetDesk { .. }
            | GetNow { .. }
            | GetThreadGist { .. }
            | GetThreadGists { .. }
            | GetRecoveryStatus
            | ActivateRecovery { .. }
            | GetStatus
            | Shutdown
            | Ping
            | GetArchiveWorkspace
            | GetArchiveOperation { .. }
            | ListJobs
            | GetJob { .. }
            | ListEnvelopes { .. }
            | ListEnvelopesByIds { .. }
            | GetEnvelope { .. }
            | GetBody { .. }
            | ListBodies { .. }
            | GetThread { .. }
            | ListThreads { .. }
            | ListLabels { .. }
            | ListAccounts
            | ListAccountsConfig
            | Search { .. }
            | SearchAggregation { .. }
            | ListCategories { .. }
            | ListCategoryMessages { .. }
            | GetCategoryMemberships { .. }
            | ListSavedSearches
            | ListSavedSearchUnreadCounts
            | GetDraft { .. }
            | UpdateDraft { .. }
            | ListDrafts
            | ListOrphanedDrafts
            | SaveDraft { .. }
            | PrepareReply { .. }
            | PrepareForward { .. }
            | GetSendOutcome { .. }
            | CheckDraftSafety { .. }
            | PreviewDraftAttachment { .. }
            | BeginDraftAttachment { .. }
            | WriteDraftAttachmentChunk { .. }
            | FinalizeDraftAttachment { .. }
            | GetAttachmentTransferMetadata { .. }
            | ReadAttachmentChunk { .. }
            | DownloadAttachment { .. }
            | GetHtmlImageAssets {
                allow_remote: false,
                ..
            }
            | GetLogs { .. }
            | ListEvents { .. }
            | ListEventCategories
            | GetDoctorReport
            | GetSyncStatus { .. }
            | GetFreshness { .. }
            | GetLlmStatus
            | GetSemanticStatus
            | ListRules
            | GetRule { .. }
            | ListScheduledSends { .. }
    )
}

#[cfg(test)]
mod tests;
