#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::{ContactsAction, OutputFormat};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::AccountId;
use mxr_core::types::{ContactAsymmetryRow, ContactDecayRow};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

pub async fn run(action: ContactsAction, format: Option<OutputFormat>) -> anyhow::Result<()> {
    match action {
        ContactsAction::Asymmetry {
            min_inbound,
            limit,
            account,
        } => asymmetry(min_inbound, limit, account, format).await,
        ContactsAction::Decay {
            threshold_days,
            limit,
            account,
        } => decay(threshold_days, limit, account, format).await,
        ContactsAction::Refresh => refresh().await,
    }
}

async fn refresh() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    match client.request(Request::RefreshContacts).await? {
        Response::Ok {
            data: ResponseData::RefreshedContacts { rows },
        } => {
            println!("Refreshed {rows} contact rows.");
            Ok(())
        }
        Response::Error { message } => anyhow::bail!(message),
        other => anyhow::bail!("Unexpected response: {other:?}"),
    }
}

async fn asymmetry(
    min_inbound: u32,
    limit: u32,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let account_id = account
        .as_deref()
        .map(AccountId::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid --account id: {e}"))?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListContactAsymmetry {
            account_id,
            min_inbound,
            limit,
        })
        .await?;
    let rows = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ContactAsymmetry { rows },
        } => Some(rows),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&rows)?),
        OutputFormat::Csv => {
            println!("email,inbound,outbound,asymmetry,last_seen_at");
            for row in &rows {
                let email = row.email.replace('"', "\"\"");
                println!(
                    "\"{email}\",{},{},{:.3},{}",
                    row.total_inbound,
                    row.total_outbound,
                    row.asymmetry,
                    row.last_seen_at.to_rfc3339(),
                );
            }
        }
        OutputFormat::Ids => {
            for row in &rows {
                println!("{}", row.email);
            }
        }
        OutputFormat::Table => render_asymmetry_table(&rows),
    }
    Ok(())
}

fn render_asymmetry_table(rows: &[ContactAsymmetryRow]) {
    if rows.is_empty() {
        println!("No contacts above the inbound threshold.");
        return;
    }
    println!(
        "{:<40} {:>5} {:>5} {:>7}",
        "EMAIL", "IN", "OUT", "GAP"
    );
    println!("{}", "-".repeat(60));
    for row in rows {
        let email: String = row.email.chars().take(40).collect();
        println!(
            "{:<40} {:>5} {:>5} {:>6.0}%",
            email,
            row.total_inbound,
            row.total_outbound,
            row.asymmetry * 100.0
        );
    }
    println!("\n{} contacts", rows.len());
}

async fn decay(
    threshold_days: u32,
    limit: u32,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let account_id = account
        .as_deref()
        .map(AccountId::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid --account id: {e}"))?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListContactDecay {
            account_id,
            threshold_days,
            limit,
        })
        .await?;
    let rows = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ContactDecay { rows },
        } => Some(rows),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&rows)?),
        OutputFormat::Csv => {
            println!("email,days_since_inbound,days_since_outbound,last_inbound_at,last_outbound_at");
            for row in &rows {
                let email = row.email.replace('"', "\"\"");
                let outbound = row
                    .last_outbound_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default();
                let dso = row
                    .days_since_outbound
                    .map(|d| d.to_string())
                    .unwrap_or_default();
                println!(
                    "\"{email}\",{},{},{},{}",
                    row.days_since_inbound,
                    dso,
                    row.last_inbound_at.to_rfc3339(),
                    outbound,
                );
            }
        }
        OutputFormat::Ids => {
            for row in &rows {
                println!("{}", row.email);
            }
        }
        OutputFormat::Table => render_decay_table(&rows),
    }
    Ok(())
}

fn render_decay_table(rows: &[ContactDecayRow]) {
    if rows.is_empty() {
        println!("No decaying contacts.");
        return;
    }
    println!("{:<40} {:>10} {:>10}", "EMAIL", "INBOUND", "OUTBOUND");
    println!("{}", "-".repeat(63));
    for row in rows {
        let email: String = row.email.chars().take(40).collect();
        let outbound = row
            .days_since_outbound
            .map(|d| format!("{d}d"))
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<40} {:>9}d {:>10}",
            email,
            row.days_since_inbound,
            outbound,
        );
    }
    println!("\n{} contacts going cold", rows.len());
}
