use crate::cli::{BriefingAction, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;
use std::str::FromStr;

pub async fn run(action: BriefingAction) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        BriefingAction::Thread {
            thread_id,
            refresh,
            format,
        } => {
            let id = mxr_core::ThreadId::from_str(&thread_id)
                .map_err(|e| anyhow::anyhow!("invalid thread id: {e}"))?;
            let resp = client
                .request(Request::GetThreadBriefing {
                    thread_id: id,
                    refresh,
                })
                .await?;
            print(resp, resolve_format(format), true)
        }
        BriefingAction::Recipient {
            email,
            account,
            refresh,
            format,
        } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::GetRecipientBriefing {
                    account_id,
                    email,
                    refresh,
                })
                .await?;
            print(resp, resolve_format(format), false)
        }
    }
}

fn print(resp: Response, fmt: OutputFormat, is_thread: bool) -> anyhow::Result<()> {
    let briefing = match resp {
        Response::Ok {
            data: ResponseData::ThreadBriefing { briefing },
        } if is_thread => briefing,
        Response::Ok {
            data: ResponseData::RecipientBriefing { briefing },
        } => briefing,
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    };
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&briefing)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&briefing)?),
        _ => {
            println!("{}", briefing.body_markdown);
            if !briefing.citations.is_empty() {
                println!();
                println!("Citations:");
                for c in &briefing.citations {
                    let mid = c.message_id.as_deref().unwrap_or("?");
                    println!("  - msg={} \"{}\"", mid, c.quote);
                }
            }
            println!(
                "\n(generated {} {})",
                briefing.generated_at.format("%Y-%m-%d %H:%M"),
                if briefing.from_cache {
                    "[cached]"
                } else {
                    "[fresh]"
                }
            );
        }
    }
    Ok(())
}
