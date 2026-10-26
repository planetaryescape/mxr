//! A retained approval can retry only after positive non-submission evidence.
//! Ambiguous attempts are reconciled, never dispatched a second time.
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use mxr_core::{
    Address, Draft, DraftSendOutcome, SendFailure, SendIdentity, SendOutcomeState,
    SendSubmissionPhase,
};
use mxr_protocol::{DaemonEvent, ResponseData};
use mxr_store::{ApprovedSendFile, SendApproval};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_SEND_ATTEMPTS: u32 = 12;

fn recovery_preview(
    outcome: DraftSendOutcome,
    reason: &str,
    evidence_sha256: Option<String>,
) -> HandlerResult {
    Ok(ResponseData::SendRecoveryPreview {
        outcome,
        recoverable: evidence_sha256.is_some(),
        reason: reason.into(),
        evidence_sha256,
    })
}

fn legacy_gate_evidence(
    log_dir: &Path,
    operation_id: &str,
    recorded_at: DateTime<Utc>,
    provider: &dyn mxr_core::MailSendProvider,
) -> Result<Option<(String, DateTime<Utc>)>, HandlerError> {
    use std::io::{BufRead, BufReader, Read};
    let _rotation_guard = crate::logging::lock(log_dir, false).map_err(|e| e.to_string())?;
    let mut proof = None;
    let mut remaining_bytes = 64 * 1024 * 1024_u64;
    for file in crate::logging::files(log_dir).map_err(|e| e.to_string())? {
        let file = crate::logging::open_read(&file).map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        let Some(remaining) = remaining_bytes.checked_sub(length) else {
            return Ok(None);
        };
        remaining_bytes = remaining;
        let mut reader = BufReader::new(file.take(length));
        loop {
            let mut line = Vec::new();
            let read = reader
                .by_ref()
                .take((crate::logging::MAX_RECORD_BYTES + 1) as u64)
                .read_until(b'\n', &mut line)
                .map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            if read > crate::logging::MAX_RECORD_BYTES || line.last() != Some(&b'\n') {
                return Ok(None);
            }
            line.pop();
            let Ok(record) = serde_json::from_slice::<serde_json::Value>(&line) else {
                continue;
            };
            if record["target"] != "mxr::handler::mutations"
                || record["fields"]["operation_id"] != operation_id
            {
                continue;
            }
            let Some(timestamp) = record["timestamp"]
                .as_str()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            else {
                continue;
            };
            if (timestamp.with_timezone(&Utc) - recorded_at)
                .num_seconds()
                .unsigned_abs()
                > 120
            {
                continue;
            }
            let Some(message) = record["fields"]["message"].as_str() else {
                continue;
            };
            if !message.starts_with("provider acceptance is uncertain:") {
                continue;
            }
            let Some(seconds) = provider.legacy_cooldown_retry_after_secs(message) else {
                return Ok(None);
            };
            let Some(deadline) = i64::try_from(seconds)
                .ok()
                .and_then(chrono::Duration::try_seconds)
                .and_then(|wait| timestamp.with_timezone(&Utc).checked_add_signed(wait))
            else {
                return Ok(None);
            };
            let hash = digest(&line);
            if proof
                .as_ref()
                .is_some_and(|(existing, _)| existing != &hash)
            {
                return Ok(None);
            }
            proof = Some((hash, deadline));
        }
    }
    Ok(proof)
}

