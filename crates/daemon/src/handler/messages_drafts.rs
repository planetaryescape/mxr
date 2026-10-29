//! Unsent replies on Messages rows: a row whose conversation has a reply you
//! started and left shows it as "Draft: ..." in every client, and opening
//! the row resumes it. The draft counted is the one `ensure_reply_draft`
//! resumes for that conversation: a reply in `draft` status that no send,
//! schedule or recovery holds.

use super::messages_view::preview_text;
use super::HandlerError;
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Draft, DraftIntent, DraftStatus};
use mxr_protocol::{MessagesRowData, MessagesRowDraftData};
use std::collections::HashMap;

/// Each conversation's unsent reply, by account and thread.
pub(super) type ConversationDrafts = HashMap<(AccountId, ThreadId), MessagesRowDraftData>;

/// The unsent replies with text in `accounts`. A reply opened and left empty
/// is not a draft to the user, so it is left out.
pub(super) async fn conversation_drafts(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<ConversationDrafts, HandlerError> {
    let mut found = ConversationDrafts::new();
    for account in accounts {
        let mut drafts = state.store.list_drafts(account).await?;
        // `ensure_reply_draft`'s order: when legacy duplicates answer the same
        // conversation, the oldest is the one opening it resumes.
        drafts.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.as_str().cmp(&right.id.as_str()))
        });
        let candidates: Vec<(Draft, String)> = drafts
            .into_iter()
            .filter(|draft| matches!(draft.intent, DraftIntent::Reply | DraftIntent::ReplyAll))
            .filter_map(|draft| {
                let text = preview_text(draft.content.analysis_text());
                (!text.is_empty() && draft.reply_headers.is_some()).then_some((draft, text))
            })
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let scheduled = state
            .store
            .list_pending_scheduled_sends(Some(account))
            .await?;
        for (draft, text) in candidates {
            if scheduled.iter().any(|send| send.draft_id == draft.id)
                || state.store.get_draft_status(&draft.id).await? != Some(DraftStatus::Draft)
                || state.store.is_restored_draft(&draft.id).await?
                || state
                    .store
                    .get_draft_send_outcome(&draft.id)
                    .await?
                    .is_some()
            {
                continue;
            }
            let Some(headers) = draft.reply_headers.as_ref() else {
                continue;
            };
            let sources = state
                .store
                .list_envelopes_by_message_id_header(account, &headers.in_reply_to)
                .await?;
            // An ambiguous Message-ID may name several threads; mark each,
            // as resuming from any of them finds this draft.
            for source in sources {
                found
                    .entry((account.clone(), source.thread_id.clone()))
                    .or_insert_with(|| MessagesRowDraftData {
                        draft_id: draft.id.clone(),
                        thread_id: source.thread_id,
                        text: text.clone(),
                        reply_all: draft.intent == DraftIntent::ReplyAll,
                        updated_at: draft.updated_at,
                    });
            }
        }
    }
    Ok(found)
}

/// Put each row's latest edited draft among its topics on the row.
pub(super) fn attach_drafts(rows: &mut [MessagesRowData], drafts: &ConversationDrafts) {
    if drafts.is_empty() {
        return;
    }
    for row in rows {
        row.draft = row
            .topics
            .iter()
            .filter_map(|topic| drafts.get(&(row.account_id.clone(), topic.thread_id.clone())))
            .max_by_key(|draft| draft.updated_at)
            .cloned();
    }
}
