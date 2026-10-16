//! `mxr assistant`: the in-app assistant from the terminal. Sending and
//! deciding start a turn in the daemon; by default the command waits for it
//! to settle and prints the conversation, with the command that answers
//! anything left waiting.

use crate::cli::{AssistantAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;
use std::time::{Duration, Instant};

/// How often a waiting command checks on the turn.
const POLL_EVERY: Duration = Duration::from_millis(400);
/// How long a command waits before handing back a turn still running.
const WAIT_AT_MOST: Duration = Duration::from_secs(600);
/// Messages listed under a waiting change before "and N more".
const PREVIEW_LINES: usize = 8;

pub async fn run(action: AssistantAction, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let format = resolve_format(format);
    let mut client = IpcClient::connect().await?;
    match action {
        AssistantAction::Status => {
            let response = client.request(Request::GetAssistantAvailability).await?;
            let ResponseData::AssistantAvailability { availability } = ok(response)? else {
                anyhow::bail!("unexpected response to GetAssistantAvailability");
            };
            if is_json(&format) {
                print_json(&availability, &format)?;
            } else if availability.available {
                let place = if availability.leaves_machine {
                    "off this machine"
                } else {
                    "on this machine"
                };
                println!(
                    "Ready on {} ({place}).",
                    availability
                        .model
                        .as_deref()
                        .unwrap_or("the configured model")
                );
            } else {
                println!(
                    "Not available: {}",
                    availability.reason.as_deref().unwrap_or("unknown reason")
                );
            }
            Ok(())
        }
        AssistantAction::Send {
            message,
            conversation,
            thread,
            messages,
            no_wait,
        } => {
            let request = Request::AssistantSend {
                conversation_id: conversation,
                message,
                context: AssistantViewContextData {
                    view: Some("cli".to_string()),
                    open_thread_id: thread,
                    selected_message_ids: messages,
                    ..AssistantViewContextData::default()
                },
            };
            let started = conversation_from(client.request(request).await?)?;
            let since = started.items.last().map_or(0, item_seq);
            finish(&mut client, started, since, no_wait, format).await
        }
        AssistantAction::Approve {
            conversation,
            call_id,
            no_wait,
        } => {
            decide(
                &mut client,
                conversation,
                call_id,
                AssistantDecisionKindData::Approve,
                no_wait,
                format,
            )
            .await
        }
        AssistantAction::Reject {
            conversation,
            call_id,
            reason,
            no_wait,
        } => {
            decide(
                &mut client,
                conversation,
                call_id,
                AssistantDecisionKindData::Reject { reason },
                no_wait,
                format,
            )
            .await
        }
        AssistantAction::Answer {
            conversation,
            call_id,
            choose,
            other,
            answers_json,
            no_wait,
        } => {
            let answers = match answers_json {
                Some(json) => serde_json::from_str::<Vec<AssistantAnswerData>>(&json)
                    .map_err(|error| anyhow::anyhow!("--answers-json: {error}"))?,
                None => vec![AssistantAnswerData {
                    selected: choose,
                    other,
                }],
            };
            decide(
                &mut client,
                conversation,
                call_id,
                AssistantDecisionKindData::Answer { answers },
                no_wait,
                format,
            )
            .await
        }
        AssistantAction::Trust { conversation, off } => {
            let response = client
                .request(Request::SetAssistantAutoApprove {
                    conversation_id: conversation,
                    enabled: !off,
                })
                .await?;
            let conversation = conversation_from(response)?;
            if is_json(&format) {
                return print_json(&conversation, &format);
            }
            if conversation.auto_approve {
                println!("Reversible changes in \"{}\" now run without asking. Sending, deleting, trash, spam and rules still ask.", conversation.title);
            } else {
                println!(
                    "Every change in \"{}\" asks first again.",
                    conversation.title
                );
            }
            Ok(())
        }
        AssistantAction::Stop { conversation } => {
            let response = client
                .request(Request::AssistantCancel {
                    conversation_id: conversation,
                })
                .await?;
            let conversation = conversation_from(response)?;
            let conversation = wait(&mut client, conversation).await?;
            print_conversation(&conversation, 0, format)
        }
        AssistantAction::List { limit } => {
            let response = client
                .request(Request::ListAssistantConversations { limit })
                .await?;
            let ResponseData::AssistantConversations { conversations } = ok(response)? else {
                anyhow::bail!("unexpected response to ListAssistantConversations");
            };
            if is_json(&format) {
                return print_json(&conversations, &format);
            }
            if conversations.is_empty() {
                println!("No conversations yet. Start one with: mxr assistant send \"...\"");
            }
            for conversation in conversations {
                let waiting = if conversation.pending_count > 0 {
                    format!("  ({} waiting for you)", conversation.pending_count)
                } else {
                    String::new()
                };
                println!(
                    "{}  {}  {}{waiting}",
                    conversation.id,
                    when(conversation.updated_at),
                    conversation.title
                );
            }
            Ok(())
        }
        AssistantAction::Show { conversation } => {
            let response = client
                .request(Request::GetAssistantConversation {
                    conversation_id: conversation,
                })
                .await?;
            print_conversation(&conversation_from(response)?, 0, format)
        }
        AssistantAction::Delete {
            conversation,
            dry_run,
        } => {
            let response = client
                .request(Request::DeleteAssistantConversation {
                    conversation_id: conversation,
                    dry_run,
                })
                .await?;
            let ResponseData::AssistantDelete { delete } = ok(response)? else {
                anyhow::bail!("unexpected response to DeleteAssistantConversation");
            };
            if is_json(&format) {
                return print_json(&delete, &format);
            }
            let verb = if delete.dry_run {
                "Would delete"
            } else {
                "Deleted"
            };
            println!(
                "{verb} \"{}\" ({} messages).",
                delete.title, delete.message_count
            );
            Ok(())
        }
    }
}

async fn decide(
    client: &mut IpcClient,
    conversation_id: String,
    call_id: String,
    decision: AssistantDecisionKindData,
    no_wait: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let response = client
        .request(Request::AssistantDecide {
            conversation_id,
            decisions: vec![AssistantDecisionData { call_id, decision }],
        })
        .await?;
    let conversation = conversation_from(response)?;
    let since = conversation.items.last().map_or(0, item_seq);
    finish(client, conversation, since, no_wait, format).await
}

async fn finish(
    client: &mut IpcClient,
    conversation: AssistantConversationData,
    since: i64,
    no_wait: bool,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let conversation = if no_wait {
        conversation
    } else {
        wait(client, conversation).await?
    };
    // A text reply shows what this turn added; JSON gets everything.
    print_conversation(&conversation, since, format)
}

/// Poll until the turn settles or the wait runs out.
async fn wait(
    client: &mut IpcClient,
    mut conversation: AssistantConversationData,
) -> anyhow::Result<AssistantConversationData> {
    let deadline = Instant::now() + WAIT_AT_MOST;
    while conversation.status == AssistantStatusData::Running && Instant::now() < deadline {
        tokio::time::sleep(POLL_EVERY).await;
        let response = client
            .request(Request::GetAssistantConversation {
                conversation_id: conversation.id.clone(),
            })
            .await?;
        conversation = conversation_from(response)?;
    }
    Ok(conversation)
}

fn print_conversation(
    conversation: &AssistantConversationData,
    since: i64,
    format: OutputFormat,
) -> anyhow::Result<()> {
    if is_json(&format) {
        return print_json(conversation, &format);
    }
    for item in conversation
        .items
        .iter()
        .filter(|item| item_seq(item) > since)
    {
        match item {
            AssistantItemData::User { text, .. } => println!("you: {text}\n"),
            AssistantItemData::Summary { .. } => {
                println!("  (earlier messages were summarised to fit the model's context)\n");
            }
            AssistantItemData::Assistant {
                text,
                failed_call_ids,
                rows,
                ..
            } => {
                println!("{text}\n");
                for row in rows {
                    match &row.detail {
                        Some(detail) => println!("  - {}  {detail}", row.title),
                        None => println!("  - {}", row.title),
                    }
                }
                if !rows.is_empty() {
                    println!();
                }
                // Small models sometimes say they did what failed: name
                // the steps before the reply that did, from mxr's record.
                let failed: Vec<&str> = conversation
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        AssistantItemData::ToolCall {
                            call_id,
                            tool,
                            reason,
                            title,
                            ..
                        } if failed_call_ids.contains(call_id) => Some(match title {
                            Some(title) => title.done.as_str(),
                            None if reason.is_empty() => tool.as_str(),
                            None => reason.as_str(),
                        }),
                        _ => None,
                    })
                    .collect();
                if !failed.is_empty() {
                    println!(
                        "  ! failed, whatever the reply says: {}\n",
                        failed.join("; ")
                    );
                }
            }
            AssistantItemData::ToolCall {
                tool,
                reason,
                state,
                result,
                title,
                ..
            } => {
                // The daemon's words; the model's reason stays in `--format json`.
                let words = match title {
                    Some(title) if *state == AssistantCallStateData::Running => &title.running,
                    Some(title) => &title.done,
                    None if reason.is_empty() => tool,
                    None => reason,
                };
                println!("  · {words} [{tool}, {}]", state_label(*state));
                if let Some(id) = undo_id(*state, result.as_deref()) {
                    println!("    undo within a minute: mxr undo {id}");
                }
            }
        }
    }
    match conversation.status {
        AssistantStatusData::Running => println!(
            "Still working. Check back with: mxr assistant show {}",
            conversation.id
        ),
        AssistantStatusData::AwaitingUser => print_pending(conversation),
        AssistantStatusData::Idle => match conversation.last_outcome {
            Some(AssistantOutcomeData::Failed) => match (
                conversation.last_error.as_deref(),
                &conversation.last_error_detail,
            ) {
                // The daemon reworded a known failure; the raw text is in
                // `--format json`.
                (Some(sentence), Some(_)) => println!("{sentence}"),
                (raw, _) => println!("The turn failed: {}", raw.unwrap_or("unknown error")),
            },
            Some(AssistantOutcomeData::StepLimit) => println!(
                "Stopped after many steps. Continue with: mxr assistant send --conversation {} \"go on\"",
                conversation.id
            ),
            Some(AssistantOutcomeData::Cancelled) => println!("Stopped."),
            _ => {}
        },
    }
    Ok(())
}