pub(super) async fn recover_legacy(
    state: &AppState,
    account_id: &mxr_core::AccountId,
    operation_id: &str,
    dry_run: bool,
    expected_evidence: Option<&str>,
) -> HandlerResult {
    state
        .ensure_effects_allowed()
        .map_err(HandlerError::Message)?;
    let (outcome, hash) = state
        .store
        .get_send_outcome(operation_id)
        .await?
        .ok_or("Send operation not found")?;
    if &outcome.account_id != account_id {
        return Err("Send outcome not found for this account".into());
    }
    let _guard = state.acquire_draft_operation(&outcome.draft_id).await;
    let (outcome, _) = state
        .store
        .get_send_outcome(operation_id)
        .await?
        .ok_or("Send operation not found")?;
    if outcome.state != SendOutcomeState::Unknown || outcome.recovery.is_some() {
        return recovery_preview(outcome, "not_a_legacy_unknown", None);
    }
    let provider = state.send_provider_for_account(account_id)?;
    let Some(draft) = state.store.get_draft(&outcome.draft_id).await? else {
        return recovery_preview(outcome, "draft_missing", None);
    };
    if state.store.is_restored_draft(&draft.id).await?
        || state.store.get_draft_status(&draft.id).await? != Some(mxr_core::DraftStatus::Sending)
    {
        return recovery_preview(outcome, "legacy_draft_not_held_for_submission", None);
    }
    if draft.account_id != *account_id
        || draft.revision != Some(outcome.identity.expected_revision)
        || super::mutations::send_command_hash(&draft)? != hash
    {
        return recovery_preview(outcome, "approved_draft_changed", None);
    }
    if !draft.attachments.is_empty() || !draft.inline_assets.is_empty() {
        return recovery_preview(
            outcome,
            "legacy_path_bytes_have_no_durable_review_hash",
            None,
        );
    }
    if draft.from.is_none() {
        return recovery_preview(outcome, "original_sender_was_not_retained", None);
    }
    let Some(message_id) = state.store.get_draft_message_id_header(&draft.id).await? else {
        return recovery_preview(outcome, "original_message_id_missing", None);
    };
    if outcome
        .rfc2822_message_id
        .as_ref()
        .is_some_and(|id| id != &message_id)
    {
        return recovery_preview(outcome, "message_id_conflicts", None);
    }
    let recorded_at = state.store.send_outcome_recorded_at(operation_id).await?;
    let log_dir = state
        .store
        .draft_attachment_dir()
        .parent()
        .ok_or("Missing mailbox directory")?
        .join("logs");
    let operation = operation_id.to_string();
    let evidence = tokio::task::spawn_blocking(move || {
        legacy_gate_evidence(&log_dir, &operation, recorded_at, provider.as_ref())
    })
    .await
    .map_err(|e| e.to_string())??;
    let Some((evidence, original_deadline)) = evidence else {
        return recovery_preview(outcome, "no_definite_pre_dispatch_evidence", None);
    };
    let resolved = super::draft_attachments::resolve(state, &draft).await?;
    super::mutations::validate_draft_content(&resolved)?;
    let from =
        super::mutations::resolve_from_address(state, account_id, draft.from.as_ref()).await?;
    if dry_run {
        return recovery_preview(
            outcome,
            "local_gmail_gate_proves_non_submission",
            Some(evidence),
        );
    }
    if expected_evidence != Some(evidence.as_str()) {
        return Err("Review recovery with --dry-run and retain --expected-evidence-sha256 before resuming this original operation".into());
    }
    let revision = draft.revision.ok_or("Draft revision missing")?;
    let mut approval = prepare(state, &outcome.identity, &resolved, &from, revision, hash).await?;
    approval.legacy_evidence_sha256 = Some(evidence.clone());
    let recovered = match state
        .store
        .recover_legacy_send(
            &outcome.identity,
            &approval,
            &message_id,
            original_deadline.max(Utc::now() + chrono::Duration::seconds(60)),
        )
        .await
    {
        Ok(recovered) => recovered,
        Err(error) => {
            discard_files(state, &outcome.identity, &approval).await;
            return Err(error.into());
        }
    };
    if !recovered {
        discard_files(state, &outcome.identity, &approval).await;
        return Err("Recovery evidence or reviewed draft changed; run the preview again".into());
    }
    tracing::info!(operation_id, evidence_sha256 = %evidence, "Historical approved send resumed from matched local pre-dispatch evidence");
    notify_current(state, &outcome.identity).await;
    Ok(ResponseData::SendOutcome {
        outcome: state
            .store
            .get_send_outcome(operation_id)
            .await?
            .map(|(outcome, _)| outcome),
    })
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn approval_dir(state: &AppState, identity: &SendIdentity) -> PathBuf {
    state
        .store
        .draft_attachment_dir()
        .join("approved-sends")
        .join(digest(identity.operation_id.as_bytes()))
}

fn private_directory(path: &Path) -> Result<(), HandlerError> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.to_string().into()),
    }
    let metadata = path.symlink_metadata().map_err(|e| e.to_string())?;
    if !metadata.is_dir() {
        return Err("Approved-send cache must be a private directory, not a link".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        // Work through a no-follow directory handle so a replaced path cannot
        // make permission repair chmod a symlink target.
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| e.to_string())?;
        let metadata = directory.metadata().map_err(|e| e.to_string())?;
        let own_uid = tempfile::tempfile()
            .and_then(|file| file.metadata())
            .map_err(|e| e.to_string())?
            .uid();
        if metadata.uid() != own_uid {
            return Err("Approved-send cache has unsafe ownership".into());
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            // Older restore paths created owned resource directories using the
            // process umask. Tighten those before publishing frozen send bytes.
            directory
                .set_permissions(std::fs::Permissions::from_mode(0o700))
                .and_then(|()| directory.sync_all())
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn read_approved_file(path: &Path) -> Result<Vec<u8>, HandlerError> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                HandlerError::from("Cannot read approved attachment bytes")
            }
            _ => HandlerError::Core(mxr_core::MxrError::Io(error)),
        })?;
    if !file
        .metadata()
        .await
        .map_err(mxr_core::MxrError::Io)?
        .is_file()
    {
        return Err("Approved attachment must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(mxr_protocol::MAX_DRAFT_ATTACHMENT_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(mxr_core::MxrError::Io)?;
    if bytes.len() as u64 > mxr_protocol::MAX_DRAFT_ATTACHMENT_BYTES {
        return Err("Approved-send attachments have a 25 MiB per-file limit".into());
    }
    Ok(bytes)
}

async fn freeze_file(
    path: &Path,
    directory: &Path,
    index: usize,
) -> Result<ApprovedSendFile, HandlerError> {
    let bytes = read_approved_file(path).await?;
    let directory = directory.join(index.to_string());
    private_directory(&directory)?;
    let target = directory.join(path.file_name().ok_or("Attachment has no filename")?);
    mxr_compose::private_tmp::write_private(&target, &bytes).map_err(|e| e.to_string())?;
    std::fs::File::open(&target)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o400))
            .await
            .map_err(|e| e.to_string())?;
    }
    std::fs::File::open(&directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(ApprovedSendFile {
        path: target,
        length: bytes.len() as u64,
        sha256: digest(&bytes),
    })
}

