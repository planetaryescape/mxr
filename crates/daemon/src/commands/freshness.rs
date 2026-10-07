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

/// The arrival's line: "09:37  Dana Lee  Lunch?  → Messages · person".
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::Address;
    use mxr_protocol::{
        AccountFreshnessData, ModeKindData, ModeMembershipData, SyncErrorData, SyncErrorKindData,
        SyncHealthData,
    };

    #[test]
    fn text_names_the_newest_mail_the_sync_state_and_where_arrivals_went() {
        let now = Utc.with_ymd_and_hms(2026, 10, 7, 9, 42, 0).unwrap();
        let account_id = AccountId::new();
        let data = FreshnessData {
            generated_at: now,
            newest_message_at: Some(now - Duration::minutes(5)),
            stale_after_secs: 900,
            worst_account_id: Some(account_id.clone()),
            accounts: vec![AccountFreshnessData {
                account_id: account_id.clone(),
                account_name: "work".into(),
                label: "Gmail".into(),
                newest_message_at: Some(now - Duration::minutes(5)),
                last_sync_ok_at: Some(now - Duration::minutes(3)),
                last_sync_attempt_at: Some(now),
                sync_in_progress: false,
                health: SyncHealthData::Paused,
                last_sync_error: Some(SyncErrorData {
                    kind: SyncErrorKindData::RateLimited,
                    message: "Rate limited, retry after 480s".into(),
                    retry_at: Some(now + Duration::minutes(8)),
                    consecutive_failures: 1,
                }),
            }],
            arrivals: vec![ArrivalData {
                account_id,
                message_id: MessageId::new(),
                thread_id: ThreadId::new(),
                from: Address {
                    name: Some("GitHub".into()),
                    email: "notifications@github.com".into(),
                },
                subject: "Build passed".into(),
                received_at: now - Duration::minutes(5),
                in_inbox: true,
                modes: vec![ModeMembershipData {
                    mode: ModeKindData::Updates,
                    name: "Updates".into(),
                    key: "g u".into(),
                    reason: "Here because: automated sender (rule).".into(),
                    also_in: String::new(),
                    early: true,
                    todo_ids: vec![],
                    tag: Some("automated".into()),
                }],
            }],
        };
        let text = render_text(&data, now, &Utc);
        assert!(text.starts_with("Latest mail 5m ago (09:37)\n"), "{text}");
        assert!(
            text.contains("Sync: Gmail paused: rate limited, retrying 09:50"),
            "{text}"
        );
        assert!(
            text.contains("09:37  GitHub  Build passed  → Updates · automated"),
            "{text}"
        );
        assert!(!text.contains('\u{2014}'), "no em dashes: {text}");
    }
}
