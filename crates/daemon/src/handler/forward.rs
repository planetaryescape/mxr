use super::HandlerError;
use crate::state::AppState;
use mxr_core::{
    DraftAttachment, DraftAttachmentId, DraftContent, DraftId, Envelope, ForwardSource, MessageBody,
};
use sha2::{Digest, Sha256};

pub(super) async fn prepare_source(
    state: &AppState,
    envelope: &Envelope,
    body: &MessageBody,
) -> Result<ForwardSource, HandlerError> {
    if body.text_plain.is_none() && body.text_html.is_none() && body.attachments.is_empty() {
        return Err("Cannot forward: original message content is missing".into());
    }
    let headers = format!(
        "{}\nFrom: {}\nDate: {}\nSubject: {}\nTo: {}{}\n\n",
        mxr_compose::body_regions::FORWARD_HEADER,
        mxr_compose::draft_codec::format_addresses(std::slice::from_ref(&envelope.from)),
        envelope.date.to_rfc2822(),
        envelope.subject,
        mxr_compose::draft_codec::format_addresses(&envelope.to),
        if envelope.cc.is_empty() {
            String::new()
        } else {
            format!(
                "\nCc: {}",
                mxr_compose::draft_codec::format_addresses(&envelope.cc)
            )
        },
    );
    let original_text = body
        .text_plain
        .as_ref()
        .filter(|plain| {
            !plain.trim().is_empty()
                || body
                    .text_html
                    .as_ref()
                    .is_none_or(|html| html.trim().is_empty())
        })
        .cloned()
        .unwrap_or_else(|| {
            mxr_outbound::html::generate_text_alternative(
                body.text_html.as_deref().unwrap_or_default(),
            )
        });
    let original_html = body
        .text_html
        .as_ref()
        .filter(|html| !html.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| format!("<pre>{}</pre>", htmlescape::encode_minimal(&original_text)));
    mxr_outbound::html::validate_html(&original_html)
        .map_err(|error| format!("Cannot forward original HTML: {error}"))?;
    {
        let document = scraper::Html::parse_document(&original_html);
        for node in document.tree.nodes() {
            let Some(element) = node.value().as_element() else {
                continue;
            };
            for (_, value) in element.attrs() {
                validate_cid_reference(body, value)?;
            }
        }
    }
    // CSS background images may reference related parts without an img/src.
    static CSS_URL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)url\(\s*['"]?(cid:[^'"\s)]+)"#).expect("static CSS CID URL regex")
    });
    for capture in CSS_URL.captures_iter(&original_html) {
        validate_cid_reference(body, &capture[1])?;
    }
    let html_headers = format!("<pre>{}</pre>\n", htmlescape::encode_minimal(&headers));
    let content = DraftContent::html(
        mxr_compose::forward::prepend_html(&original_html, &html_headers),
        Some(format!("{headers}{original_text}")),
    );
    let draft_id = DraftId::new();
    let mut attachments = Vec::new();
    for attachment in &body.attachments {
        if attachment.size_bytes > mxr_protocol::MAX_DRAFT_ATTACHMENT_BYTES {
            return Err(format!(
                "Cannot forward attachment '{}': exceeds the daemon's 25 MiB resource limit",
                attachment.filename
            )
            .into());
        }
        let prepared = async {
            let cached = match &attachment.local_path {
                Some(path) => match tokio::fs::read(path).await {
                    Ok(bytes) => Some(bytes),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(HandlerError::from(format!(
                            "Cannot read cached attachment: {error}"
                        )))
                    }
                },
                None => None,
            };
            let bytes = match cached {
                Some(bytes) => bytes,
                None => {
                    state
                        .get_provider(Some(&envelope.account_id))?
                        .fetch_attachment(&envelope.provider_id, &attachment.provider_id)
                        .await?
                }
            };
            if attachment.size_bytes != 0 && bytes.len() as u64 != attachment.size_bytes {
                return Err(HandlerError::from(format!(
                    "Original attachment length mismatch: expected {} bytes, retrieved {}",
                    attachment.size_bytes,
                    bytes.len()
                )));
            }
            let metadata = DraftAttachment {
                id: DraftAttachmentId::new(),
                draft_id: draft_id.clone(),
                account_id: envelope.account_id.clone(),
                filename: if attachment.filename.is_empty() {
                    "attachment".into()
                } else {
                    attachment.filename.clone()
                },
                mime_type: attachment.mime_type.clone(),
                content_id: attachment
                    .content_id
                    .as_deref()
                    .map(|cid| {
                        cid.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .to_string()
                    })
                    .or_else(|| {
                        attachment
                            .content_location
                            .is_some()
                            .then(|| format!("mxr-{}@forward", attachment.id))
                    }),
                content_location: attachment.content_location.clone(),
                length_bytes: bytes.len() as u64,
                sha256: Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect(),
            };
            super::draft_attachments::import_forward(state, &metadata, &bytes).await?;
            Ok::<_, HandlerError>(metadata)
        }
        .await
        .map_err(|e| {
            format!(
                "Cannot forward attachment '{}' ({}): {e}",
                attachment.filename, attachment.id
            )
        })?;
        attachments.push(prepared);
    }
    Ok(ForwardSource {
        draft_id,
        account_id: envelope.account_id.clone(),
        message_id: envelope.id.clone(),
        content,
        attachments,
    })
}

