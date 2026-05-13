use crate::cli::{CadenceAction, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(action: CadenceAction) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match action {
        CadenceAction::Watch {
            email,
            account,
            expected_days,
            note,
            allow_list_sender,
        } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::WatchCadence {
                    account_id,
                    email,
                    expected_days,
                    note,
                    allow_list_sender,
                })
                .await?;
            ack(resp, "watching")
        }
        CadenceAction::Unwatch { email, account } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::UnwatchCadence { account_id, email })
                .await?;
            ack(resp, "unwatched")
        }
        CadenceAction::List { account, format } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCadenceWatch { account_id })
                .await?;
            print_list(resp, resolve_format(format))
        }
        CadenceAction::Drift { account, format } => {
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::ListCadenceDrift { account_id })
                .await?;
            print_drift(resp, resolve_format(format))
        }
    }
}

fn ack(resp: Response, label: &str) -> anyhow::Result<()> {
    match resp {
        Response::Ok { .. } => {
            println!("{label}");
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

fn print_list(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CadenceWatchList { entries },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
            OutputFormat::Jsonl => {
                for e in entries {
                    println!("{}", serde_json::to_string(&e)?);
                }
            }
            _ => {
                for e in entries {
                    let exp = e
                        .expected_days
                        .map(|d| format!("{d:.1}d"))
                        .unwrap_or_else(|| "auto".into());
                    println!("{:<32}  expected={}  added={}", e.email, exp, e.added_at.format("%Y-%m-%d"));
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn print_drift(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::CadenceDriftList { rows },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
            OutputFormat::Jsonl => {
                for r in rows {
                    println!("{}", serde_json::to_string(&r)?);
                }
            }
            _ => {
                println!("{:<32}  {:>8}  {:>8}", "email", "drift", "expected");
                for r in rows {
                    println!(
                        "{:<32}  {:>6.1}d  {:>6.1}d",
                        r.email, r.drift_days, r.expected_days
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