fn print_pending(conversation: &AssistantConversationData) {
    let id = &conversation.id;
    for pending in &conversation.pending {
        match pending {
            AssistantPendingData::Approval {
                call_id,
                tool,
                arguments,
                reason,
                title,
                preview,
                preview_error,
            } => {
                match title {
                    Some(title) => println!("\nWaiting for your approval: {title}\n  {reason}"),
                    None => println!("\nWaiting for your approval: {reason}"),
                }
                println!("  {tool} {arguments}");
                if let Some(preview) = preview {
                    let messages = preview_messages(preview);
                    if messages.is_empty() {
                        println!("  preview: {}", truncate(&preview.to_string(), 600));
                    } else {
                        println!("  touches {} message(s):", messages.len());
                        for line in messages.iter().take(PREVIEW_LINES) {
                            println!("    - {line}");
                        }
                        if messages.len() > PREVIEW_LINES {
                            println!("    and {} more", messages.len() - PREVIEW_LINES);
                        }
                    }
                }
                if let Some(error) = preview_error {
                    println!("  the preview failed: {error}");
                }
                println!("  approve: mxr assistant approve {id} {call_id}");
                println!("  reject:  mxr assistant reject {id} {call_id} --reason \"...\"");
            }
            AssistantPendingData::Question {
                call_id, questions, ..
            } => {
                for question in questions {
                    println!("\n{}", question.question);
                    for option in &question.options {
                        match &option.description {
                            Some(description) => println!("  - {}: {description}", option.label),
                            None => println!("  - {}", option.label),
                        }
                    }
                }
                if questions.len() == 1 {
                    println!(
                        "  answer: mxr assistant answer {id} {call_id} --choose \"<label>\"{}",
                        if questions[0].allow_other {
                            " (or --other \"...\")"
                        } else {
                            ""
                        }
                    );
                } else {
                    println!(
                        "  answer: mxr assistant answer {id} {call_id} --answers-json '[{{\"selected\": [\"...\"]}}, ...]'"
                    );
                }
            }
        }
    }
}

