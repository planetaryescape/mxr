//! IPC for the in-app assistant. Sending and deciding start a background
//! turn and return at once; clients follow `AssistantUpdated` events.

use super::{HandlerError, HandlerResult};
use crate::assistant::{
    abandon_pending, conversation_view, parse_pending, start_turn, summary_view, TurnStart,
};
use crate::state::AppState;
use mxr_assistant::{Answer, Decision};
use mxr_llm::LlmFeature;
use mxr_protocol::{
    AssistantAvailabilityData, AssistantDecisionData, AssistantDecisionKindData,
    AssistantDeleteData, Request, ResponseData,
};
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 500;
const TITLE_CHARS: usize = 60;
/// Longest message a user can send, in characters.
const MAX_MESSAGE_CHARS: usize = 20_000;

pub(super) async fn handle(state: &Arc<AppState>, req: &Request) -> HandlerResult {
    let result = match req {
        Request::GetAssistantAvailability => Ok(ResponseData::AssistantAvailability {
            availability: availability(state),
        }),
        Request::AssistantSend {
            conversation_id,
            message,
            context,
        } => send(state, conversation_id.as_deref(), message, context.clone()).await,
        Request::AssistantDecide {
            conversation_id,
            decisions,
        } => decide(state, conversation_id, decisions).await,
        Request::AssistantCancel { conversation_id } => cancel(state, conversation_id).await,
        Request::SetAssistantAutoApprove {
            conversation_id,
            enabled,
        } => set_auto_approve(state, conversation_id, *enabled).await,
        Request::ListAssistantConversations { limit } => {
            let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
            state
                .store
                .list_assistant_conversations(limit)
                .await
                .map(|rows| ResponseData::AssistantConversations {
                    conversations: rows.iter().map(summary_view).collect(),
                })
                .map_err(|error| error.to_string())
        }
        Request::GetAssistantConversation { conversation_id } => {
            conversation(state, conversation_id).await
        }
        Request::DeleteAssistantConversation {
            conversation_id,
            dry_run,
        } => delete(state, conversation_id, *dry_run).await,
        _ => Err("not an assistant request".to_string()),
    };
    result.map_err(HandlerError::from)
}

/// Whether a turn can run. Checked before any conversation is written, so
/// a missing model never leaves an unanswered message behind.
fn availability(state: &AppState) -> AssistantAvailabilityData {
    let pinned = state.llm.for_feature(LlmFeature::Assistant).pin();
    let unavailable = |reason: String| AssistantAvailabilityData {
        available: false,
        model: None,
        leaves_machine: false,
        reason: Some(reason),
    };
    if !pinned.is_configured() {
        return unavailable(
            "No AI model is set up. Choose one in Settings, or run `mxr llm config`, then try again."
                .to_string(),
        );
    }
    if let Some(reason) = pinned.blocked_reason() {
        return unavailable(format!(
            "The assistant reads across your mailbox, and your privacy settings keep that on this machine: {reason}"
        ));
    }
    AssistantAvailabilityData {
        available: true,
        model: Some(pinned.model_name().to_string()),
        leaves_machine: pinned.leaves_machine(),
        reason: None,
    }
}

async fn send(
    state: &Arc<AppState>,
    conversation_id: Option<&str>,
    message: &str,
    context: mxr_protocol::AssistantViewContextData,
) -> Result<ResponseData, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("the message is empty".to_string());
    }
    if message.chars().count() > MAX_MESSAGE_CHARS {
        return Err(format!(
            "the message is longer than {MAX_MESSAGE_CHARS} characters"
        ));
    }
    let availability = availability(state);
    if !availability.available {
        return Err(availability.reason.unwrap_or_default());
    }

    let (conversation_id, pending) = match conversation_id {
        Some(id) => {
            let row = state
                .store
                .get_assistant_conversation(id)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| format!("no assistant conversation {id}"))?;
            (row.id, parse_pending(row.pending_json.as_deref()))
        }
        None => {
            let id = format!("asst_{}", uuid::Uuid::now_v7().simple());
            state
                .store
                .create_assistant_conversation(&id, &title(message))
                .await
                .map_err(|error| error.to_string())?;
            (id, Vec::new())
        }
    };
    start_turn(
        state,
        &conversation_id,
        pending,
        TurnStart::Send {
            message: message.to_string(),
            context,
        },
    )
    .await?;
    conversation(state, &conversation_id).await
}

