//! The conversation as clients show it: user and assistant messages, and
//! each tool call joined with how it ended.

use super::links::{linked_objects, KnownLinks};
use super::parse_pending;
use super::readout::{listed_rows, readout};
use super::titles::{change_title, settled_state, step_title, Context};
use mxr_assistant::{
    call_state, prompt, readable_result, CallState, MessageMeta, PendingItem, PendingKind,
};
use mxr_llm::ToolChatMessage;
use mxr_protocol::{
    AssistantCallStateData, AssistantConversationData, AssistantConversationSummaryData,
    AssistantItemData, AssistantOutcomeData, AssistantPendingData, AssistantQuestionData,
    AssistantQuestionOptionData, AssistantReadoutRowData, AssistantStatusData,
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
    // Everything each step listed, by the step's place in `items`, for the
    // rows a reply names.
    let mut listed: HashMap<usize, Vec<AssistantReadoutRowData>> = HashMap::new();
    // Each result whole, by its step's place in `items`: titles and the
    // failure checks read these, since `result` is cut for clients.
    let mut wholes: HashMap<usize, String> = HashMap::new();
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
                        leaves_machine: meta.leaves_machine,
                        failed_call_ids: Vec::new(),
                        rows: Vec::new(),
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
                        title: None,
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
                    listed.insert(*index, listed_rows(&whole));
                    wholes.insert(*index, whole.clone());
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
    fill_titles(&mut items, &pending, &wholes);
    mark_failed_steps(&mut items);
    attach_rows(&mut items, &listed);

    AssistantConversationData {
        id: row.id.clone(),
        title: row.title.clone(),
        status: status(&row.status),
        last_outcome: row.last_outcome.as_deref().and_then(outcome),
        last_error: row.last_error.as_deref().map(plain_error),
        last_error_detail: row
            .last_error
            .as_deref()
            .filter(|raw| plain_error(raw) != *raw)
            .map(str::to_string),
        created_at: row.created_at,
        updated_at: row.updated_at,
        auto_approve: row.auto_approve,
        items: with_summary(items, row),
        pending: pending.into_iter().map(pending_view).collect(),
    }
}

/// Most rows a reply gets.
const REPLY_ROWS: usize = 8;

/// Put the rows a reply names under it: when it links two or more emails
/// or people that one step of its own turn listed, those rows, in the
/// order the reply names them. The step that lists most of them wins; a
/// later step wins a tie, being the one the reply most likely reads from.
fn attach_rows(
    items: &mut [AssistantItemData],
    listed: &HashMap<usize, Vec<AssistantReadoutRowData>>,
) {
    let mut turn_start = 0;
    let mut attached: Vec<(usize, Vec<AssistantReadoutRowData>)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            AssistantItemData::User { .. } => turn_start = index + 1,
            AssistantItemData::Assistant { text, .. } => {
                let named = linked_objects(text);
                if named.len() < 2 {
                    continue;
                }
                let mut best: Vec<AssistantReadoutRowData> = Vec::new();
                for step in turn_start..index {
                    let Some(rows) = listed.get(&step) else {
                        continue;
                    };
                    let matched: Vec<AssistantReadoutRowData> = named
                        .iter()
                        .filter_map(|link| {
                            rows.iter()
                                .find(|row| row.link.as_ref() == Some(link))
                                .cloned()
                        })
                        .collect();
                    if matched.len() >= 2 && matched.len() >= best.len() {
                        best = matched;
                    }
                }
                best.truncate(REPLY_ROWS);
                attached.push((index, best));
            }
            _ => {}
        }
    }
    for (index, best) in attached {
        if let Some(AssistantItemData::Assistant { rows, .. }) = items.get_mut(index) {
            *rows = best;
        }
    }
}