pub(super) async fn prepare(
    state: &AppState,
    identity: &SendIdentity,
    draft: &Draft,
    from: &Address,
    stored_revision: i64,
    command_hash: String,
) -> Result<SendApproval, HandlerError> {
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
    let mut approved = draft.clone();
    let directory = approval_dir(state, identity);
    // A failed preparation owns only its unique temporary snapshot. Retrying
    // cannot overwrite a retained approval or get stuck on a partial file.
    let snapshot = if draft.attachments.is_empty() && draft.inline_assets.is_empty() {
        None
    } else {
        private_directory(state.store.draft_attachment_dir())?;
        private_directory(directory.parent().ok_or("Missing approved-send cache")?)?;
        private_directory(&directory)?;
        Some(
            tempfile::Builder::new()
                .prefix("snapshot-")
                .tempdir_in(&directory)
                .map_err(|e| e.to_string())?,
        )
    };
    let snapshot_path = snapshot
        .as_ref()
        .map_or(directory.as_path(), |snapshot| snapshot.path());
    let mut files = Vec::new();
    for path in &mut approved.attachments {
        let file = freeze_file(path, snapshot_path, files.len()).await?;
        *path = file.path.clone();
        files.push(file);
    }
    for asset in &mut approved.inline_assets {
        let file = freeze_file(&asset.path, snapshot_path, files.len()).await?;
        asset.path = file.path.clone();
        files.push(file);
    }
    // A resource may have been tampered with between resolution and copying.
    // The immutable review's advertised hash, not the freshly copied bytes,
    // remains authoritative.
    let managed: Vec<_> = draft
        .managed_attachments
        .iter()
        .flatten()
        .filter(|part| part.content_id.is_none())
        .collect();
    let start = approved
        .attachments
        .len()
        .checked_sub(managed.len())
        .ok_or("Managed attachments were not resolved")?;
    for (index, part) in managed.iter().enumerate() {
        let frozen = &files[start + index];
        if frozen.length != part.length_bytes || frozen.sha256 != part.sha256 {
            return Err("Approved attachment differs from its reviewed immutable bytes".into());
        }
    }
    for (index, asset) in draft.inline_assets.iter().enumerate() {
        if let Some(part) = draft
            .managed_attachments
            .iter()
            .flatten()
            .find(|part| part.content_id.as_deref() == Some(asset.cid.as_str()))
        {
            let frozen = &files[draft.attachments.len() + index];
            if frozen.length != part.length_bytes || frozen.sha256 != part.sha256 {
                return Err(
                    "Approved inline image differs from its reviewed immutable bytes".into(),
                );
            }
        }
    }
    if let Some(snapshot) = snapshot {
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| e.to_string())?;
        let _ = snapshot.keep();
    }
    Ok(SendApproval {
        draft: approved,
        from: from.clone(),
        workspace_id,
        recovery_generation,
        stored_revision,
        command_hash,
        files,
        legacy_evidence_sha256: None,
    })
}

