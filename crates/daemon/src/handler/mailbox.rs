use super::{
    find_label_by_name, materialize_attachment_file, open_local_file,
    populate_envelope_label_provider_ids, HandlerResult,
};
use crate::state::AppState;
use mxr_core::id::{AccountId, AttachmentId, LabelId, MessageId, ThreadId};
use mxr_protocol::ResponseData;
use std::sync::Arc;

fn resolve_account_id(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<AccountId, String> {
    account_id
        .cloned()
        .or_else(|| state.default_account_id_opt())
        .ok_or_else(|| "No sync-capable accounts configured".to_string())
}

pub(super) async fn list_envelopes(
    state: &Arc<AppState>,
    label_id: Option<&LabelId>,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> HandlerResult {
    let result = if let Some(label_id) = label_id {
        tracing::debug!(label_id = %label_id, limit, offset, "listing envelopes by label");
        state
            .store
            .list_envelopes_by_label(label_id, limit, offset)
            .await
    } else {
        let Some(default_account_id) = state.default_account_id_opt() else {
            return Ok(ResponseData::Envelopes {
                envelopes: Vec::new(),
            });
        };
        state
            .store
            .list_envelopes_by_account(account_id.unwrap_or(&default_account_id), limit, offset)
            .await
    };

    let mut envelopes = result.map_err(|e| e.to_string())?;
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }

    tracing::debug!(
        count = envelopes.len(),
        by_label = label_id.is_some(),
        "listed envelopes"
    );
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn list_envelopes_by_ids(
    state: &Arc<AppState>,
    message_ids: &[MessageId],
) -> HandlerResult {
    let mut envelopes = state
        .store
        .list_envelopes_by_ids(message_ids)
        .await
        .map_err(|e| e.to_string())?;
    for envelope in &mut envelopes {
        if let Ok(labels) = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
        {
            let _ = populate_envelope_label_provider_ids(state, envelope, &labels).await;
        }
    }
    Ok(ResponseData::Envelopes { envelopes })
}

pub(super) async fn get_envelope(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    match state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(mut envelope) => {
            if let Ok(labels) = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
            {
                let _ = populate_envelope_label_provider_ids(state, &mut envelope, &labels).await;
            }
            Ok(ResponseData::Envelope { envelope })
        }
        None => Err(format!("Envelope not found: {message_id}")),
    }
}

pub(super) async fn get_body(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    let body = state
        .sync_engine
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Body { body })
}

pub(super) async fn download_attachment(
    state: &Arc<AppState>,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> HandlerResult {
    let file = materialize_attachment_file(state, message_id, attachment_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::AttachmentFile { file })
}

pub(super) async fn open_attachment(
    state: &Arc<AppState>,
    message_id: &MessageId,
    attachment_id: &AttachmentId,
) -> HandlerResult {
    let file = materialize_attachment_file(state, message_id, attachment_id)
        .await
        .map_err(|e| e.to_string())?;
    open_local_file(&file.path).map_err(|e| e.to_string())?;
    Ok(ResponseData::AttachmentFile { file })
}

pub(super) async fn list_bodies(state: &Arc<AppState>, message_ids: &[MessageId]) -> HandlerResult {
    tracing::debug!(count = message_ids.len(), "ListBodies: fetching bodies");
    let mut bodies = Vec::with_capacity(message_ids.len());
    for id in message_ids {
        if let Ok(Some(full)) = state.store.get_body(id).await {
            let (plain, html) = if full.text_plain.is_some() {
                (full.text_plain, None)
            } else {
                (None, full.text_html)
            };
            bodies.push(mxr_core::types::MessageBody {
                message_id: full.message_id,
                text_plain: plain,
                text_html: html,
                attachments: vec![],
                fetched_at: full.fetched_at,
                metadata: full.metadata,
            });
        }
    }
    Ok(ResponseData::Bodies { bodies })
}

pub(super) async fn get_thread(state: &Arc<AppState>, thread_id: &ThreadId) -> HandlerResult {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Thread not found".to_string())?;
    let mut messages = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .unwrap_or_default();
    if let Ok(labels) = state.store.list_labels_by_account(&thread.account_id).await {
        for message in &mut messages {
            let _ = populate_envelope_label_provider_ids(state, message, &labels).await;
        }
    }
    Ok(ResponseData::Thread { thread, messages })
}

pub(super) async fn list_labels(
    state: &Arc<AppState>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let Some(account_id) = account_id
        .cloned()
        .or_else(|| state.default_account_id_opt())
    else {
        return Ok(ResponseData::Labels { labels: Vec::new() });
    };
    let labels = state
        .store
        .list_labels_by_account(&account_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Labels { labels })
}

pub(super) async fn create_label(
    state: &Arc<AppState>,
    name: &str,
    color: Option<&str>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let provider = state.get_provider(Some(&account_id));
    let mut label = provider
        .create_label(name, color)
        .await
        .map_err(|e| e.to_string())?;
    if label.account_id != account_id {
        label.account_id = account_id;
    }
    state
        .store
        .upsert_label(&label)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Label { label })
}

pub(super) async fn delete_label(
    state: &Arc<AppState>,
    name: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let label = find_label_by_name(state, &account_id, name).await?;
    let provider = state.get_provider(Some(&account_id));
    provider
        .delete_label(&label.provider_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .store
        .delete_label(&label.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn rename_label(
    state: &Arc<AppState>,
    old: &str,
    new: &str,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let account_id = resolve_account_id(state, account_id)?;
    let existing = find_label_by_name(state, &account_id, old).await?;
    let provider = state.get_provider(Some(&account_id));
    let mut label = provider
        .rename_label(&existing.provider_id, new)
        .await
        .map_err(|e| e.to_string())?;
    if label.account_id != account_id {
        label.account_id = account_id.clone();
    }
    state
        .store
        .replace_label(&existing.id, &label)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Label { label })
}
