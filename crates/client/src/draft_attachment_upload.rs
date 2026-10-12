use crate::{ClientError, IpcConnection};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::{Draft, DraftAttachment, DraftAttachmentId};
use mxr_protocol::{
    Request, Response, ResponseData, MAX_ATTACHMENT_CHUNK_BYTES, MAX_DRAFT_ATTACHMENT_BYTES,
};
use sha2::{Digest, Sha256};
use std::io::Read;

fn error(message: impl Into<String>) -> ClientError {
    ClientError::Io(std::io::Error::other(message.into()))
}

impl IpcConnection {
    async fn upload_request(
        &mut self,
        request: Request,
        workspace: Option<&mxr_protocol::ArchiveWorkspaceData>,
    ) -> Result<ResponseData, ClientError> {
        match self
            .request_response_unchecked_owned(request, &mut |_| {}, None, workspace)
            .await?
        {
            Response::Ok { data } => Ok(data),
            Response::Error {
                message,
                kind,
                code,
                retryable,
                details,
            } => Err(ClientError::Daemon {
                message,
                kind,
                code,
                retryable,
                details,
            }),
        }
    }

    /// Capability check precedes opening any client file. No host-path fallback.
    pub async fn require_draft_attachment_upload(&mut self) -> Result<(), ClientError> {
        match self.upload_request(Request::GetStatus, None).await? {
            ResponseData::Status { draft_attachment_upload_supported: true, .. } => Ok(()),
            _ => Err(error("Daemon does not support portable draft attachments; upgrade the daemon before uploading")),
        }
    }

    pub async fn upload_draft_attachment_bytes(
        &mut self,
        draft: &Draft,
        filename: String,
        mime_type: String,
        bytes: &[u8],
    ) -> Result<DraftAttachment, ClientError> {
        let workspace = self.capture_workspace().await?;
        self.upload_draft_attachment_bytes_owned(
            draft,
            filename,
            mime_type,
            bytes,
            Some(&workspace),
        )
        .await
    }

    pub async fn upload_draft_attachment_bytes_owned(
        &mut self,
        draft: &Draft,
        filename: String,
        mime_type: String,
        bytes: &[u8],
        workspace: Option<&mxr_protocol::ArchiveWorkspaceData>,
    ) -> Result<DraftAttachment, ClientError> {
        if let Some(owner) = workspace {
            self.verify_workspace(owner).await?;
        } else {
            return Err(error(
                "Attachment upload requires its original workspace owner",
            ));
        }
        self.require_draft_attachment_upload().await?;
        if bytes.len() as u64 > MAX_DRAFT_ATTACHMENT_BYTES {
            return Err(error(
                "Managed draft attachment exceeds the daemon's 25 MiB resource limit",
            ));
        }
        let metadata = DraftAttachment {
            id: DraftAttachmentId::new(),
            draft_id: draft.id.clone(),
            account_id: draft.account_id.clone(),
            filename,
            mime_type,
            length_bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        };
        match self
            .upload_request(
                Request::BeginDraftAttachment {
                    metadata: metadata.clone(),
                },
                workspace,
            )
            .await?
        {
            ResponseData::DraftAttachment { metadata: accepted } if accepted == metadata => (),
            _ => return Err(error("Unexpected upload declaration response")),
        }
        for (index, chunk) in bytes
            .chunks(MAX_ATTACHMENT_CHUNK_BYTES as usize)
            .enumerate()
        {
            match self
                .upload_request(
                    Request::WriteDraftAttachmentChunk {
                        draft_id: draft.id.clone(),
                        account_id: draft.account_id.clone(),
                        resource_id: metadata.id.clone(),
                        offset: index as u64 * u64::from(MAX_ATTACHMENT_CHUNK_BYTES),
                        data_base64: STANDARD.encode(chunk),
                    },
                    workspace,
                )
                .await?
            {
                ResponseData::Ack => (),
                _ => return Err(error("Unexpected upload chunk response")),
            }
        }
        match self
            .upload_request(
                Request::FinalizeDraftAttachment {
                    draft_id: draft.id.clone(),
                    account_id: draft.account_id.clone(),
                    resource_id: metadata.id.clone(),
                },
                workspace,
            )
            .await?
        {
            ResponseData::DraftAttachment { metadata: accepted } if accepted == metadata => {
                Ok(accepted)
            }
            _ => Err(error("Unexpected upload finalization response")),
        }
    }

