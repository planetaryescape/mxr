use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::mxr_protocol::{EventLogEntry, Request, Response, ResponseData};
use crate::output::resolve_format;

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

pub async fn run(
    category: Option<String>,
    level: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListEvents {
            limit,
            level,
            category,
        })
        .await?;

    let fmt = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::EventLogEntries { entries },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&entries)?),
            _ => render_table(&entries),
        },
        Response::Error { message } => anyhow::bail!("{}", message),
        _ => anyhow::bail!("Unexpected response"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_core::id::AccountId;

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