async fn validate_approval(state: &AppState, approval: &SendApproval) -> Result<(), HandlerError> {
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
    if workspace_id != approval.workspace_id || recovery_generation != approval.recovery_generation
    {
        return Err("workspace_changed".into());
    }
    let current = state
        .store
        .get_draft(&approval.draft.id)
        .await?
        .ok_or("draft_removed")?;
    if current.revision != Some(approval.stored_revision)
        || current.account_id != approval.draft.account_id
        || super::mutations::send_command_hash(&current)? != approval.command_hash
    {
        return Err("draft_changed".into());
    }
    let from =
        super::mutations::resolve_from_address(state, &current.account_id, current.from.as_ref())
            .await?;
    if from != approval.from {
        return Err("sender_changed".into());
    }
    validate_files(approval).await?;
    super::mutations::validate_draft_content(&approval.draft)?;
    Ok(())
}

async fn validate_files(approval: &SendApproval) -> Result<(), HandlerError> {
    for file in &approval.files {
        let bytes = read_approved_file(&file.path).await?;
        if bytes.len() as u64 != file.length || digest(&bytes) != file.sha256 {
            return Err("approved_attachment_changed".into());
        }
    }
    Ok(())
}

fn notify(state: &AppState, outcome: &DraftSendOutcome) {
    let operation_id = outcome.identity.operation_id.clone();
    let operation = "send_recovery".to_string();
    let account_id = Some(outcome.account_id.clone());
    let event = match outcome.state {
        SendOutcomeState::Waiting => DaemonEvent::OperationProgress {
            operation_id,
            operation,
            account_id,
            current: outcome
                .recovery
                .as_ref()
                .map_or(0, |status| status.attempts),
            total: Some(MAX_SEND_ATTEMPTS),
            message: format!(
                "Approved send waiting for provider; automatic retry at {}",
                outcome
                    .recovery
                    .as_ref()
                    .and_then(|status| status.retry_at)
                    .map_or_else(|| "pending".into(), |at| at.to_rfc3339())
            ),
        },
        SendOutcomeState::Accepted => DaemonEvent::OperationCompleted {
            operation_id,
            operation,
            account_id,
            message: "Mail provider accepted the original approved send".into(),
        },
        SendOutcomeState::Unknown => DaemonEvent::OperationFailed {
            operation_id,
            operation,
            account_id,
            error: "Acceptance uncertain; checking provider evidence without resending".into(),
            retryable: false,
        },
        SendOutcomeState::NotAccepted => DaemonEvent::OperationFailed {
            operation_id,
            operation,
            account_id,
            error: match outcome.recovery.as_ref().and_then(|status| status.reason.as_deref()) {
                Some("approval_invalidated") => "Approved draft, workspace, sender or attachment changed; review the current draft before sending",
                Some("retry_limit_reached") => "Automatic send retries exhausted; check the account/provider, then review before a new send",
                Some("account_unavailable") => "Sending account unavailable; restore its configuration and review the draft before sending",
                _ if outcome.recovery.as_ref().is_some_and(|status| status.phase == SendSubmissionPhase::Authentication) => "Sending authentication failed; reconnect the account and review before a new send",
                _ => "Approved send not accepted; check Send outcomes and review before a new send",
            }.into(),
            retryable: false,
        },
    };
    crate::chimes::emit_daemon_event(state, event);
}