/// Give each step its words, now that its state, result and (for a call
/// waiting on the user) preview are known.
fn fill_titles(
    items: &mut [AssistantItemData],
    pending: &[PendingItem],
    wholes: &HashMap<usize, String>,
) {
    for (index, item) in items.iter_mut().enumerate() {
        let AssistantItemData::ToolCall {
            call_id,
            tool,
            arguments,
            state,
            title,
            ..
        } = item
        else {
            continue;
        };
        let whole = wholes.get(&index).map(String::as_str);
        let preview = pending
            .iter()
            .find(|waiting| waiting.call_id == *call_id)
            .and_then(|waiting| match &waiting.kind {
                PendingKind::Approval { preview, .. } => preview.as_ref(),
                PendingKind::Question { .. } => None,
            });
        // A result that says nothing happened is a failure whatever state
        // it was stored in, so the glyph, the title and the failure note agree.
        *state = settled_state(tool, *state, whole);
        *title = Some(step_title(
            tool,
            arguments,
            *state,
            Context {
                preview,
                result: whole,
            },
            whole,
        ));
    }
}

/// A turn's failure as a sentence with what to do next. The raw text names
/// the model's provider and is kept in `last_error_detail`.
fn plain_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let has = |needle: &str| lower.contains(needle);
    let settings = "Settings › Language model";
    if has("empty completion") {
        format!(
            "The model stopped answering. Send your message again, or switch models in {settings}."
        )
    } else if has("unreachable") {
        format!("mxr couldn't reach the model. Check that it is running, or switch models in {settings}.")
    } else if has("rate-limited") {
        "The model's provider is limiting requests right now. Wait a moment, then send your message again.".to_string()
    } else if has("timed out") {
        format!("The model took too long to answer. Send your message again, or switch to a faster model in {settings}.")
    } else if has("authentication failed") {
        format!("The model's provider didn't accept the API key. Check it in {settings}.")
    } else if has("privacy policy") {
        format!("Your privacy settings don't let the assistant use this model. Change the model in {settings}.")
    } else if has("disabled in config") {
        format!("The language model is switched off. Turn it on in {settings}.")
    } else if has("paused for") {
        "The model kept failing, so mxr paused it for a while. Try again in a minute.".to_string()
    } else if has("can't call tools") {
        format!("This model can't use mxr's tools. Choose another in {settings}.")
    } else if has("tools didn't start") {
        "The assistant's tools didn't start. Send your message again; if it keeps happening, restart mxr.".to_string()
    } else {
        crate::output::terminal_text(raw).into_owned()
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

/// Changes whose effect the user can't take back from mxr.
const IRREVERSIBLE_TOOLS: &[&str] = &[
    "mxr_send_draft",
    "mxr_got_it",
    "mxr_delete_draft",
    "mxr_sorting_rule",
    "mxr_category_rule",
    "mxr_category_delete",
    "mxr_copy_draft_to_provider",
    "mxr_sync_draft_to_provider",
    "mxr_reading_item",
];
const SENDING_TOOLS: &[&str] = &["mxr_send_draft", "mxr_got_it"];

/// The warning above Apply, or None for a change that can be put back. One
/// rule for every client (docs/blueprint/29). The words are fixed here and
/// only the tool and `action` choose between them, so nothing a model or an
/// email wrote can reach the text.
pub(crate) fn irreversible_warning(tool: &str, arguments: &serde_json::Value) -> Option<String> {
    let trash_or_spam = tool == "mxr_mutate"
        && matches!(
            arguments.get("action").and_then(serde_json::Value::as_str),
            Some("trash" | "spam")
        );
    if !IRREVERSIBLE_TOOLS.contains(&tool) && !trash_or_spam {
        return None;
    }
    Some(if SENDING_TOOLS.contains(&tool) {
        "Sends an email. Can't be undone.".to_string()
    } else {
        "Can't be undone.".to_string()
    })
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
            title: Some(change_title(&tool, &arguments, preview.as_ref())),
            warning: irreversible_warning(&tool, &arguments),
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
            title: None,
            created_at: 0,
        }
    }

    fn reply(seq: i64) -> AssistantItemData {
        AssistantItemData::Assistant {
            seq,
            text: "Starred it.".into(),
            model: None,
            leaves_machine: None,
            failed_call_ids: Vec::new(),
            rows: Vec::new(),
            created_at: 0,
        }
    }

    fn failed_ids(item: &AssistantItemData) -> Option<Vec<&str>> {
        match item {
            AssistantItemData::Assistant {
                failed_call_ids, ..
            } => Some(failed_call_ids.iter().map(String::as_str).collect()),
            _ => None,
        }
    }

    #[test]
    fn a_reply_names_the_failed_steps_before_it_that_no_retry_redid() {
        use AssistantCallStateData::{AutoApproved, Done, Failed};
        let star = json!({"action": "star", "message_ids": ["m1"], "confirm": true});
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
        assert_eq!(failed_ids(&items[2]), Some(vec!["bad-shape"]));
        assert_eq!(failed_ids(&items[9]), Some(vec!["other"]));
        assert_eq!(failed_ids(&items[11]), Some(vec![]));
    }

    fn row(title: &str, thread: &str) -> AssistantReadoutRowData {
        AssistantReadoutRowData {
            title: title.into(),
            detail: None,
            link: Some(mxr_protocol::AssistantLinkData::Thread {
                thread_id: thread.into(),
            }),
        }
    }

    fn reply_with(text: &str) -> AssistantItemData {
        AssistantItemData::Assistant {
            seq: 9,
            text: text.into(),
            model: None,
            leaves_machine: None,
            failed_call_ids: Vec::new(),
            rows: Vec::new(),
            created_at: 0,
        }
    }

    fn reply_rows(item: &AssistantItemData) -> Option<Vec<&str>> {
        match item {
            AssistantItemData::Assistant { rows, .. } => {
                Some(rows.iter().map(|row| row.title.as_str()).collect())
            }
            _ => None,
        }
    }

    #[test]
    fn a_reply_gets_the_rows_it_names_in_the_order_it_names_them() {
        // The step lists a, b, c, d; the reply names d then b.
        let listed = HashMap::from([(
            1,
            vec![
                row("A", "t-aaaaaaaa"),
                row("B", "t-bbbbbbbb"),
                row("C", "t-cccccccc"),
                row("D", "t-dddddddd"),
            ],
        )]);
        let mut items = vec![
            user(0),
            step("s", json!({}), AssistantCallStateData::Done),
            reply_with("First [D](mxr:thread/t-dddddddd), then [B](mxr:thread/t-bbbbbbbb)."),
        ];
        attach_rows(&mut items, &listed);
        assert_eq!(reply_rows(&items[2]), Some(vec!["D", "B"]));
    }

    #[test]
    fn rows_need_two_named_objects_from_one_step_of_the_same_turn() {
        let listed = HashMap::from([
            (1, vec![row("A", "t-aaaaaaaa")]),
            (2, vec![row("B", "t-bbbbbbbb")]),
        ]);
        let two_steps = vec![
            user(0),
            step("s1", json!({}), AssistantCallStateData::Done),
            step("s2", json!({}), AssistantCallStateData::Done),
            reply_with("[A](mxr:thread/t-aaaaaaaa) and [B](mxr:thread/t-bbbbbbbb)"),
        ];
        let mut items = two_steps;
        attach_rows(&mut items, &listed);
        assert_eq!(
            reply_rows(&items[3]),
            Some(vec![]),
            "one each from two steps"
        );

        // A step from an earlier turn is not this reply's.
        let listed = HashMap::from([(1, vec![row("A", "t-aaaaaaaa"), row("B", "t-bbbbbbbb")])]);
        let mut items = vec![
            user(0),
            step("old", json!({}), AssistantCallStateData::Done),
            reply_with("done"),
            user(3),
            reply_with("[A](mxr:thread/t-aaaaaaaa) and [B](mxr:thread/t-bbbbbbbb)"),
        ];
        attach_rows(&mut items, &listed);
        assert_eq!(reply_rows(&items[4]), Some(vec![]));
    }

    #[test]
    fn a_reply_that_links_one_object_or_names_it_twice_gets_no_rows() {
        let listed = HashMap::from([(1, vec![row("A", "t-aaaaaaaa"), row("B", "t-bbbbbbbb")])]);
        let mut items = vec![
            user(0),
            step("s", json!({}), AssistantCallStateData::Done),
            reply_with("[A](mxr:thread/t-aaaaaaaa) and again [A](mxr:thread/t-aaaaaaaa)"),
        ];
        attach_rows(&mut items, &listed);
        assert_eq!(reply_rows(&items[2]), Some(vec![]));
    }

    #[test]
    fn rows_are_capped_at_eight() {
        let many: Vec<_> = (0..12)
            .map(|n| row(&format!("R{n}"), &format!("t-{n:08}")))
            .collect();
        let text: String = (0..12)
            .map(|n| format!("[R{n}](mxr:thread/t-{n:08}) "))
            .collect();
        let listed = HashMap::from([(1, many)]);
        let mut items = vec![
            user(0),
            step("s", json!({}), AssistantCallStateData::Done),
            reply_with(&text),
        ];
        attach_rows(&mut items, &listed);
        assert_eq!(
            reply_rows(&items[2]).map(|rows| rows.len()),
            Some(REPLY_ROWS)
        );
    }

    #[test]
    fn steps_are_titled_from_their_own_state_and_arguments() {
        let mut items = vec![
            step(
                "a",
                json!({"action": "archive", "message_ids": ["m1", "m2"], "confirm": true}),
                AssistantCallStateData::Waiting,
            ),
            step(
                "b",
                json!({"action": "star", "message_ids": ["m1"]}),
                AssistantCallStateData::Failed,
            ),
        ];
        fill_titles(&mut items, &[], &HashMap::new());
        let title = |item: &AssistantItemData| match item {
            AssistantItemData::ToolCall { title, .. } => title.clone(),
            _ => None,
        };
        let waiting = title(&items[0]).map(|t| (t.running, t.done));
        assert_eq!(
            waiting,
            Some((
                "Archiving 2 emails".to_string(),
                "Waiting for you: archive 2 emails".to_string()
            ))
        );
        assert_eq!(
            title(&items[1]).map(|t| t.done),
            Some("Couldn't star 1 email".to_string())
        );
    }

    #[test]
    fn a_waiting_approval_carries_the_daemons_title() {
        let view = pending_view(PendingItem {
            call_id: "c1".into(),
            kind: PendingKind::Approval {
                tool: "mxr_mutate".into(),
                arguments: json!({"action": "archive", "message_ids": ["a", "b", "c"], "confirm": true}),
                reason: "old".into(),
                preview: None,
                preview_error: None,
            },
        });
        assert!(matches!(
            view,
            AssistantPendingData::Approval { title: Some(ref title), .. } if title == "Archive 3 emails"
        ));
    }

    fn warning_for(tool: &str, arguments: serde_json::Value) -> Option<String> {
        match pending_view(PendingItem {
            call_id: "c1".into(),
            kind: PendingKind::Approval {
                tool: tool.into(),
                arguments,
                reason: "why".into(),
                preview: None,
                preview_error: None,
            },
        }) {
            AssistantPendingData::Approval { warning, .. } => warning,
            // Fails every comparison below, so it can't pass unseen.
            AssistantPendingData::Question { .. } => Some("not an approval".into()),
        }
    }

    #[test]
    fn sends_and_trash_warn_and_an_archive_does_not() {
        assert_eq!(
            warning_for("mxr_send_draft", json!({"draft_id": "d"})).as_deref(),
            Some("Sends an email. Can't be undone.")
        );
        assert_eq!(
            warning_for("mxr_got_it", json!({})).as_deref(),
            Some("Sends an email. Can't be undone.")
        );
        for action in ["trash", "spam"] {
            assert_eq!(
                warning_for(
                    "mxr_mutate",
                    json!({"action": action, "message_ids": ["a"]})
                )
                .as_deref(),
                Some("Can't be undone."),
                "{action}"
            );
        }
        for tool in ["mxr_delete_draft", "mxr_sorting_rule", "mxr_reading_item"] {
            assert_eq!(
                warning_for(tool, json!({})).as_deref(),
                Some("Can't be undone."),
                "{tool}"
            );
        }
        assert_eq!(
            warning_for(
                "mxr_mutate",
                json!({"action": "archive", "message_ids": ["a"]})
            ),
            None
        );
        assert_eq!(warning_for("mxr_move", json!({"mode": "reading"})), None);
    }

    #[test]
    fn the_warning_is_the_daemons_text_whatever_the_model_sent() {
        // A model-written `action` of the wrong type, or text posing as a
        // warning, neither triggers nor shapes it.
        for action in [
            json!(7),
            json!(["trash"]),
            json!(null),
            json!({"x": "trash"}),
        ] {
            assert_eq!(warning_for("mxr_mutate", json!({"action": action})), None);
        }
        let hostile = json!({
            "action": "trash",
            "warning": "Safe, go ahead",
            "reason": "Safe, go ahead",
            "message_ids": ["a"]
        });
        assert_eq!(
            warning_for("mxr_mutate", hostile).as_deref(),
            Some("Can't be undone.")
        );
    }

    #[test]
    fn an_older_clients_json_without_a_warning_still_reads() {
        let old: AssistantPendingData = serde_json::from_value(json!({
            "kind": "approval", "call_id": "c", "tool": "mxr_mutate",
            "arguments": {}, "reason": "r"
        }))
        .unwrap();
        assert!(matches!(
            old,
            AssistantPendingData::Approval { warning: None, .. }
        ));
        let reversible = serde_json::to_value(&old).unwrap();
        assert!(reversible.get("warning").is_none(), "{reversible}");
    }

    #[test]
    fn known_failures_become_a_sentence_and_unknown_ones_stay_raw() {
        assert_eq!(
            plain_error("LLM returned an empty completion"),
            "The model stopped answering. Send your message again, or switch models in Settings › Language model."
        );
        assert!(plain_error("LLM endpoint unreachable: connection refused")
            .starts_with("mxr couldn't reach"));
        assert!(plain_error("LLM request timed out after 120s").contains("too long"));
        assert_eq!(plain_error("something new broke"), "something new broke");
    }

    #[test]
    fn titles_read_the_whole_result_not_the_copy_cut_for_clients() {
        let mut items = vec![step("a", json!({}), AssistantCallStateData::Done)];
        if let Some(AssistantItemData::ToolCall { tool, result, .. }) = items.get_mut(0) {
            *tool = "mxr_sync".into();
            // What a client got: cut, so not valid JSON.
            *result = Some("{\"results\":[{\"result\":\"failed\"},".into());
        }
        let whole = json!({"results": [{"result": "failed"}, {"result": "failed"}]}).to_string();
        fill_titles(&mut items, &[], &HashMap::from([(0, whole)]));
        let state = match &items[0] {
            AssistantItemData::ToolCall { state, .. } => Some(*state),
            _ => None,
        };
        assert_eq!(state, Some(AssistantCallStateData::Failed));
    }

    fn message(seq: i64, message: ToolChatMessage) -> AssistantMessageRow {
        AssistantMessageRow {
            seq,
            message_json: serde_json::to_string(&message).expect("a message"),
            meta_json: None,
            created_at: 0,
        }
    }

    #[test]
    fn a_plain_reply_that_quotes_listed_titles_gets_their_links_and_rows() {
        let conversation = AssistantConversationRow {
            id: "conv-1".into(),
            title: "To do".into(),
            status: "idle".into(),
            last_outcome: None,
            last_error: None,
            pending_json: None,
            auto_approve: false,
            summary: None,
            summary_through_seq: None,
            created_at: 0,
            updated_at: 0,
            message_count: 4,
        };
        let todos = json!({"todos": {"your_turn": [
            {"title": "Pay Patreon", "thread_id": "thread-pat-1"},
            {"title": "Book an engineer to come out", "thread_id": "thread-eng-1"},
            {"title": "Renew passport", "thread_id": "thread-pass-1"},
        ]}});
        let messages = vec![
            message(
                1,
                ToolChatMessage::User {
                    content: "what pressing thing do i need to take care of".into(),
                },
            ),
            message(
                2,
                ToolChatMessage::Assistant {
                    text: String::new(),
                    tool_calls: vec![mxr_llm::ToolCall {
                        id: "call-1".into(),
                        name: "mxr_todo_list".into(),
                        arguments: json!({}),
                    }],
                    provider_turn: None,
                },
            ),
            message(
                3,
                ToolChatMessage::Tool {
                    call_id: "call-1".into(),
                    content: todos.to_string(),
                    is_error: false,
                },
            ),
            message(
                4,
                ToolChatMessage::Assistant {
                    // Plain prose: no mxr: links, as small local models write.
                    text: "Pay Patreon is due first, then Book an engineer to come out.".into(),
                    tool_calls: Vec::new(),
                    provider_turn: None,
                },
            ),
        ];

        let view = conversation_view(&conversation, &messages);
        let reply = view
            .items
            .iter()
            .find_map(|item| match item {
                AssistantItemData::Assistant { text, rows, .. } => Some((text, rows)),
                _ => None,
            })
            .expect("a reply");
        assert_eq!(
            reply.0,
            "[Pay Patreon](mxr:thread/thread-pat-1) is due first, then \
             [Book an engineer to come out](mxr:thread/thread-eng-1)."
        );
        let titles: Vec<&str> = reply.1.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, vec!["Pay Patreon", "Book an engineer to come out"]);
    }
    fn stored_reply(
        seq: i64,
        text: &str,
        model: &str,
        leaves_machine: bool,
    ) -> mxr_store::AssistantMessageRow {
        let message = mxr_llm::ToolChatMessage::Assistant {
            text: text.into(),
            tool_calls: Vec::new(),
            provider_turn: None,
        };
        let meta = MessageMeta {
            model: Some(model.into()),
            leaves_machine: Some(leaves_machine),
            ..MessageMeta::default()
        };
        mxr_store::AssistantMessageRow {
            seq,
            message_json: serde_json::to_string(&message).unwrap(),
            meta_json: Some(serde_json::to_string(&meta).unwrap()),
            created_at: 0,
        }
    }

    #[test]
    fn each_reply_keeps_the_model_and_place_that_wrote_it_after_a_switch() {
        let row = mxr_store::AssistantConversationRow {
            id: "c".into(),
            title: "t".into(),
            status: "idle".into(),
            last_outcome: None,
            last_error: None,
            pending_json: None,
            auto_approve: false,
            summary: None,
            summary_through_seq: None,
            created_at: 0,
            updated_at: 0,
            message_count: 2,
        };
        let view = conversation_view(
            &row,
            &[
                stored_reply(1, "From the local model.", "gemma4:latest", false),
                stored_reply(
                    2,
                    "From the hosted one.",
                    "claude-haiku-5-5 via Anthropic",
                    true,
                ),
            ],
        );
        let marks: Vec<(Option<&str>, Option<bool>)> = view
            .items
            .iter()
            .filter_map(|item| match item {
                AssistantItemData::Assistant {
                    model,
                    leaves_machine,
                    ..
                } => Some((model.as_deref(), *leaves_machine)),
                _ => None,
            })
            .collect();
        assert_eq!(
            marks,
            [
                (Some("gemma4:latest"), Some(false)),
                (Some("claude-haiku-5-5 via Anthropic"), Some(true)),
            ]
        );
    }
}
