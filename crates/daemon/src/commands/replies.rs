//! `mxr replies` — manage the reply-later queue.
//!
//! The default invocation lists currently-flagged messages. Subcommands
//! mark and unmark individual messages by ID. Local-only state — flags
//! never roundtrip to the provider.

use crate::cli::{OutputFormat, RepliesAction};
use crate::commands::daemon_response_error;
use crate::commands::{ensure_message_account, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::MessageId;
use mxr_core::Envelope;
use mxr_protocol::*;
use std::io::Write;

pub async fn run(
    action: Option<RepliesAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(RepliesAction::List);
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;

    match action {
        RepliesAction::List => {
            let messages =
                filter_reply_queue(list_reply_queue(&mut client).await?, account_id.as_ref());
            let fmt = resolve_format(format);
            match fmt {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&messages)?);
                }
                OutputFormat::Jsonl => {
                    println!("{}", jsonl(&messages)?);
                }
                _ => {
                    if messages.is_empty() {
                        println!("Reply-later queue is empty");
                    } else {
                        for env in &messages {
                            print_reply_queue_row(env);
                        }
                    }
                }
            }
        }
        RepliesAction::Walk => walk_reply_queue(&mut client, account_id.as_ref()).await?,
        RepliesAction::Add { message_id } => {
            let id = parse_message_id(&message_id)?;
            ensure_message_account(&mut client, &id, account_id.as_ref()).await?;
            let resp = client
                .request(Request::SetReplyLater {
                    message_id: id,
                    flag: true,
                })
                .await?;
            ack_or_bail(resp, "Marked for reply later")?;
        }
        RepliesAction::Remove { message_id } => {
            let id = parse_message_id(&message_id)?;
            ensure_message_account(&mut client, &id, account_id.as_ref()).await?;
            let resp = client
                .request(Request::SetReplyLater {
                    message_id: id,
                    flag: false,
                })
                .await?;
            ack_or_bail(resp, "Cleared reply-later flag")?;
        }
    }

    Ok(())
}

async fn list_reply_queue(client: &mut IpcClient) -> anyhow::Result<Vec<Envelope>> {
    let resp = client.request(Request::ListReplyQueue).await?;
    match resp {
        Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        } => Ok(messages),
        Response::Error { message, kind, .. } => Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn walk_reply_queue(
    client: &mut IpcClient,
    account_id: Option<&mxr_core::AccountId>,
) -> anyhow::Result<()> {
    let mut messages = filter_reply_queue(list_reply_queue(client).await?, account_id);
    if messages.is_empty() {
        println!("Reply-later queue is empty");
        return Ok(());
    }

    let mut index = 0;
    while index < messages.len() {
        let env = messages[index].clone();
        println!();
        println!("[{}/{}] {}", index + 1, messages.len(), env.subject.trim());
        print_reply_queue_row(&env);
        if !env.snippet.trim().is_empty() {
            println!("  {}", env.snippet.trim());
        }
        println!("Actions: [r]eply  [c]lear  [s]kip  [q]uit");
        print!("reply> ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        match parse_walk_action(input.trim()) {
            WalkAction::Reply => {
                crate::commands::mutations::reply(crate::commands::mutations::ReplyCommand {
                    message_id: env.id.to_string(),
                    body: None,
                    body_stdin: false,
                    attach: Vec::new(),
                    signature: None,
                    no_signature: false,
                    yes: false,
                    draft: false,
                    dry_run: false,
                    remind_after: None,
                    format: None,
                    account: None,
                    from: None,
                })
                .await?;
                clear_reply_later(client, &env.id).await?;
                apply_walk_action(&mut messages, &mut index, WalkAction::Reply);
            }
            WalkAction::Clear => {
                clear_reply_later(client, &env.id).await?;
                println!("Cleared reply-later flag");
                apply_walk_action(&mut messages, &mut index, WalkAction::Clear);
            }
            WalkAction::Skip => apply_walk_action(&mut messages, &mut index, WalkAction::Skip),
            WalkAction::Quit => break,
            WalkAction::Unknown(other) => {
                println!("Unknown action `{other}`");
            }
        }
    }

    Ok(())
}

fn filter_reply_queue(
    messages: Vec<Envelope>,
    account_id: Option<&mxr_core::AccountId>,
) -> Vec<Envelope> {
    messages
        .into_iter()
        .filter(|message| account_id.is_none_or(|account_id| message.account_id == *account_id))
        .collect()
}

/// Decoded user input for an iteration of `mxr replies walk`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WalkAction {
    Reply,
    Clear,
    Skip,
    Quit,
    Unknown(String),
}

pub(crate) fn parse_walk_action(input: &str) -> WalkAction {
    match input.to_ascii_lowercase().as_str() {
        "r" | "reply" => WalkAction::Reply,
        "c" | "clear" => WalkAction::Clear,
        "" | "s" | "skip" => WalkAction::Skip,
        "q" | "quit" => WalkAction::Quit,
        other => WalkAction::Unknown(other.to_string()),
    }
}

/// Apply one walk-mode action to the local queue snapshot.
///
/// `Reply` and `Clear` remove the current entry — the daemon-side
/// flag clear is the source of truth for the queue, but the local
/// snapshot must reflect it so the user keeps walking forward instead
/// of re-prompting on the same message. `Skip` leaves the entry but
/// advances the cursor so the user can come back to it later (the
/// flag stays set in the store). `Quit` and `Unknown` are no-ops on
/// the queue.
pub(crate) fn apply_walk_action(queue: &mut Vec<Envelope>, index: &mut usize, action: WalkAction) {
    if *index >= queue.len() {
        return;
    }
    match action {
        WalkAction::Reply | WalkAction::Clear => {
            queue.remove(*index);
        }
        WalkAction::Skip => {
            *index += 1;
        }
        WalkAction::Quit | WalkAction::Unknown(_) => {}
    }
}

async fn clear_reply_later(client: &mut IpcClient, message_id: &MessageId) -> anyhow::Result<()> {
    let resp = client
        .request(Request::SetReplyLater {
            message_id: message_id.clone(),
            flag: false,
        })
        .await?;
    ack_or_bail(resp, "")
}

fn print_reply_queue_row(env: &Envelope) {
    let from = env.from.name.as_deref().unwrap_or(&env.from.email);
    println!("  {}  {}  {}", env.id.as_str(), from, env.subject);
}

fn parse_message_id(input: &str) -> anyhow::Result<MessageId> {
    input
        .parse::<MessageId>()
        .map_err(|e| anyhow::anyhow!("invalid message id `{input}`: {e}"))
}

fn ack_or_bail(resp: Response, success_message: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            if !success_message.is_empty() {
                println!("{success_message}");
            }
            Ok(())
        }
        Response::Error { message, kind, .. } => Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response"),
    }
}
