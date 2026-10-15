//! The conversation as clients show it: user and assistant messages, and
//! each tool call joined with how it ended.

use super::parse_pending;
use mxr_assistant::{
    call_state, prompt, readable_result, CallState, MessageMeta, PendingItem, PendingKind,
};
use mxr_llm::ToolChatMessage;
use mxr_protocol::{
    AssistantCallStateData, AssistantConversationData, AssistantConversationSummaryData,
    AssistantItemData, AssistantOutcomeData, AssistantPendingData, AssistantQuestionData,
    AssistantQuestionOptionData, AssistantStatusData,
};
use mxr_store::{AssistantConversationRow, AssistantMessageRow};
use std::collections::{HashMap, HashSet};

/// How much of a tool result a client gets for its details view.
const RESULT_PREVIEW_CHARS: usize = 4_000;

pub(crate) fn summary_view(row: &AssistantConversationRow) -> AssistantConversationSummaryData {
    AssistantConversationSummaryData {
        id: row.id.clone(),
        title: row.title.clone(),
        status: status(&row.status),
        last_outcome: row.last_outcome.as_deref().and_then(outcome),
        created_at: row.created_at,
        updated_at: row.updated_at,
        message_count: row.message_count,
        pending_count: parse_pending(row.pending_json.as_deref()).len(),
    }
}

pub(crate) fn conversation_view(
    row: &AssistantConversationRow,
    messages: &[AssistantMessageRow],
) -> AssistantConversationData {
    let pending = parse_pending(row.pending_json.as_deref());
    let waiting: HashSet<&str> = pending.iter().map(|item| item.call_id.as_str()).collect();
    let running = row.status == "running";

    let mut items: Vec<AssistantItemData> = Vec::new();
    // Where each call's item sits, so its result can fill it in.
    let mut call_index: HashMap<String, usize> = HashMap::new();
    for row in messages {
        let Ok(message) = serde_json::from_str::<ToolChatMessage>(&row.message_json) else {
            tracing::warn!(seq = row.seq, "skipping an unreadable assistant message");
            continue;
        };
        let meta: MessageMeta = row
            .meta_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        match message {
            ToolChatMessage::System { .. } => {}
            ToolChatMessage::User { content } => items.push(AssistantItemData::User {
                seq: row.seq,
                text: prompt::user_text(&content).to_string(),
                created_at: row.created_at,
            }),
            ToolChatMessage::Assistant {
                text, tool_calls, ..
            } => {
                if !text.trim().is_empty() {
                    items.push(AssistantItemData::Assistant {
                        seq: row.seq,
                        text,
                        model: meta.model.clone(),
                        created_at: row.created_at,
                    });
                }
                for call in tool_calls {
                    let (arguments, _) = prompt::split_reason(&call.arguments);
                    let state = if waiting.contains(call.id.as_str()) {
                        AssistantCallStateData::Waiting
                    } else if running {
                        AssistantCallStateData::Running
                    } else {
                        AssistantCallStateData::Interrupted
                    };
                    call_index.insert(call.id.clone(), items.len());
                    items.push(AssistantItemData::ToolCall {
                        seq: row.seq,
                        reason: meta.reasons.get(&call.id).cloned().unwrap_or_default(),
                        call_id: call.id,
                        tool: call.name,
                        arguments,
                        state,
                        result: None,
                        created_at: row.created_at,
                    });
                }
            }
            ToolChatMessage::Tool {
                call_id,
                content,
                is_error,
            } => {
                let Some(index) = call_index.get(&call_id) else {
                    continue;
                };
                if let Some(AssistantItemData::ToolCall { state, result, .. }) =
                    items.get_mut(*index)
                {
                    *state = call_state_data(call_state(&content, is_error));
                    *result = Some(readable_result(&content, RESULT_PREVIEW_CHARS));
                }
            }
        }
    }

    AssistantConversationData {
        id: row.id.clone(),
        title: row.title.clone(),
        status: status(&row.status),
        last_outcome: row.last_outcome.as_deref().and_then(outcome),
        last_error: row.last_error.clone(),
        created_at: row.created_at,
        updated_at: row.updated_at,
        auto_approve: row.auto_approve,
        items: with_summary(items, row),
        pending: pending.into_iter().map(pending_view).collect(),
    }
}

/// The summary sits where the messages it replaces end.
fn with_summary(
    mut items: Vec<AssistantItemData>,
    row: &AssistantConversationRow,
) -> Vec<AssistantItemData> {
    let (Some(text), Some(through)) = (&row.summary, row.summary_through_seq) else {
        return items;
    };
    let at = items
        .iter()
        .position(|item| item_seq(item) > through)
        .unwrap_or(items.len());
    items.insert(
        at,
        AssistantItemData::Summary {
            seq: through,
            text: text.clone(),
            created_at: row.updated_at,
        },
    );
    items
}

fn item_seq(item: &AssistantItemData) -> i64 {
    match item {
        AssistantItemData::User { seq, .. }
        | AssistantItemData::Assistant { seq, .. }
        | AssistantItemData::Summary { seq, .. }
        | AssistantItemData::ToolCall { seq, .. } => *seq,
    }
}

fn pending_view(item: PendingItem) -> AssistantPendingData {
    match item.kind {
        PendingKind::Approval {
            tool,
            arguments,
            reason,
            preview,
            preview_error,
        } => AssistantPendingData::Approval {
            call_id: item.call_id,
            tool,
            arguments,
            reason,
            preview,
            preview_error,
        },
        PendingKind::Question { questions, reason } => AssistantPendingData::Question {
            call_id: item.call_id,
            reason,
            questions: questions
                .into_iter()
                .map(|question| AssistantQuestionData {
                    allow_other: question.allow_other || question.options.is_empty(),
                    question: question.question,
                    header: question.header,
                    options: question
                        .options
                        .into_iter()
                        .map(|option| AssistantQuestionOptionData {
                            label: option.label,
                            description: option.description,
                        })
                        .collect(),
                    multi_select: question.multi_select,
                })
                .collect(),
        },
    }
}

fn call_state_data(state: CallState) -> AssistantCallStateData {
    match state {
        CallState::Done => AssistantCallStateData::Done,
        CallState::Failed => AssistantCallStateData::Failed,
        CallState::Approved => AssistantCallStateData::Approved,
        CallState::AutoApproved => AssistantCallStateData::AutoApproved,
        CallState::Declined => AssistantCallStateData::Declined,
        CallState::Answered => AssistantCallStateData::Answered,
        CallState::Cancelled => AssistantCallStateData::Cancelled,
    }
}

fn status(stored: &str) -> AssistantStatusData {
    match stored {
        "running" => AssistantStatusData::Running,
        "awaiting_user" => AssistantStatusData::AwaitingUser,
        _ => AssistantStatusData::Idle,
    }
}

fn outcome(stored: &str) -> Option<AssistantOutcomeData> {
    Some(match stored {
        "finished" => AssistantOutcomeData::Finished,
        "cancelled" => AssistantOutcomeData::Cancelled,
        "step_limit" => AssistantOutcomeData::StepLimit,
        "failed" => AssistantOutcomeData::Failed,
        "interrupted" => AssistantOutcomeData::Interrupted,
        _ => return None,
    })
}