pub(super) async fn notify_current(state: &AppState, identity: &SendIdentity) {
    if let Ok(Some((outcome, _))) = state.store.get_send_outcome(&identity.operation_id).await {
        notify(state, &outcome);
    }
}

/// Remove only the exact files created by this operation; never recursively
/// delete a cache root or follow a client-supplied attachment path.
pub(super) async fn discard_files(
    state: &AppState,
    identity: &SendIdentity,
    approval: &SendApproval,
) {
    let root = approval_dir(state, identity);
    for file in &approval.files {
        if !file.path.starts_with(&root) {
            continue;
        }
        if tokio::fs::remove_file(&file.path).await.is_ok() {
            if let Some(parent) = file.path.parent() {
                let _ = tokio::fs::remove_dir(parent).await;
                if let Some(snapshot) = parent
                    .parent()
                    .filter(|snapshot| snapshot.starts_with(&root))
                {
                    let _ = tokio::fs::remove_dir(snapshot).await;
                }
            }
        }
    }
    let _ = tokio::fs::remove_dir(root).await;
}

pub(super) async fn failure(
    state: &AppState,
    identity: &SendIdentity,
    failure: SendFailure,
    portable: bool,
    now: DateTime<Utc>,
) -> HandlerResult {
    let (phase, retry_after_secs, error) = match failure {
        SendFailure::NotSubmitted {
            phase,
            retry_after_secs,
            error,
        } => (phase, retry_after_secs, error),
        SendFailure::NotAccepted(error) => (SendSubmissionPhase::ProviderResponse, None, error),
        SendFailure::Unknown(error) => {
            state
                .store
                .record_send_recovery(
                    identity,
                    "unknown",
                    SendSubmissionPhase::Dispatch,
                    Some(now + chrono::Duration::seconds(60)),
                    Some("acceptance_uncertain"),
                )
                .await?;
            tracing::warn!(operation_id = %identity.operation_id, "provider acceptance is uncertain: {error}");
            let (outcome, _) = state
                .store
                .get_send_outcome(&identity.operation_id)
                .await?
                .ok_or("Send outcome missing")?;
            notify(state, &outcome);
            return Err(HandlerError::SendOutcome {
                outcome: Box::new(outcome),
                diagnostic: (!portable).then(|| error.to_string()),
            });
        }
    };
    let attempts = state
        .store
        .get_send_recovery_status(&identity.operation_id)
        .await?
        .map_or(1, |(_, status)| status.attempts);
    if let Some(hint) = retry_after_secs.filter(|_| attempts < MAX_SEND_ATTEMPTS) {
        // Never retry earlier than the provider's deadline. A stable per-job
        // jitter avoids a restart or concurrent clients producing a stampede.
        let jitter = u64::from(Sha256::digest(identity.operation_id.as_bytes())[0] % 31);
        let backoff = 60_u64
            .saturating_mul(1 << attempts.saturating_sub(1).min(6))
            .min(3600);
        let delay = hint.max(backoff).saturating_add(jitter);
        let retry_at = i64::try_from(delay)
            .ok()
            .and_then(chrono::Duration::try_seconds)
            .and_then(|delay| now.checked_add_signed(delay));
        if let Some(retry_at) = retry_at {
            state
                .store
                .record_send_recovery(
                    identity,
                    "waiting",
                    phase,
                    Some(retry_at),
                    Some("provider_temporarily_unavailable"),
                )
                .await?;
            tracing::info!(operation_id = %identity.operation_id, retry_at = %retry_at, "Approved send waiting for provider; retry scheduled");
            let (outcome, _) = state
                .store
                .get_send_outcome(&identity.operation_id)
                .await?
                .ok_or("Send outcome missing")?;
            notify(state, &outcome);
            return Ok(ResponseData::SendOutcome {
                outcome: Some(outcome),
            });
        }
        // A malformed/unrepresentable hint cannot erase known non-submission
        // or authorize an earlier retry. Retain the phase and require action.
    }
    let reason = if attempts >= MAX_SEND_ATTEMPTS {
        "retry_limit_reached"
    } else {
        "action_required"
    };
    state
        .store
        .record_send_recovery(identity, "failed", phase, None, Some(reason))
        .await?;
    state.store.reject_draft_send(identity).await?;
    tracing::warn!(operation_id = %identity.operation_id, "Provider did not accept approved send: {error}");
    let (outcome, _) = state
        .store
        .get_send_outcome(&identity.operation_id)
        .await?
        .ok_or("Send outcome missing")?;
    notify(state, &outcome);
    if portable {
        Err(HandlerError::SendOutcome {
            outcome: Box::new(outcome),
            diagnostic: None,
        })
    } else {
        Err(format!(
            "Provider did not accept send operation {} ({reason}): {error}",
            identity.operation_id
        )
        .into())
    }
}

