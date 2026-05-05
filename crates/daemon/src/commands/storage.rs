#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::{OutputFormat, StorageGroupByArg};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::AccountId;
use mxr_core::types::{LargestMessageRow, StorageBucket, StorageGroupBy};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

fn header_for(group_by: StorageGroupBy) -> &'static str {
    match group_by {
        StorageGroupBy::Sender => "SENDER",
        StorageGroupBy::Mimetype => "MIMETYPE",
        StorageGroupBy::Label => "LABEL",
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut idx = 0;
    while value >= 1024.0 && idx + 1 < UNITS.len() {
        value /= 1024.0;
        idx += 1;
    }
    if idx == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[idx])
    }
}

fn render_table(group_by: StorageGroupBy, rows: &[StorageBucket]) {
    if rows.is_empty() {
        println!("No data.");
        return;
    }

    let key_label = header_for(group_by);
    println!("{:<48} {:>12} {:>10}", key_label, "SIZE", "COUNT");
    println!("{}", "-".repeat(72));
    for row in rows {
        let key: String = row.key.chars().take(48).collect();
        println!(
            "{:<48} {:>12} {:>10}",
            key,
            human_bytes(row.bytes),
            row.count,
        );
    }
    let total_bytes: u64 = rows.iter().map(|r| r.bytes).sum();
    let total_count: u32 = rows.iter().map(|r| r.count).sum();
    println!(
        "\n{} buckets — {} across {} items",
        rows.len(),
        human_bytes(total_bytes),
        total_count
    );
}

pub async fn run(
    by: StorageGroupByArg,
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
    let fmt = resolve_format(format);

    if let Some(group_by) = by.as_core() {
        let resp = client
            .request(Request::ListStorageBreakdown {
                account_id,
                group_by,
                limit,
            })
            .await?;
        let rows = crate::commands::expect_response(resp, |r| match r {
            Response::Ok {
                data: ResponseData::StorageBreakdown { rows },
            } => Some(rows),
            _ => None,
        })?;
        render_buckets(group_by, &rows, fmt)?;
    } else {
        // --by message: rank single messages by size_bytes.
        let resp = client
            .request(Request::ListLargestMessages {
                account_id,
                since_days: None,
                limit,
            })
            .await?;
        let rows = crate::commands::expect_response(resp, |r| match r {
            Response::Ok {
                data: ResponseData::LargestMessages { rows },
            } => Some(rows),
            _ => None,
        })?;
        render_largest_messages(&rows, fmt)?;
    }

    Ok(())
}

fn render_buckets(
    group_by: StorageGroupBy,
    rows: &[StorageBucket],
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(rows)?),
        OutputFormat::Csv => {
            println!("key,bytes,count");
            for row in rows {
                let key = row.key.replace('"', "\"\"");
                println!("\"{key}\",{},{}", row.bytes, row.count);
            }
        }
        OutputFormat::Ids => {
            for row in rows {
                println!("{}", row.key);
            }
        }
        OutputFormat::Table => render_table(group_by, rows),
    }
    Ok(())
}

fn render_largest_messages(rows: &[LargestMessageRow], fmt: OutputFormat) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(rows)?),
        OutputFormat::Csv => {
            println!("message_id,size_bytes,date,from_email,subject");
            for row in rows {
                let from = row.from_email.replace('"', "\"\"");
                let subject = row.subject.replace('"', "\"\"");
                println!(
                    "{},{},{},\"{from}\",\"{subject}\"",
                    row.message_id,
                    row.size_bytes,
                    row.date.to_rfc3339(),
                );
            }
        }
        OutputFormat::Ids => {
            // The point of `--by message --format ids`: pipe the message IDs
            // into `mxr trash` / `mxr archive` / `mxr cat` for direct action.
            for row in rows {
                println!("{}", row.message_id);
            }
        }
        OutputFormat::Table => render_largest_messages_table(rows),
    }
    Ok(())
}

fn render_largest_messages_table(rows: &[LargestMessageRow]) {
    if rows.is_empty() {
        println!("No messages.");
        return;
    }
    println!(
        "{:>10} {:<10} {:<32} {:<48}",
        "SIZE", "DATE", "FROM", "SUBJECT"
    );
    println!("{}", "-".repeat(102));
    for row in rows {
        let date = row.date.format("%Y-%m-%d");
        let from: String = row.from_email.chars().take(32).collect();
        let subject: String = row.subject.chars().take(48).collect();
        println!(
            "{:>10} {} {:<32} {:<48}",
            human_bytes(row.size_bytes),
            date,
            from,
            subject,
        );
    }
    let total: u64 = rows.iter().map(|r| r.size_bytes).sum();
    println!(
        "\n{} messages — {} total. Pipe `--format ids` into `mxr trash` / `mxr archive` to act.",
        rows.len(),
        human_bytes(total),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rows() -> Vec<StorageBucket> {
        vec![
            StorageBucket {
                key: "application/pdf".into(),
                bytes: 50_000,
                count: 1,
            },
            StorageBucket {
                key: "image/png".into(),
                bytes: 20_000,
                count: 2,
            },
        ]
    }

    #[test]
    fn human_bytes_scales_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn json_render_round_trips() {
        let rendered = serde_json::to_string(&sample_rows()).unwrap();
        assert!(rendered.contains("application/pdf"));
        assert!(rendered.contains("50000"));
    }

    #[test]
    fn table_render_handles_empty() {
        render_table(StorageGroupBy::Mimetype, &[]);
    }
}
