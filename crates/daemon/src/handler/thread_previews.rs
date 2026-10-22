//! `GetThreadPreviews`: a thread's headline facts with no bodies, for
//! hover cards and `mxr thread-preview`. One store read for the batch.

use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_protocol::{ResponseData, ThreadPreviewData, THREAD_PREVIEWS_MAX};
use std::collections::{HashMap, HashSet};

pub(super) async fn get_thread_previews(
    state: &AppState,
    thread_ids: &[ThreadId],
) -> HandlerResult {
    let mut seen = HashSet::new();
    let wanted: Vec<ThreadId> = thread_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    if wanted.is_empty() {
        return Err(HandlerError::InvalidRequest("name thread_ids".into()));
    }
    if wanted.len() > THREAD_PREVIEWS_MAX {
        return Err(HandlerError::InvalidRequest(format!(
            "at most {THREAD_PREVIEWS_MAX} threads at once"
        )));
    }
    let mut found: HashMap<ThreadId, ThreadPreviewData> = state
        .store
        .thread_previews(&wanted)
        .await?
        .into_iter()
        .map(|record| {
            (
                record.thread_id.clone(),
                ThreadPreviewData {
                    thread_id: record.thread_id,
                    subject: record.subject,
                    sender_name: record.sender_name.filter(|name| !name.trim().is_empty()),
                    sender_email: record.sender_email,
                    latest_at: record.latest_at.timestamp(),
                    message_count: record.message_count,
                    unread_count: record.unread_count,
                    snippet: record.snippet,
                    has_attachments: record.has_attachments,
                },
            )
        })
        .collect();
    Ok(ResponseData::ThreadPreviews {
        previews: wanted.iter().filter_map(|id| found.remove(id)).collect(),
    })
}
