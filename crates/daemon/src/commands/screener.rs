//! `mxr screener` — manage per-sender consent decisions.

use crate::cli::{OutputFormat, ScreenerAction};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::*;

pub async fn run(
    action: Option<ScreenerAction>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let action = action.unwrap_or(ScreenerAction::Queue { limit: 100 });
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;

    match action {
        ScreenerAction::Queue { limit } => {
            let resp = client
                .request(Request::ListScreenerQueue { account_id, limit })
                .await?;
            render_queue(resp, format)?;
        }
        ScreenerAction::List => {
            let resp = client
                .request(Request::ListScreenerDecisions { account_id })
                .await?;
            render_decisions(resp, format)?;
        }
        ScreenerAction::Allow {
            sender_email,
            label,
        } => {
            set(
                &mut client,
                account_id,
                sender_email,
                ScreenerDispositionData::Allow,
                label,
            )
            .await?
        }
        ScreenerAction::Deny {
            sender_email,
            label,
        } => {
            set(
                &mut client,
                account_id,
                sender_email,
                ScreenerDispositionData::Deny,
                label,
            )
            .await?
        }
        ScreenerAction::Feed {
            sender_email,
            label,
        } => {
            set(
                &mut client,
                account_id,
                sender_email,
                ScreenerDispositionData::Feed,
                label,
            )
            .await?
        }
        ScreenerAction::PaperTrail {
            sender_email,
            label,
        } => {
            set(
                &mut client,
                account_id,
                sender_email,
                ScreenerDispositionData::PaperTrail,
                label,
            )
            .await?
        }
        ScreenerAction::Clear { sender_email } => {
            let resp = client
                .request(Request::ClearScreenerDecision {
                    account_id,
                    sender_email: sender_email.clone(),
                })
                .await?;
            ack_or_bail(resp, &format!("Cleared decision for {sender_email}"))?;
        }
    }

    Ok(())
}

async fn set(
    client: &mut IpcClient,
    account_id: mxr_core::AccountId,
    sender_email: String,
    disposition: ScreenerDispositionData,
    route_label: Option<String>,
) -> anyhow::Result<()> {
    let resp = client
        .request(Request::SetScreenerDecision {
            account_id,
            sender_email: sender_email.clone(),
            disposition,
            route_label,
        })
        .await?;
    let label = match disposition {
        ScreenerDispositionData::Allow => "allow",
        ScreenerDispositionData::Deny => "deny",
        ScreenerDispositionData::Feed => "feed",
        ScreenerDispositionData::PaperTrail => "paper_trail",
        ScreenerDispositionData::Unknown => "unknown",
    };
    ack_or_bail(resp, &format!("Set {sender_email} → {label}"))
}

fn render_queue(resp: Response, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::ScreenerQueue { entries },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
            OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
            _ => {
                if entries.is_empty() {
                    println!("Screener queue is empty — every sender has a decision");
                } else {
                    for e in &entries {
                        let name = e.display_name.as_deref().unwrap_or(e.sender_email.as_str());
                        println!(
                            "  {name} <{}>  ({} msg) — {}",
                            e.sender_email, e.message_count, e.latest_subject
                        );
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn render_decisions(resp: Response, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::ScreenerDecisions { decisions },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&decisions)?),
            OutputFormat::Jsonl => println!("{}", jsonl(&decisions)?),
            _ => {
                if decisions.is_empty() {
                    println!("No screener decisions yet");
                } else {
                    for d in &decisions {
                        let label_suffix = d
                            .route_label
                            .as_deref()
                            .map(|l| format!(" → label `{l}`"))
                            .unwrap_or_default();
                        let disp = match d.disposition {
                            ScreenerDispositionData::Allow => "allow",
                            ScreenerDispositionData::Deny => "deny",
                            ScreenerDispositionData::Feed => "feed",
                            ScreenerDispositionData::PaperTrail => "paper_trail",
                            ScreenerDispositionData::Unknown => "unknown",
                        };
                        println!("  {}: {disp}{label_suffix}", d.sender_email);
                    }
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn ack_or_bail(resp: Response, success: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            println!("{success}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn resolve_account(
    client: &mut IpcClient,
    explicit: Option<&str>,
) -> anyhow::Result<mxr_core::AccountId> {
    let resp = client.request(Request::ListAccounts).await?;
    let accounts = match resp {
        Response::Ok {
            data: ResponseData::Accounts { accounts },
        } => accounts,
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    };
    if let Some(key) = explicit {
        return accounts
            .into_iter()
            .find(|a| a.key.as_deref() == Some(key) || a.email == key)
            .map(|a| a.account_id)
            .ok_or_else(|| anyhow::anyhow!("No account matching '{key}'"));
    }
    if accounts.len() == 1 {
        return Ok(accounts.into_iter().next().unwrap().account_id);
    }
    if let Some(default) = accounts.iter().find(|a| a.is_default) {
        return Ok(default.account_id.clone());
    }
    anyhow::bail!("Multiple accounts configured; pass --account <key>")
}
