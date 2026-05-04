#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::AccountId;
use mxr_core::types::{ResponseTimeDirection, ResponseTimeSummary};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

fn fmt_duration_secs(seconds: u32) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

fn render_table(summary: &ResponseTimeSummary) {
    let direction = match summary.direction {
        ResponseTimeDirection::IReplied => "I replied to",
        ResponseTimeDirection::TheyReplied => "They replied to",
    };
    println!("Reply-latency summary: {direction}");
    println!("Sample size: {}", summary.sample_count);
    println!();
    println!("{:<20} {:>12} {:>12}", "", "P50", "P90");
    println!("{}", "-".repeat(46));
    println!(
        "{:<20} {:>12} {:>12}",
        "clock",
        fmt_duration_secs(summary.clock_p50_seconds),
        fmt_duration_secs(summary.clock_p90_seconds),
    );
    let bp50 = summary
        .business_hours_p50_seconds
        .map(fmt_duration_secs)
        .unwrap_or_else(|| "-".into());
    let bp90 = summary
        .business_hours_p90_seconds
        .map(fmt_duration_secs)
        .unwrap_or_else(|| "-".into());
    println!("{:<20} {:>12} {:>12}", "business-hours", bp50, bp90);
}

pub async fn run(
    theirs: bool,
    counterparty: Option<String>,
    since_days: Option<u32>,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let direction = if theirs {
        ResponseTimeDirection::TheyReplied
    } else {
        ResponseTimeDirection::IReplied
    };
    let account_id = account
        .as_deref()
        .map(AccountId::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid --account id: {e}"))?;

    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListResponseTime {
            account_id,
            direction,
            counterparty,
            since_days,
        })
        .await?;
    let summary = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::ResponseTime { summary },
        } => Some(summary),
        _ => None,
    })?;
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&summary)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&[summary])?),
        OutputFormat::Csv => {
            println!(
                "direction,sample_count,clock_p50,clock_p90,business_p50,business_p90"
            );
            println!(
                "{},{},{},{},{},{}",
                summary.direction.as_db_str(),
                summary.sample_count,
                summary.clock_p50_seconds,
                summary.clock_p90_seconds,
                summary
                    .business_hours_p50_seconds
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                summary
                    .business_hours_p90_seconds
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            );
        }
        OutputFormat::Ids => println!("{}", summary.direction.as_db_str()),
        OutputFormat::Table => render_table(&summary),
    }
    Ok(())
}