    /// All Rust compose clients converge here, after their preview/dry-run decision.
    pub(crate) async fn prepare_draft_attachment_request(
        &mut self,
        mut request: Request,
        workspace: Option<&mxr_protocol::ArchiveWorkspaceData>,
    ) -> Result<Request, ClientError> {
        let preview_only = matches!(request, Request::CheckDraftSafety { .. });
        let sending = matches!(request, Request::SendDraft { .. });
        let draft = match &mut request {
            Request::SaveDraft { draft }
            | Request::UpdateDraft { draft }
            | Request::SaveDraftToServer { draft }
            | Request::SendDraft { draft, .. }
            | Request::CheckDraftSafety { draft, .. } => draft,
            _ => return Ok(request),
        };
        if draft.attachments.is_empty() {
            return Ok(request);
        }
        if preview_only
            && draft
                .attachments
                .iter()
                .any(|path| !path.to_string_lossy().starts_with("mxr-resource:"))
        {
            return Ok(request);
        }
        self.require_draft_attachment_upload().await?;
        let mut managed = draft.managed_attachments.take().unwrap_or_default();
        let mut files = Vec::new();
        let mut canonical = if draft.revision.unwrap_or(0) > 0 {
            match self
                .request_response_unchecked_owned(
                    Request::GetDraft {
                        draft_id: draft.id.clone(),
                    },
                    &mut |_| {},
                    None,
                    workspace,
                )
                .await?
            {
                Response::Ok {
                    data: ResponseData::Draft { draft },
                } => Some(draft),
                Response::Error {
                    kind: mxr_protocol::IpcErrorKind::NotFound,
                    ..
                } => None,
                Response::Error {
                    message,
                    kind,
                    code,
                    retryable,
                    details,
                } => {
                    return Err(ClientError::Daemon {
                        message,
                        kind,
                        code,
                        retryable,
                        details,
                    })
                }
                _ => return Err(error("Unexpected owner draft response")),
            }
        } else {
            None
        };
        for path in std::mem::take(&mut draft.attachments) {
            let rendered = path.to_string_lossy();
            if let Some(token) = rendered.strip_prefix("mxr-resource:") {
                let id: DraftAttachmentId = token
                    .split('/')
                    .next()
                    .unwrap_or_default()
                    .parse()
                    .map_err(|_| error("Invalid managed attachment reference"))?;
                if canonical.is_none() {
                    canonical = match self
                        .upload_request(
                            Request::GetDraft {
                                draft_id: draft.id.clone(),
                            },
                            workspace,
                        )
                        .await?
                    {
                        ResponseData::Draft { draft } => Some(draft),
                        _ => return Err(error("Unexpected owner draft response")),
                    };
                }
                let resource = canonical
                    .as_ref()
                    .and_then(|d| d.managed_attachments.as_ref())
                    .and_then(|set| set.iter().find(|m| m.id == id))
                    .ok_or_else(|| {
                        error("Managed attachment reference is not attached to this draft")
                    })?;
                if !managed.iter().any(|m| m.id == id) {
                    managed.push(resource.clone());
                }
            } else {
                if canonical
                    .as_ref()
                    .is_some_and(|stored| stored.attachments.contains(&path))
                {
                    draft.attachments.push(path);
                    continue;
                }
                if sending {
                    return Err(error("Save portable draft attachments and review the returned revision before sending"));
                }
                let metadata = std::fs::metadata(&path).map_err(ClientError::Io)?;
                if !metadata.is_file() || metadata.len() > MAX_DRAFT_ATTACHMENT_BYTES {
                    return Err(error("Attachment must be a regular file within the daemon's 25 MiB managed-resource limit"));
                }
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| error("Attachment filename must be UTF-8"))?
                    .to_string();
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(ClientError::Io)?
                    .take(MAX_DRAFT_ATTACHMENT_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(ClientError::Io)?;
                if bytes.len() as u64 > MAX_DRAFT_ATTACHMENT_BYTES {
                    return Err(error(
                        "Attachment grew beyond the daemon's 25 MiB managed-resource limit",
                    ));
                }
                files.push((filename, bytes));
            }
        }
        draft.managed_attachments = Some(managed);
        if files.is_empty() {
            return Ok(request);
        }
        let needs_owner = canonical.is_none();
        if needs_owner {
            let mut owner = draft.clone();
            owner.revision = None;
            *draft = match self
                .upload_request(Request::SaveDraft { draft: owner }, workspace)
                .await?
            {
                ResponseData::Draft { draft } => draft,
                _ => return Err(error("Unexpected owner draft save response")),
            };
        }
        for (filename, bytes) in files {
            let resource = self
                .upload_draft_attachment_bytes_owned(
                    draft,
                    filename,
                    "application/octet-stream".into(),
                    &bytes,
                    workspace,
                )
                .await?;
            draft
                .managed_attachments
                .get_or_insert_default()
                .push(resource);
        }
        // Attach resources with the reviewed owner's revision, before any send/provider effect.
        *draft = match self
            .upload_request(
                Request::UpdateDraft {
                    draft: draft.clone(),
                },
                workspace,
            )
            .await?
        {
            ResponseData::Draft { draft } => draft,
            _ => return Err(error("Unexpected attachment-set save response")),
        };
        match &mut request {
            Request::SaveDraft { draft } => {
                return Ok(Request::GetDraft {
                    draft_id: draft.id.clone(),
                })
            }
            Request::UpdateDraft { draft } => {
                return Ok(Request::GetDraft {
                    draft_id: draft.id.clone(),
                })
            }
            _ => (),
        }
        Ok(request)
    }
}

#[cfg(test)]
mod tests;