fn state_label(state: AssistantCallStateData) -> &'static str {
    match state {
        AssistantCallStateData::Running => "running",
        AssistantCallStateData::Done => "done",
        AssistantCallStateData::Failed => "failed",
        AssistantCallStateData::Waiting => "waiting for you",
        AssistantCallStateData::Approved => "approved",
        AssistantCallStateData::AutoApproved => "ran without asking",
        AssistantCallStateData::Declined => "declined",
        AssistantCallStateData::Answered => "answered",
        AssistantCallStateData::Cancelled => "stopped",
        AssistantCallStateData::Interrupted => "interrupted",
    }
}

/// The id a mail change the assistant made can be undone with.
fn undo_id(state: AssistantCallStateData, result: Option<&str>) -> Option<String> {
    if !matches!(
        state,
        AssistantCallStateData::Approved | AssistantCallStateData::AutoApproved
    ) {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(result?).ok()?;
    value
        .pointer("/result/mutation_id")
        .or_else(|| value.get("mutation_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn item_seq(item: &AssistantItemData) -> i64 {
    match item {
        AssistantItemData::User { seq, .. }
        | AssistantItemData::Assistant { seq, .. }
        | AssistantItemData::Summary { seq, .. }
        | AssistantItemData::ToolCall { seq, .. } => *seq,
    }
}

fn conversation_from(response: Response) -> anyhow::Result<AssistantConversationData> {
    match ok(response)? {
        ResponseData::AssistantConversation { conversation } => Ok(conversation),
        _ => anyhow::bail!("unexpected response from the assistant"),
    }
}

fn ok(response: Response) -> anyhow::Result<ResponseData> {
    match response {
        Response::Ok { data } => Ok(data),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

fn is_json(format: &OutputFormat) -> bool {
    matches!(format, OutputFormat::Json | OutputFormat::Jsonl)
}

fn print_json<T: serde::Serialize>(value: &T, format: &OutputFormat) -> anyhow::Result<()> {
    if matches!(format, OutputFormat::Jsonl) {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn when(unix_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(unix_seconds, 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// "Subject (sender)" for each message a preview names: the first list of
/// objects with a subject, wherever the tool put it.
fn preview_messages(preview: &serde_json::Value) -> Vec<String> {
    fn find(value: &serde_json::Value, depth: u8) -> Option<&Vec<serde_json::Value>> {
        if depth > 4 {
            return None;
        }
        match value {
            serde_json::Value::Array(items)
                if !items.is_empty() && items.iter().all(|item| item.get("subject").is_some()) =>
            {
                Some(items)
            }
            serde_json::Value::Array(items) => items.iter().find_map(|item| find(item, depth + 1)),
            serde_json::Value::Object(map) => map.values().find_map(|item| find(item, depth + 1)),
            _ => None,
        }
    }
    find(preview, 0)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let subject = item["subject"].as_str().unwrap_or("(no subject)");
                    let from = &item["from"];
                    let sender = from["name"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .or_else(|| from["email"].as_str())
                        .unwrap_or("");
                    format!("{subject} ({sender})")
                })
                .collect()
        })
        .unwrap_or_default()
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push('…');
    cut
}
