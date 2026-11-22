//! `mxr status --freshness`: is the local copy of my mail current? When
//! the newest message arrived, each account's sync health, and the last
//! arrivals with the modes they went to.

use crate::cli::OutputFormat;
use crate::commands::{expect_response, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{resolve_format, terminal_text};
use chrono::{DateTime, Local, Utc};
use mxr_protocol::freshness_copy::{latest_mail, sync_line};
use mxr_protocol::{ArrivalData, FreshnessData, Request, Response, ResponseData};
use std::fmt::Write as _;

pub async fn run(
    account: Option<&str>,
    limit: Option<u32>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account).await?;
    let freshness = expect_response(
        client
            .request(Request::GetFreshness { account_id, limit })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::Freshness { freshness },
            } => Some(freshness),
            _ => None,
        },
    )?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&freshness)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&freshness)?),
        _ => print!("{}", render_text(&freshness, Utc::now(), &Local)),
    }
    Ok(())
}

fn arrival_line<Tz: chrono::TimeZone>(arrival: &ArrivalData, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let sender = arrival
        .from
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&arrival.from.email);
    let went_to = match arrival.modes.first() {
        Some(mode) => match mode.tag.as_deref() {
            Some(tag) => format!("→ {} · {tag}", mode.name),
            None => format!("→ {}", mode.name),
        },
        None if arrival.in_inbox => "→ Inbox".to_string(),
        None => "→ not in the inbox".to_string(),
    };
    format!(
        "{}  {}  {}  {went_to}",
        arrival.received_at.with_timezone(tz).format("%H:%M"),
        terminal_text(sender),
        terminal_text(&arrival.subject),
    )
}

fn clock<Tz: chrono::TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.with_timezone(tz).format("%H:%M").to_string()
}

fn render_text<Tz: chrono::TimeZone>(data: &FreshnessData, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::new();
    let _ = write!(out, "{}", latest_mail(data.newest_message_at, now));
    if let Some(at) = data.newest_message_at {
        let _ = write!(out, " ({})", clock(at, tz));
    }
    out.push('\n');
    if let Some(worst) = data.worst_account(now) {
        let _ = writeln!(
            out,
            "Sync: {}",
            sync_line(worst, data.stale_after_secs, now, tz)
        );
    }
    if data.accounts.len() > 1 {
        out.push_str("Accounts:\n");
        for account in &data.accounts {
            let _ = writeln!(
                out,
                "  {}: {}, {}",
                terminal_text(&account.account_name),
                latest_mail(account.newest_message_at, now),
                sync_line(account, data.stale_after_secs, now, tz)
            );
        }
    }
    for account in &data.accounts {
        if let Some(error) = account.last_sync_error.as_ref() {
            let _ = writeln!(
                out,
                "Last error ({}): {}",
                terminal_text(&account.account_name),
                terminal_text(&error.message)
            );
        }
    }
    if !data.arrivals.is_empty() {
        out.push_str("Last arrivals:\n");
        for arrival in &data.arrivals {
            let _ = writeln!(out, "  {}", arrival_line(arrival, tz));
        }
    }
    out
}
