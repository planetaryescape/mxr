//! The conversation as clients show it: user and assistant messages, and
//! each tool call joined with how it ended.

use super::links::KnownLinks;
use super::parse_pending;
use super::readout::readout;
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
    // What the model was given, so the emails and people its replies name
    // link to the right place.
    let mut links = KnownLinks::default();
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
                        failed_call_ids: Vec::new(),
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
                        readout: None,
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
                if let Some(AssistantItemData::ToolCall {
                    state,
                    result,
                    readout: found,
                    ..
                }) = items.get_mut(*index)
                {
                    *state = call_state_data(call_state(&content, is_error));
                    let whole = readable_result(&content, usize::MAX);
                    *found = readout(&whole);
                    *result = Some(readable_result(&whole, RESULT_PREVIEW_CHARS));
                    links.add_result(&whole);
                }
            }
        }
    }

    for item in &mut items {
        if let AssistantItemData::Assistant { text, .. } = item {
            *text = links.apply(text);
        }
    }
    mark_failed_steps(&mut items);

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

/// Note on each reply the steps right before it that failed and that no
/// later step among them redid. A retry counts when it is the same tool
/// with the same arguments, apart from `confirm`: a change mxr refused for
/// lacking it and then made with it did happen.
fn mark_failed_steps(items: &mut [AssistantItemData]) {
    let succeeded = |state: &AssistantCallStateData| {
        matches!(
            state,
            AssistantCallStateData::Done
                | AssistantCallStateData::Approved
                | AssistantCallStateData::AutoApproved
        )
    };
    let without_confirm = |arguments: &serde_json::Value| {
        let mut arguments = arguments.clone();
        if let Some(object) = arguments.as_object_mut() {
            object.remove("confirm");
        }
        arguments
    };
    let mut steps_start = 0;
    for index in 0..items.len() {
        match &items[index] {
            AssistantItemData::ToolCall { .. } => continue,
            AssistantItemData::Assistant { .. } => {}
            _ => {
                steps_start = index + 1;
                continue;
            }
        }
        let steps: Vec<(&str, &str, serde_json::Value, AssistantCallStateData)> = items
            [steps_start..index]
            .iter()
            .filter_map(|item| match item {
                AssistantItemData::ToolCall {
                    call_id,
                    tool,
                    arguments,
                    state,
                    ..
                } => Some((
                    call_id.as_str(),
                    tool.as_str(),
                    without_confirm(arguments),
                    *state,
                )),
                _ => None,
            })
            .collect();
        let failed: Vec<String> = steps
            .iter()
            .enumerate()
            .filter(|(at, (_, tool, arguments, state))| {
                *state == AssistantCallStateData::Failed
                    && !steps[at + 1..]
                        .iter()
                        .any(|(_, later_tool, later_arguments, later)| {
                            later_tool == tool && later_arguments == arguments && succeeded(later)
                        })
            })
            .map(|(_, (call_id, ..))| call_id.to_string())
            .collect();
        if let AssistantItemData::Assistant {
            failed_call_ids, ..
        } = &mut items[index]
        {
            *failed_call_ids = failed;
        }
        steps_start = index + 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(seq: i64) -> AssistantItemData {
        AssistantItemData::User {
            seq,
            text: "star it".into(),
            created_at: 0,
        }
    }

    fn step(
        call_id: &str,
        arguments: serde_json::Value,
        state: AssistantCallStateData,
    ) -> AssistantItemData {
        AssistantItemData::ToolCall {
            seq: 0,
            call_id: call_id.into(),
            tool: "mxr_mutate".into(),
            reason: String::new(),
            arguments,
            state,
            result: None,
            readout: None,
            created_at: 0,
        }
    }

    fn reply(seq: i64) -> AssistantItemData {
        AssistantItemData::Assistant {
            seq,
            text: "Starred it.".into(),
            model: None,
            failed_call_ids: Vec::new(),
            created_at: 0,
        }
    }

    fn failed_ids(item: &AssistantItemData) -> Vec<&str> {
        match item {
            AssistantItemData::Assistant {
                failed_call_ids, ..
            } => failed_call_ids.iter().map(String::as_str).collect(),
            other => panic!("not a reply: {other:?}"),
        }
    }

    #[test]
    fn a_reply_names_the_failed_steps_before_it_that_no_retry_redid() {
        use AssistantCallStateData::{AutoApproved, Done, Failed};
        let star = json!({"action": "star", "message_ids": ["m1"]});
        let mut items = vec![
            user(1),
            step("bad-shape", json!({"action": {"is_starred": true}}), Failed),
            reply(2),
            user(3),
            // Refused for lacking confirm, then made with it: it happened.
            step("refused", star.clone(), Failed),
            step("preview", json!({"preview": true}), Done),
            step(
                "made",
                json!({"action": "star", "message_ids": ["m1"], "confirm": true}),
                AutoApproved,
            ),
            // A retry on another message doesn't redo this one.
            step(
                "other",
                json!({"action": "star", "message_ids": ["m2"]}),
                Failed,
            ),
            step(
                "other-retry",
                json!({"action": "star", "message_ids": ["m3"], "confirm": true}),
                AutoApproved,
            ),
            reply(4),
            user(5),
            reply(6),
        ];
        mark_failed_steps(&mut items);
        assert_eq!(failed_ids(&items[2]), vec!["bad-shape"]);
        assert_eq!(failed_ids(&items[9]), vec!["other"]);
        assert!(failed_ids(&items[11]).is_empty());
    }
}
