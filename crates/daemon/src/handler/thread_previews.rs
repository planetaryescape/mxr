//! `GetThreadPreviews`: a thread's headline facts with no bodies, for
//! hover cards and `mxr thread-preview`. One store read for the batch.

use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_protocol::{ResponseData, ThreadPreviewData, THREAD_PREVIEWS_MAX};
use std::collections::HashSet;

pub(super) async fn get_thread_previews(
    state: &AppState,
    thread_ids: &[ThreadId],
    thread_accounts: &[mxr_protocol::ThreadAccountRef],
) -> HandlerResult {
    // A pair previews that account's copy of the thread alone. A bare id
    // previews each account that holds it, as its own preview.
    let wanted = super::modes::request_thread_targets(thread_ids, thread_accounts);
    if wanted.is_empty() {
        return Err(HandlerError::InvalidRequest("name thread_ids".into()));
    }
    if wanted.len() > THREAD_PREVIEWS_MAX {
        return Err(HandlerError::InvalidRequest(format!(
            "at most {THREAD_PREVIEWS_MAX} threads at once"
        )));
    }
    let found = state.store.thread_previews(&wanted).await?;
    let mut previews = Vec::new();
    let mut taken: HashSet<(ThreadId, AccountId)> = HashSet::new();
    for (thread_id, account_id) in &wanted {
        for record in found.iter().filter(|record| {
            record.thread_id == *thread_id
                && account_id
                    .as_ref()
                    .is_none_or(|account_id| *account_id == record.account_id)
        }) {
            if !taken.insert((record.thread_id.clone(), record.account_id.clone())) {
                continue;
            }
            previews.push(ThreadPreviewData {
                thread_id: record.thread_id.clone(),
                subject: record.subject.clone(),
                sender_name: record
                    .sender_name
                    .clone()
                    .filter(|name| !name.trim().is_empty()),
                sender_email: record.sender_email.clone(),
                latest_at: record.latest_at.timestamp(),
                message_count: record.message_count,
                unread_count: record.unread_count,
                snippet: record.snippet.clone(),
                has_attachments: record.has_attachments,
                account_id: Some(record.account_id.clone()),
            });
        }
    }
    Ok(ResponseData::ThreadPreviews { previews })
}
