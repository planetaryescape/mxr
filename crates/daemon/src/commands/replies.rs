//! `mxr replies` — manage the reply-later queue.
//!
//! The default invocation lists currently-flagged messages. Subcommands
//! mark and unmark individual messages by ID. Local-only state — flags
//! never roundtrip to the provider.

use crate::cli::{OutputFormat, RepliesAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::MessageId;
use mxr_core::Envelope;
use mxr_protocol::*;
use std::io::Write;

pub async fn run(
    action: Option<RepliesAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(RepliesAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        RepliesAction::List => {
            let messages = list_reply_queue(&mut client).await?;
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
        RepliesAction::Walk => walk_reply_queue(&mut client).await?,
        RepliesAction::Add { message_id } => {
            let id = parse_message_id(&message_id)?;
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
        Response::Error { message, .. } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn walk_reply_queue(client: &mut IpcClient) -> anyhow::Result<()> {
    let mut messages = list_reply_queue(client).await?;
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
        match input.trim().to_ascii_lowercase().as_str() {
            "r" | "reply" => {
                crate::commands::mutations::reply(
                    env.id.to_string(),
                    None,
                    false,
                    None,
                    false,
                    false,
                    false,
                    None,
                )
                .await?;
                clear_reply_later(client, &env.id).await?;
                messages.remove(index);
            }
            "c" | "clear" => {
                clear_reply_later(client, &env.id).await?;
                println!("Cleared reply-later flag");
                messages.remove(index);
            }
            "" | "s" | "skip" => {
                index += 1;
            }
            "q" | "quit" => {
                break;
            }
            other => {
                println!("Unknown action `{other}`");
            }
        }
    }

    Ok(())
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
        Response::Error { message, .. } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}
