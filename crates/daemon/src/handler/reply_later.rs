use super::HandlerResult;
use crate::state::AppState;
use chrono::{DateTime, Utc};
use mxr_core::id::MessageId;
use mxr_protocol::ResponseData;

pub(super) async fn set_reply_later(
    state: &AppState,
    message_id: &MessageId,
    flag: bool,
) -> HandlerResult {
    set_reply_later_at(state, message_id, flag, Utc::now()).await?;
    Ok(ResponseData::Ack)
}

pub(crate) async fn set_reply_later_at(
    state: &AppState,
    message_id: &MessageId,
    flag: bool,
    now: DateTime<Utc>,
) -> Result<(), String> {
    if flag {
        state
            .store
            .set_reply_later(message_id, now)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        state
            .store
            .clear_reply_later(message_id, now)
            .await
            .map_err(|e| e.to_string())?;
    }
    refresh_reply_later_search_marker(state, message_id, flag).await
}

pub(crate) async fn refresh_reply_later_search_marker(
    state: &AppState,
    message_id: &MessageId,
    reply_later: bool,
) -> Result<(), String> {
    let Some(envelope) = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    let body = state
        .store
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope,
                body,
                reply_later,
            }],
            removed_message_ids: Vec::new(),
        })
        .await
        .map_err(|e| e.to_string())
}

pub(super) async fn list_reply_queue(state: &AppState) -> HandlerResult {
    let ids = state.store.list_reply_later(Utc::now()).await?;
    let messages = state.store.list_envelopes_by_ids(&ids).await?;
    let id_order: std::collections::HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let mut sorted = messages;
    sorted.sort_by_key(|env| id_order.get(&env.id).copied().unwrap_or(usize::MAX));
    Ok(ResponseData::ReplyQueue { messages: sorted })
}

pub(super) async fn set_auto_reminder(
    state: &AppState,
    sent_message_id: &MessageId,
    remind_at: DateTime<Utc>,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(sent_message_id)
        .await?
        .ok_or_else(|| format!("unknown message id `{}`", sent_message_id.as_str()))?;
    let now = Utc::now();
    state
        .store
        .set_auto_reminder(sent_message_id, &envelope.account_id, remind_at, now)
        .await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn cancel_auto_reminder(
    state: &AppState,
    sent_message_id: &MessageId,
) -> HandlerResult {
    state
        .store
        .cancel_auto_reminder(sent_message_id, Utc::now())
        .await?;
    Ok(ResponseData::Ack)
}
