//! `mxr sender <addr>` — relationship aggregates for one contact.

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    email: String,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;

    // Resolve the account id (CLI accepts the configured `key` or
    // falls back to the only/default account if not specified).
    let account_id = resolve_account(&mut client, account.as_deref()).await?;

    let resp = client
        .request(Request::GetSenderProfile {
            account_id,
            email: email.clone(),
        })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::SenderProfile { profile },
        } => match (fmt, profile) {
            (OutputFormat::Json, p) => {
                println!("{}", serde_json::to_string_pretty(&p)?);
            }
            (OutputFormat::Jsonl, p) => {
                println!("{}", serde_json::to_string(&p)?);
            }
            (_, None) => {
                println!("No history with {email}");
            }
            (_, Some(p)) => {
                let name = p.display_name.as_deref().unwrap_or(p.email.as_str());
                println!("{name} <{}>", p.email);
                if p.is_list_sender {
                    println!(
                        "  list-sender: {}",
                        p.list_id.as_deref().unwrap_or("(unidentified list)")
                    );
                }
                println!(
                    "  volume:   {} in, {} out",
                    p.total_inbound, p.total_outbound
                );
                println!("  replied:  {} times", p.replied_count);
                if let Some(cadence) = p.cadence_days_p50 {
                    println!("  cadence:  {:.1} days (p50)", cadence);
                }
                if let Some(last_in) = p.last_inbound_at {
                    println!(
                        "  last in:  {}",
                        last_in
                            .with_timezone(&chrono::Local)
                            .format("%a %b %e %H:%M")
                    );
                }
                if let Some(last_out) = p.last_outbound_at {
                    println!(
                        "  last out: {}",
                        last_out
                            .with_timezone(&chrono::Local)
                            .format("%a %b %e %H:%M")
                    );
                }
                if p.open_thread_count > 0 {
                    println!(
                        "  open:     {} thread(s) waiting on you",
                        p.open_thread_count
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }

    Ok(())
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
