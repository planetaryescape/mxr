use super::messages_view::preview_text;
use super::HandlerError;
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Draft, DraftIntent, DraftStatus};
use mxr_protocol::{MessagesRowData, MessagesRowDraftData};
use std::collections::HashMap;

pub(super) type ConversationDrafts = HashMap<(AccountId, ThreadId), MessagesRowDraftData>;

pub(super) async fn conversation_drafts(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<ConversationDrafts, HandlerError> {
    let mut found = ConversationDrafts::new();
    for account in accounts {
        let mut drafts = state.store.list_drafts(account).await?;
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
