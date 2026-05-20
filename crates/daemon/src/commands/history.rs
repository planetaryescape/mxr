#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{EventLogEntry, Request, Response, ResponseData};

fn render_table(entries: &[EventLogEntry]) {
    if entries.is_empty() {
        println!("No history entries.");
        return;
    }

    println!(
        "{:<19} {:<5} {:<10} {:<44} {:<36}",
        "TIME", "LEVEL", "CATEGORY", "SUMMARY", "MESSAGE ID"
    );
    println!("{}", "-".repeat(122));
    for entry in entries {
        let timestamp = chrono::DateTime::from_timestamp(entry.timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| entry.timestamp.to_string());
        let message_id = entry.message_id.as_deref().unwrap_or("-");
        let summary: String = entry.summary.chars().take(44).collect();
        println!(
            "{:<19} {:<5} {:<10} {:<44} {:<36}",
            timestamp, entry.level, entry.category, summary, message_id
        );
    }
    println!("\n{} entries", entries.len());
}

pub struct HistoryRunOptions {
    pub category: Option<String>,
    pub category_prefix: Option<String>,
    pub level: Option<String>,
    pub search: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub offset: u32,
    pub limit: u32,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: HistoryRunOptions) -> anyhow::Result<()> {
    let HistoryRunOptions {
        category,
        category_prefix,
        level,
        search,
        since,
        until,
        offset,
        limit,
        format,
    } = options;
    let since_ts = since.as_deref().map(parse_history_time).transpose()?;
    let until_ts = until.as_deref().map(parse_history_time).transpose()?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListEvents {
            limit,
            level,
            category,
            category_prefix,
            since: since_ts,
            until: until_ts,
            search,
            offset,
        })
        .await?;

    let fmt = resolve_format(format);
    let entries = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::EventLogEntries { entries },
        } => Some(entries),
        _ => None,
    })?;
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&entries)?),
        _ => render_table(&entries),
    }

    Ok(())
}

/// Accept either a relative duration (`1h`, `3d`, `2w`) interpreted as
/// "elapsed time before now" or an ISO date/datetime. Returns unix
/// seconds (matching the `event_log.timestamp` column). Returns an error
/// for unparseable input — never silently passes garbage through.
pub fn parse_history_time(s: &str) -> anyhow::Result<i64> {
    if let Some(elapsed_secs) = parse_duration_secs(s) {
        return Ok(chrono::Utc::now().timestamp() - elapsed_secs);
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date
            .and_hms_opt(0, 0, 0)
            .map(|dt| dt.and_utc().timestamp())
            .unwrap_or(0));
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp());
    }
    anyhow::bail!(
        "could not parse time '{s}'. Try `1h`, `3d`, `2w`, `2026-05-01`, or `2026-05-01T09:00:00Z`."
    )
}

fn parse_duration_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let n: i64 = num_part.parse().ok()?;
    match unit {
        "s" => Some(n),
        "m" => Some(n * 60),
        "h" => Some(n * 3_600),
        "d" => Some(n * 86_400),
        "w" => Some(n * 7 * 86_400),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::AccountId;

    fn sample_entry(summary: &str) -> EventLogEntry {
        EventLogEntry {
            timestamp: 1_710_000_000,
            level: "info".into(),
            category: "mutation".into(),
            account_id: Some(AccountId::new()),
            message_id: Some("msg-1".into()),
            rule_id: None,
            summary: summary.into(),
            details: None,
        }
    }

    #[test]
    fn table_render_handles_empty() {
        render_table(&[]);
    }

    #[test]
    fn json_render_round_trips() {
        let rendered = serde_json::to_string(&vec![sample_entry("Archived message")]).unwrap();
        assert!(rendered.contains("Archived message"));
        assert!(rendered.contains("mutation"));
    }
}
