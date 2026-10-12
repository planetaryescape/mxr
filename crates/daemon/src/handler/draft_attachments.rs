//! Draft guard precedes file I/O and store writes. No upload lock or open SQL transaction.
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::{AccountId, Draft, DraftAttachment, DraftAttachmentId, DraftId, DraftStatus};
use mxr_protocol::{ResponseData, MAX_ATTACHMENT_CHUNK_BYTES, MAX_DRAFT_ATTACHMENT_BYTES};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

fn validate(metadata: &DraftAttachment) -> Result<(), HandlerError> {
    if metadata.length_bytes > MAX_DRAFT_ATTACHMENT_BYTES {
        return Err("Managed draft attachment exceeds the daemon's 25 MiB resource limit (provider limits may differ)".into());
    }
    if metadata.filename.is_empty()
        || metadata.filename.len() > 200
        || metadata.filename == "."
        || metadata.filename == ".."
        || metadata
            .filename
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || metadata.mime_type.len() > 200
        || metadata.mime_type.chars().any(char::is_control)
        || metadata.sha256.len() != 64
        || !metadata
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Invalid draft attachment declaration: require filename, bounded MIME type and lowercase SHA-256".into());
    }
    Ok(())
}

pub(super) fn preview(metadata: &DraftAttachment) -> HandlerResult {
    validate(metadata)?;
    Ok(ResponseData::DraftAttachment {
        metadata: metadata.clone(),
    })
}

fn paths(state: &AppState, metadata: &DraftAttachment) -> (PathBuf, PathBuf) {
    let dir = state
        .store
        .draft_attachment_dir()
        .join(metadata.id.as_str());
    (
        dir.join("upload.part"),
        dir.join("final").join(&metadata.filename),
    )
}

fn private_dir(path: &std::path::Path) -> Result<(), HandlerError> {
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn editable_owner(
    state: &AppState,
    draft_id: &DraftId,
    account_id: &AccountId,
) -> Result<(), HandlerError> {
    let draft = state
        .store
        .get_draft(draft_id)
        .await?
        .ok_or("Save the owner draft before uploading")?;
    if &draft.account_id != account_id {
        return Err("Attachment owner account does not match draft".into());
    }
    if state.store.get_draft_status(draft_id).await? != Some(DraftStatus::Draft) {
        return Err("Attachments cannot change while a send is uncertain or completed".into());
    }
    Ok(())
}

pub(super) async fn begin(state: &AppState, metadata: &DraftAttachment) -> HandlerResult {
    validate(metadata)?;
    cleanup(state).await?;
    let _guard = state.acquire_draft_operation(&metadata.draft_id).await;
    editable_owner(state, &metadata.draft_id, &metadata.account_id).await?;
    if let Some((stored, _)) = state.store.get_draft_attachment(&metadata.id).await? {
        if &stored != metadata {
            return Err("Resource ID already has a different declaration or owner".into());
        }
    } else {
        state.store.insert_draft_attachment(metadata).await?;
    }
    let (partial, final_path) = paths(state, metadata);
    private_dir(state.store.draft_attachment_dir())?;
    private_dir(partial.parent().ok_or("Missing resource directory")?)?;
    private_dir(final_path.parent().ok_or("Missing publication directory")?)?;
    if !partial.exists() && !final_path.exists() {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(partial).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    Ok(ResponseData::DraftAttachment {
        metadata: metadata.clone(),
    })
}

async fn owned(
    state: &AppState,
    draft_id: &DraftId,
    account_id: &AccountId,
    id: &DraftAttachmentId,
) -> Result<(DraftAttachment, bool), HandlerError> {
    let (metadata, ready) = state
        .store
        .get_draft_attachment(id)
        .await?
        .ok_or("Draft attachment resource not found")?;
    if &metadata.draft_id != draft_id || &metadata.account_id != account_id {
        return Err("Draft attachment resource belongs to another draft/account".into());
    }
    Ok((metadata, ready))
}

pub(super) async fn write_chunk(
    state: &AppState,
    draft_id: &DraftId,
    account_id: &AccountId,
    id: &DraftAttachmentId,
    offset: u64,
    encoded: &str,
) -> HandlerResult {
    // Bound the encoded allocation before decoding untrusted client bytes.
    if encoded.len() > (MAX_ATTACHMENT_CHUNK_BYTES as usize).div_ceil(3) * 4 {
        return Err("Draft attachment chunk exceeds 256 KiB".into());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid attachment chunk base64")?;
    if bytes.is_empty() || bytes.len() > MAX_ATTACHMENT_CHUNK_BYTES as usize {
        return Err("Chunk must contain 1..256 KiB".into());
    }
    let _guard = state.acquire_draft_operation(draft_id).await;
    editable_owner(state, draft_id, account_id).await?;
    let (metadata, ready) = owned(state, draft_id, account_id, id).await?;
    let end = offset
        .checked_add(bytes.len() as u64)
        .ok_or("Chunk offset overflow")?;
    if end > metadata.length_bytes {
        return Err("Chunk range exceeds declared attachment length".into());
    }
    let (partial, final_path) = paths(state, &metadata);
    let published = ready || final_path.exists();
    let path = if published { final_path } else { partial };
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(!published)
        .open(path)
        .map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if offset < length {
        if end > length {
            return Err("Chunk overlaps committed bytes; retry the original chunk".into());
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut existing = vec![0; bytes.len()];
        file.read_exact(&mut existing).map_err(|e| e.to_string())?;
        if existing != bytes {
            return Err("Retried chunk conflicts with committed bytes".into());
        }
    } else if offset == length && !published {
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    } else {
        return Err("Chunk must follow committed bytes; finalized resources are immutable".into());
    }
    Ok(ResponseData::Ack)
}

fn verify(path: &std::path::Path, metadata: &DraftAttachment) -> Result<(), HandlerError> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Managed attachment bytes missing: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() != metadata.length_bytes {
        return Err("Attachment length does not match declaration".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
        != metadata.sha256
    {
        return Err("Attachment SHA-256 does not match declaration".into());
    }
    Ok(())
}

pub(super) async fn finalize(
    state: &AppState,
    draft_id: &DraftId,
    account_id: &AccountId,
    id: &DraftAttachmentId,
) -> HandlerResult {
    let _guard = state.acquire_draft_operation(draft_id).await;
    editable_owner(state, draft_id, account_id).await?;
    let (metadata, _) = owned(state, draft_id, account_id, id).await?;
    let (partial, final_path) = paths(state, &metadata);
    if !final_path.exists() {
        verify(&partial, &metadata)?;
        let file = std::fs::File::open(&partial).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o400))
                .map_err(|e| e.to_string())?;
        }
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&partial, &final_path).map_err(|e| e.to_string())?;
        // File publication precedes ready metadata; retries recognize and reverify published bytes.
        std::fs::File::open(final_path.parent().ok_or("Missing resource directory")?)
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
    }
    verify(&final_path, &metadata)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&final_path, std::fs::Permissions::from_mode(0o400))
            .map_err(|e| e.to_string())?;
    }
    state.store.finalize_draft_attachment(id).await?;
    Ok(ResponseData::DraftAttachment { metadata })
}