fn validate_cid_reference(body: &MessageBody, value: &str) -> Result<(), HandlerError> {
    let Some((_, cid)) = value
        .split_once(':')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("cid"))
    else {
        return Ok(());
    };
    let cid = percent_encoding::percent_decode_str(cid)
        .decode_utf8()
        .map_err(|_| "Cannot forward: invalid inline attachment cid URL")?;
    if !body.attachments.iter().any(|part| {
        part.content_id.as_deref().is_some_and(|id| {
            id.trim().trim_start_matches('<').trim_end_matches('>') == cid.as_ref()
        })
    }) {
        return Err(format!(
            "Cannot forward: original HTML references missing inline attachment cid:{cid}"
        )
        .into());
    }
    Ok(())
}

/// Keep the immediate local Sent copy complete too. Its cache must outlive the
/// draft resources, which can be collected after the accepted draft is deleted.
pub(super) async fn sent_attachments(
    state: &AppState,
    draft: &mxr_core::Draft,
    message_id: &mxr_core::MessageId,
) -> Result<Vec<mxr_core::AttachmentMeta>, String> {
    let regular: Vec<_> = draft
        .managed_attachments
        .iter()
        .flatten()
        .filter(|part| part.content_id.is_none())
        .collect();
    let start = draft
        .attachments
        .len()
        .checked_sub(regular.len())
        .ok_or("Forward attachments were not resolved for Sent")?;
    let files = mxr_outbound::attachments::resolve_attachment_paths(&draft.attachments)
        .map_err(|e| e.to_string())?;
    let mut parts = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let metadata = index
            .checked_sub(start)
            .and_then(|index| regular.get(index).copied());
        parts.push((file, metadata, None));
    }
    for asset in &draft.inline_assets {
        let file =
            mxr_outbound::attachments::resolve_attachment_paths(std::slice::from_ref(&asset.path))
                .map_err(|e| e.to_string())?
                .remove(0);
        let metadata = draft
            .managed_attachments
            .iter()
            .flatten()
            .find(|part| part.content_id.as_deref() == Some(asset.cid.as_str()));
        parts.push((file, metadata, Some(asset.cid.clone())));
    }
    let directory = state.attachment_dir().join(message_id.as_str());
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|e| e.to_string())?;
    }
    let mut attachments = Vec::new();
    for (file, metadata, cid) in parts {
        let filename = metadata.map_or(file.filename, |part| part.filename.clone());
        let mime_type = metadata.map_or(file.mime_type, |part| part.mime_type.clone());
        let id = mxr_core::AttachmentId::new();
        let path = directory.join(super::sanitized_attachment_filename(&filename, &id));
        let size_bytes = tokio::fs::copy(&file.path, &path)
            .await
            .map_err(|e| e.to_string())?;
        super::set_private_file_permissions(&path)
            .await
            .map_err(|e| e.to_string())?;
        attachments.push(mxr_core::AttachmentMeta {
            id,
            message_id: message_id.clone(),
            filename,
            mime_type,
            disposition: if cid.is_some() {
                mxr_core::AttachmentDisposition::Inline
            } else {
                mxr_core::AttachmentDisposition::Attachment
            },
            content_id: cid,
            content_location: metadata.and_then(|part| part.content_location.clone()),
            size_bytes,
            local_path: Some(path),
            provider_id: String::new(),
        });
    }
    Ok(attachments)
}
