use crate::{ClientError, IpcConnection};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::id::{AttachmentId, MessageId};
use mxr_protocol::{
    AttachmentChunk, AttachmentTransferMetadata, Request, ResponseData, MAX_ATTACHMENT_CHUNK_BYTES,
};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub length_bytes: u64,
    pub sha256: String,
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Transfer an attachment through IPC and expose the verified bytes locally.
/// Existing paths are preserved unless replacement was explicitly confirmed.
pub async fn download_attachment_to(
    connection: &mut IpcConnection,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
    destination: &Path,
    replace_existing: bool,
) -> Result<DownloadedAttachment, ClientError> {
    let metadata = match connection
        .request(Request::GetAttachmentTransferMetadata {
            message_id: message_id.clone(),
            attachment_id: attachment_id.clone(),
        })
        .await?
    {
        ResponseData::AttachmentTransferMetadata { metadata } => metadata,
        other => {
            return Err(invalid_transfer_response(format!(
                "unexpected metadata response: {other:?}"
            )))
        }
    };
    validate_metadata(&metadata, message_id, attachment_id)?;
    validate_destination(destination, replace_existing)?;

    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(ClientError::Io)?;
    let temp_path = parent.join(format!(".mxr-attachment-{}.part", uuid::Uuid::now_v7()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path).map_err(ClientError::Io)?;
    let temporary = TemporaryFile(temp_path.clone());
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;

    while offset < metadata.length_bytes {
        let remaining = metadata.length_bytes - offset;
        let length = remaining.min(u64::from(metadata.max_chunk_bytes)) as u32;
        let chunk = match connection
            .request(Request::ReadAttachmentChunk {
                message_id: message_id.clone(),
                attachment_id: attachment_id.clone(),
                offset,
                length,
            })
            .await?
        {
            ResponseData::AttachmentChunk { chunk } => chunk,
            other => {
                return Err(invalid_transfer_response(format!(
                    "unexpected chunk response: {other:?}"
                )))
            }
        };
        let bytes = validate_chunk(chunk, message_id, attachment_id, offset, length)?;
        file.write_all(&bytes).map_err(ClientError::Io)?;
        hasher.update(&bytes);
        offset = offset
            .checked_add(u64::from(length))
            .ok_or_else(|| invalid_transfer_response("attachment offset overflow"))?;
    }
    set_private_permissions(&file)?;
    file.sync_all().map_err(ClientError::Io)?;
    drop(file);

    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if offset != metadata.length_bytes || digest != metadata.sha256 {
        return Err(invalid_transfer_response(
            "attachment length or SHA-256 did not match transfer metadata",
        ));
    }
    validate_destination(destination, replace_existing)?;
    if replace_existing {
        std::fs::rename(&temp_path, destination).map_err(ClientError::Io)?;
    } else {
        std::fs::hard_link(&temp_path, destination).map_err(ClientError::Io)?;
    }
    drop(temporary);
    Ok(DownloadedAttachment {
        path: destination.to_path_buf(),
        filename: metadata.filename,
        length_bytes: metadata.length_bytes,
        sha256: digest,
    })
}

fn validate_metadata(
    metadata: &AttachmentTransferMetadata,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> Result<(), ClientError> {
    if &metadata.message_id != message_id || &metadata.attachment_id != attachment_id {
        return Err(invalid_transfer_response(
            "attachment metadata IDs do not match the request",
        ));
    }
    if metadata.max_chunk_bytes == 0 || metadata.max_chunk_bytes > MAX_ATTACHMENT_CHUNK_BYTES {
        return Err(invalid_transfer_response(
            "daemon advertised an invalid attachment chunk size",
        ));
    }
    if metadata.sha256.len() != 64
        || !metadata
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_transfer_response(
            "daemon returned an invalid SHA-256 digest",
        ));
    }
    Ok(())
}

fn validate_chunk(
    chunk: AttachmentChunk,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
    expected_offset: u64,
    expected_length: u32,
) -> Result<Vec<u8>, ClientError> {
    if &chunk.message_id != message_id
        || &chunk.attachment_id != attachment_id
        || chunk.offset != expected_offset
    {
        return Err(invalid_transfer_response(
            "attachment chunk IDs or offset do not match the request",
        ));
    }
    let bytes = STANDARD.decode(chunk.data_base64).map_err(|_| {
        invalid_transfer_response("daemon returned invalid base64 attachment bytes")
    })?;
    if bytes.len() != expected_length as usize {
        return Err(invalid_transfer_response(
            "daemon returned an attachment chunk with the wrong length",
        ));
    }
    Ok(bytes)
}

fn validate_destination(path: &Path, replace_existing: bool) -> Result<(), ClientError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(invalid_transfer_response(
                "attachment destination must be a regular file, not a symlink",
            ))
        }
        Ok(_) if !replace_existing => Err(invalid_transfer_response(
            "attachment destination already exists; choose another name or confirm replacement",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ClientError::Io(error)),
    }
}

fn invalid_transfer_response(message: impl Into<String>) -> ClientError {
    ClientError::Io(std::io::Error::other(message.into()))
}

#[cfg(unix)]
fn set_private_permissions(file: &std::fs::File) -> Result<(), ClientError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(ClientError::Io)
}

#[cfg(not(unix))]
fn set_private_permissions(_file: &std::fs::File) -> Result<(), ClientError> {
    Ok(())
}

#[cfg(test)]
mod tests;