pub(super) async fn validate_set(state: &AppState, draft: &Draft) -> Result<(), HandlerError> {
    let mut ids = std::collections::HashSet::new();
    for metadata in draft.managed_attachments.as_deref().unwrap_or_default() {
        if !ids.insert(metadata.id.clone()) {
            return Err("Duplicate managed attachment resource ID".into());
        }
        let (stored, ready) = owned(state, &draft.id, &draft.account_id, &metadata.id).await?;
        if !ready || stored != *metadata {
            return Err(
                "Managed attachment must be finalized and match its persisted declaration".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn require_visible_set(incoming: &Draft, stored: &Draft) -> Result<(), HandlerError> {
    if incoming.managed_attachments.is_none()
        && stored
            .managed_attachments
            .as_ref()
            .is_some_and(|set| !set.is_empty())
    {
        return Err("This draft has managed attachments invisible to this client; upgrade the client before editing or sending".into());
    }
    Ok(())
}

/// Only the provider clone receives daemon-local paths. Reviewed and persisted drafts stay portable.
pub(super) async fn resolve(state: &AppState, draft: &Draft) -> Result<Draft, HandlerError> {
    validate_set(state, draft).await?;
    let mut resolved = draft.clone();
    for metadata in draft.managed_attachments.as_deref().unwrap_or_default() {
        let (_, path) = paths(state, metadata);
        verify(&path, metadata)?;
        resolved.attachments.push(path);
    }
    Ok(resolved)
}

async fn cleanup(state: &AppState) -> Result<(), HandlerError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).timestamp();
    for metadata in state.store.stale_draft_attachments(cutoff).await? {
        let _guard = state.acquire_draft_operation(&metadata.draft_id).await;
        // Existing owners retain even abandoned partials. Sending/uncertain/accepted drafts stay owners.
        if state.store.get_draft(&metadata.draft_id).await?.is_some() {
            continue;
        }
        if state
            .store
            .get_draft_send_outcome(&metadata.draft_id)
            .await?
            .is_some_and(|outcome| {
                outcome.state == mxr_core::SendOutcomeState::Unknown
                    || (outcome.state == mxr_core::SendOutcomeState::Accepted
                        && outcome.local_message_id.is_none())
            })
        {
            continue;
        }
        let (partial, _) = paths(state, &metadata);
        match std::fs::remove_dir_all(partial.parent().ok_or("Missing resource directory")?) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string().into()),
        }
        state.store.delete_draft_attachment(&metadata.id).await?;
    }
    Ok(())
}

pub(super) async fn require_review_flag(
    state: &AppState,
    draft_id: &DraftId,
) -> Result<(), HandlerError> {
    if state
        .store
        .get_draft(draft_id)
        .await?
        .is_some_and(|draft| draft.managed_attachments.is_some_and(|set| !set.is_empty()))
    {
        return Err("Managed draft attachments require a capable client's review; upgrade the client before sending or scheduling".into());
    }
    Ok(())
}