async fn decide(
    state: &Arc<AppState>,
    conversation_id: &str,
    decisions: &[AssistantDecisionData],
) -> Result<ResponseData, String> {
    if decisions.is_empty() {
        return Err("no decisions were given".to_string());
    }
    let row = state
        .store
        .get_assistant_conversation(conversation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no assistant conversation {conversation_id}"))?;
    if row.status != "awaiting_user" || state.assistant.is_running(conversation_id) {
        return Err("nothing in this conversation is waiting for you".to_string());
    }
    let pending = parse_pending(row.pending_json.as_deref());
    let mut decided = HashMap::new();
    for decision in decisions {
        if decided
            .insert(decision.call_id.clone(), to_decision(&decision.decision))
            .is_some()
        {
            return Err(format!("call {} was decided twice", decision.call_id));
        }
    }
    mxr_assistant::check_decisions(&pending, &decided)?;
    if decided
        .values()
        .any(|decision| matches!(decision, Decision::Approve))
    {
        let availability = availability(state);
        if !availability.available {
            return Err(availability.reason.unwrap_or_default());
        }
    }
    start_turn(
        state,
        conversation_id,
        pending,
        TurnStart::Decide { decisions: decided },
    )
    .await?;
    conversation(state, conversation_id).await
}

async fn cancel(state: &Arc<AppState>, conversation_id: &str) -> Result<ResponseData, String> {
    if !state.assistant.cancel(conversation_id) {
        let row = state
            .store
            .get_assistant_conversation(conversation_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("no assistant conversation {conversation_id}"))?;
        let pending = parse_pending(row.pending_json.as_deref());
        if !pending.is_empty() {
            abandon_pending(state, conversation_id, &pending).await?;
        }
    }
    conversation(state, conversation_id).await
}

async fn set_auto_approve(
    state: &Arc<AppState>,
    conversation_id: &str,
    enabled: bool,
) -> Result<ResponseData, String> {
    if !state
        .store
        .set_assistant_auto_approve(conversation_id, enabled)
        .await
        .map_err(|error| error.to_string())?
    {
        return Err(format!("no assistant conversation {conversation_id}"));
    }
    conversation(state, conversation_id).await
}

async fn conversation(state: &AppState, conversation_id: &str) -> Result<ResponseData, String> {
    let row = state
        .store
        .get_assistant_conversation(conversation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no assistant conversation {conversation_id}"))?;
    let messages = state
        .store
        .assistant_messages(conversation_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(ResponseData::AssistantConversation {
        conversation: conversation_view(&row, &messages),
    })
}

async fn delete(
    state: &AppState,
    conversation_id: &str,
    dry_run: bool,
) -> Result<ResponseData, String> {
    let row = state
        .store
        .get_assistant_conversation(conversation_id)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no assistant conversation {conversation_id}"))?;
    if state.assistant.is_running(conversation_id) {
        return Err("the assistant is working on this conversation; stop it first".to_string());
    }
    let deleted = if dry_run {
        false
    } else {
        state
            .store
            .delete_assistant_conversation(conversation_id)
            .await
            .map_err(|error| error.to_string())?
    };
    Ok(ResponseData::AssistantDelete {
        delete: AssistantDeleteData {
            conversation_id: row.id,
            title: row.title,
            message_count: row.message_count,
            dry_run,
            deleted,
        },
    })
}

fn to_decision(decision: &AssistantDecisionKindData) -> Decision {
    match decision {
        AssistantDecisionKindData::Approve => Decision::Approve,
        AssistantDecisionKindData::Reject { reason } => Decision::Reject {
            reason: reason.clone(),
        },
        AssistantDecisionKindData::Answer { answers } => Decision::Answer {
            answers: answers
                .iter()
                .map(|answer| Answer {
                    selected: answer.selected.clone(),
                    other: answer.other.clone(),
                })
                .collect(),
        },
    }
}

/// The first line of the first message, as the conversation's name.
fn title(message: &str) -> String {
    let line = message.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= TITLE_CHARS {
        return line.to_string();
    }
    let mut title: String = line.chars().take(TITLE_CHARS - 1).collect();
    title.push('…');
    title
}

#[cfg(test)]
mod tests {
    use super::title;

    #[test]
    fn titles_are_the_first_line_cut_to_fit() {
        assert_eq!(title("  Archive bank mail\nand more"), "Archive bank mail");
        let long = "x".repeat(80);
        assert_eq!(title(&long).chars().count(), 60);
        assert!(title(&long).ends_with('…'));
    }
}