/// Caller holds the draft-operation lock. Provider evidence never authorizes
/// a resend; positive evidence may finish the retained acceptance instead.
pub(super) async fn reconcile_locked(
    state: &AppState,
    outcome: DraftSendOutcome,
    now: DateTime<Utc>,
) -> HandlerResult {
    let Some(approval) = state
        .store
        .get_send_approval(&outcome.identity.operation_id)
        .await?
    else {
        return Ok(ResponseData::SendOutcome {
            outcome: Some(outcome),
        });
    };
    state
        .ensure_effects_allowed()
        .map_err(HandlerError::Message)?;
    let (workspace_id, generation) = state.store.archive_workspace().await?;
    if approval.workspace_id != workspace_id || approval.recovery_generation != generation {
        // Evidence and local completion must not follow a restored/new owner.
        state
            .store
            .postpone_send_recovery(
                &outcome.identity.operation_id,
                now + chrono::Duration::hours(1),
            )
            .await?;
        return Ok(ResponseData::SendOutcome {
            outcome: Some(outcome),
        });
    }
    if outcome
        .recovery
        .as_ref()
        .and_then(|status| status.retry_at)
        .is_some_and(|at| at > now)
    {
        return Ok(ResponseData::SendOutcome {
            outcome: Some(outcome),
        });
    }
    if outcome.state == SendOutcomeState::Unknown {
        if let Some(message_id) = outcome.rfc2822_message_id.as_deref() {
            let sender = state.send_provider_for_account(&outcome.account_id)?;
            if let Ok(Some(receipt)) = sender.reconcile_send(message_id, &approval.from).await {
                // Acceptance is independent of local attachment availability.
                // Persist it first; a missing cache blocks only local repair.
                state
                    .store
                    .accept_draft_send(&outcome.identity, &receipt)
                    .await?;
                notify_current(state, &outcome.identity).await;
                if validate_files(&approval).await.is_err() {
                    return accepted_needs_repair(state, &outcome.identity, now).await;
                }
                return super::mutations::finish_accepted_send(
                    state,
                    &approval.draft,
                    &approval.from,
                    &outcome.identity,
                    &receipt,
                    true,
                )
                .await;
            }
        }
        state
            .store
            .record_send_recovery(
                &outcome.identity,
                "unknown",
                SendSubmissionPhase::Dispatch,
                Some(now + chrono::Duration::seconds(300)),
                Some("acceptance_uncertain"),
            )
            .await?;
    } else if outcome.state == SendOutcomeState::Accepted && outcome.local_message_id.is_none() {
        if validate_files(&approval).await.is_err() {
            return accepted_needs_repair(state, &outcome.identity, now).await;
        }
        if let Some(message_id) = outcome.rfc2822_message_id.clone() {
            let receipt = mxr_core::SendReceipt {
                provider_message_id: outcome.provider_message_id.clone(),
                rfc2822_message_id: message_id,
                sent_at: state
                    .store
                    .approved_send_accepted_at(&outcome.identity.operation_id)
                    .await?
                    .unwrap_or_else(Utc::now),
            };
            return super::mutations::finish_accepted_send(
                state,
                &approval.draft,
                &approval.from,
                &outcome.identity,
                &receipt,
                true,
            )
            .await;
        }
    }
    let outcome = state
        .store
        .get_send_outcome(&outcome.identity.operation_id)
        .await?
        .map(|(outcome, _)| outcome);
    Ok(ResponseData::SendOutcome { outcome })
}

async fn accepted_needs_repair(
    state: &AppState,
    identity: &SendIdentity,
    now: DateTime<Utc>,
) -> HandlerResult {
    state
        .store
        .postpone_send_recovery(&identity.operation_id, now + chrono::Duration::seconds(300))
        .await?;
    Ok(ResponseData::SendOutcome {
        outcome: state
            .store
            .get_send_outcome(&identity.operation_id)
            .await?
            .map(|(outcome, _)| outcome),
    })
}

