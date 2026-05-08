//! `mxr replies` — manage the reply-later queue.
//!
//! The default invocation lists currently-flagged messages. Subcommands
//! mark and unmark individual messages by ID. Local-only state — flags
//! never roundtrip to the provider.

use crate::cli::{OutputFormat, RepliesAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::MessageId;
use mxr_protocol::*;

pub async fn run(
    action: Option<RepliesAction>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(RepliesAction::List);
    let mut client = IpcClient::connect().await?;

    match action {
        RepliesAction::List => {
            let resp = client.request(Request::ListReplyQueue).await?;
            let fmt = resolve_format(format);
            match resp {
                Response::Ok {
                    data: ResponseData::ReplyQueue { messages },
                } => match fmt {
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
                                let from = env.from.name.as_deref().unwrap_or(&env.from.email);
                                println!("  {}  {}  {}", env.id.as_str(), from, env.subject);
                            }
                        }
                    }
                },
                Response::Error { message, .. } => anyhow::bail!("{}", message),
                _ => anyhow::bail!("Unexpected response"),
            }
        }
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
            println!("{success_message}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }
}
