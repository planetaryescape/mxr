use mxr_core::types::ExportFormat;
use mxr_export::{ExportAttachment, ExportMessage, ExportThread};
use mxr_protocol::*;
use mxr_reader::ReaderConfig;
use crate::state::AppState;

/// Build an ExportThread from a thread_id by fetching envelopes and bodies from the store.
async fn build_export_thread(
    state: &AppState,
    thread_id: &mxr_core::ThreadId,
) -> Result<ExportThread, String> {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread not found: {thread_id}"))?;

    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let mut messages = Vec::with_capacity(envelopes.len());
    for env in &envelopes {
        let body = state
            .store
            .get_body(&env.id)
            .await
            .map_err(|e| e.to_string())?;

        messages.push(ExportMessage {
            id: env.id.to_string(),
            from_name: env.from.name.clone(),
            from_email: env.from.email.clone(),
            to: env.to.iter().map(|a| a.email.clone()).collect(),
            date: env.date,
            subject: env.subject.clone(),
            body_text: body.as_ref().and_then(|b| b.text_plain.clone()),
            body_html: body.as_ref().and_then(|b| b.text_html.clone()),
            headers_raw: body.as_ref().and_then(|b| b.metadata.raw_headers.clone()),
            attachments: body
                .as_ref()
                .map(|b| {
                    b.attachments
                        .iter()
                        .map(|a| ExportAttachment {
                            filename: a.filename.clone(),
                            size_bytes: a.size_bytes,
                            local_path: a.local_path.as_ref().map(|p| p.display().to_string()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    Ok(ExportThread {
        thread_id: thread_id.to_string(),
        subject: thread.subject,
        messages,
    })
}

pub(super) async fn handle_export_thread(
    state: &AppState,
    thread_id: &mxr_core::ThreadId,
    format: &ExportFormat,
) -> Response {
    match build_export_thread(state, thread_id).await {
        Ok(export_thread) => {
            let reader_config = ReaderConfig::default();
            let content = mxr_export::export(&export_thread, format, &reader_config);
            Response::Ok {
                data: ResponseData::ExportResult { content },
            }
        }
        Err(e) => Response::Error { message: e },
    }
}

pub(super) async fn handle_export_search(
    state: &AppState,
    query: &str,
    format: &ExportFormat,
) -> Response {
    let search_results = match state
        .search
        .search(query, 100, 0, mxr_core::types::SortOrder::DateDesc)
        .await
    {
        Ok(results) => results,
        Err(e) => {
            return Response::Error {
                message: e.to_string(),
            }
        }
    };

    // Collect unique thread IDs from search results
    let thread_ids: Vec<mxr_core::ThreadId> = {
        let mut seen = std::collections::HashSet::new();
        search_results
            .results
            .iter()
            .filter_map(|r| {
                let tid =
                    mxr_core::ThreadId::from_uuid(uuid::Uuid::parse_str(&r.thread_id).ok()?);
                if seen.insert(tid.clone()) {
                    Some(tid)
                } else {
                    None
                }
            })
            .collect()
    };

    let reader_config = ReaderConfig::default();
    let mut all_content = String::new();

    for tid in &thread_ids {
        match build_export_thread(state, tid).await {
            Ok(export_thread) => {
                all_content.push_str(&mxr_export::export(
                    &export_thread,
                    format,
                    &reader_config,
                ));
                all_content.push('\n');
            }
            Err(e) => {
                tracing::warn!(thread_id = %tid, error = %e, "Skipping thread in bulk export");
            }
        }
    }

    Response::Ok {
        data: ResponseData::ExportResult {
            content: all_content,
        },
    }
}

pub(super) async fn materialize_attachment_file(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    attachment_id: &mxr_core::AttachmentId,
) -> Result<mxr_protocol::AttachmentFile, mxr_core::MxrError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|err| mxr_core::MxrError::Store(err.to_string()))?
        .ok_or_else(|| mxr_core::MxrError::NotFound(format!("message {message_id}")))?;

    let mut body = state.sync_engine.get_body(message_id).await?;
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| &attachment.id == attachment_id)
        .cloned()
        .ok_or_else(|| {
            mxr_core::MxrError::NotFound(format!("attachment {attachment_id}"))
        })?;

    if let Some(path) = attachment.local_path.as_ref().filter(|path| path.exists()) {
        return Ok(mxr_protocol::AttachmentFile {
            attachment_id: attachment.id,
            filename: attachment.filename,
            path: path.display().to_string(),
        });
    }

    let provider = state
        .get_provider(Some(&envelope.account_id))
        .map_err(mxr_core::MxrError::Provider)?;
    let bytes = provider
        .fetch_attachment(&envelope.provider_id, &attachment.provider_id)
        .await?;

    let target_dir = state.attachment_dir().join(message_id.as_str());
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(mxr_core::MxrError::Io)?;

    let filename = sanitized_attachment_filename(&attachment.filename, &attachment.id);
    let path = target_dir.join(filename);
    tokio::fs::write(&path, bytes)
        .await
        .map_err(mxr_core::MxrError::Io)?;

    for existing in &mut body.attachments {
        if existing.id == *attachment_id {
            existing.local_path = Some(path.clone());
        }
    }
    state
        .store
        .insert_body(&body)
        .await
        .map_err(|err| mxr_core::MxrError::Store(err.to_string()))?;

    Ok(mxr_protocol::AttachmentFile {
        attachment_id: attachment.id,
        filename: attachment.filename,
        path: path.display().to_string(),
    })
}

pub(super) fn sanitized_attachment_filename(
    filename: &str,
    attachment_id: &mxr_core::AttachmentId,
) -> String {
    let candidate = std::path::Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filename);
    let sanitized: String = candidate
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect();

    if sanitized.trim().is_empty() {
        format!("attachment-{}", attachment_id.as_str())
    } else {
        sanitized
    }
}

pub(super) fn open_local_file(path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .spawn()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("opening attachments is not supported on this platform")
    }
}