pub(crate) async fn process_due(state: &AppState, now: DateTime<Utc>) -> Result<u32, HandlerError> {
    if state.is_restore_pending() {
        return Ok(0);
    }
    state
        .ensure_effects_allowed()
        .map_err(HandlerError::Message)?;
    let due = state.store.due_send_recovery(now).await?;
    let mut processed = 0;
    for operation_id in due {
        match process_operation(state, &operation_id, now).await {
            Ok(count) => processed += count,
            Err(error) => {
                tracing::warn!(operation_id, %error, "Send recovery requires attention; other approved sends will continue");
                crate::chimes::emit_daemon_event(state, DaemonEvent::OperationFailed {
                    operation_id: operation_id.clone(), operation: "send_recovery".into(), account_id: None,
                    error: "Send recovery needs attention; inspect the original operation. Do not create a duplicate send.".into(), retryable: false,
                });
                state
                    .store
                    .postpone_send_recovery(&operation_id, now + chrono::Duration::seconds(300))
                    .await?;
            }
        }
    }
    Ok(processed)
}

async fn process_operation(
    state: &AppState,
    operation_id: &str,
    now: DateTime<Utc>,
) -> Result<u32, HandlerError> {
    let Some((outcome, _)) = state.store.get_send_outcome(operation_id).await? else {
        return Ok(0);
    };
    let _guard = state.acquire_draft_operation(&outcome.draft_id).await;
    let Some((outcome, _)) = state.store.get_send_outcome(operation_id).await? else {
        return Ok(0);
    };
    if outcome.state != SendOutcomeState::Waiting {
        if matches!(
            outcome.state,
            SendOutcomeState::Unknown | SendOutcomeState::Accepted
        ) {
            reconcile_locked(state, outcome, now).await?;
        }
        return Ok(0);
    }
    let Some(approval) = state.store.get_send_approval(operation_id).await? else {
        return Ok(0);
    };
    let invalid = match validate_approval(state, &approval).await {
        Ok(()) => None,
        Err(error @ (HandlerError::Store(_) | HandlerError::Core(mxr_core::MxrError::Io(_)))) => {
            return Err(error);
        }
        Err(_) => Some("approval_invalidated"),
    };
    let invalid = if invalid.is_some() {
        invalid
    } else if state
        .send_provider_for_account(&approval.draft.account_id)
        .is_err()
    {
        Some("account_unavailable")
    } else if outcome.rfc2822_message_id.is_none() {
        Some("approval_incomplete")
    } else {
        None
    };
    if let Some(reason) = invalid {
        state
            .store
            .cancel_waiting_send(&outcome.identity, reason)
            .await?;
        notify_current(state, &outcome.identity).await;
        return Ok(0);
    }
    let sender = state.send_provider_for_account(&approval.draft.account_id)?;
    let message_id = outcome
        .rfc2822_message_id
        .as_deref()
        .ok_or("Approved send has no Message-ID")?;
    if !state
        .store
        .claim_waiting_send(&outcome.identity, &approval, now)
        .await?
    {
        return Ok(0);
    }
    match sender
        .send_classified(&approval.draft, &approval.from, message_id)
        .await
    {
        Ok(receipt) => {
            super::mutations::finish_accepted_send(
                state,
                &approval.draft,
                &approval.from,
                &outcome.identity,
                &receipt,
                false,
            )
            .await?;
        }
        Err(error) => {
            // Unknown and definitive refusal are expected outcomes, not queue
            // errors. Persistence errors still surface and retain the hold.
            match failure(state, &outcome.identity, error, true, now).await {
                Ok(_) | Err(HandlerError::SendOutcome { .. }) => (),
                Err(error) => return Err(error),
            }
        }
    }
    Ok(1)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn approved_send_cache_tightens_owned_legacy_directory_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("restored-resources");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        private_directory(&path).unwrap();

        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn approved_send_cache_rejects_symlinks_without_changing_target_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("outside-cache");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = temp.path().join("cache-link");
        symlink(&target, &link).unwrap();

        assert!(private_directory(&link).is_err());
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(
            target.metadata().unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
