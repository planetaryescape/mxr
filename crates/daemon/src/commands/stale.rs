#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::AccountId;
use mxr_core::types::{StaleBallInCourt, StaleThreadRow};
use mxr_protocol::{Request, Response, ResponseData};
use std::str::FromStr;

fn render_table(rows: &[StaleThreadRow], perspective: StaleBallInCourt) {
    if rows.is_empty() {
        match perspective {
            StaleBallInCourt::Mine => println!("Nothing waiting on you."),
            StaleBallInCourt::Theirs => println!("Nothing waiting on them."),
        }
        return;
    }
    println!(
        "{:<32} {:>5} {:<48}",
        "COUNTERPARTY", "DAYS", "LATEST SUBJECT"
    );
    println!("{}", "-".repeat(88));
    for row in rows {
        let subject: String = row.latest_subject.chars().take(48).collect();
        let cp: String = row.counterparty_email.chars().take(32).collect();
        println!("{:<32} {:>5} {:<48}", cp, row.days_stale, subject);
    }
    println!("\n{} stale threads", rows.len());
}

pub async fn run(
    mine: bool,
    theirs: bool,
    older_than_days: u32,
    limit: u32,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    // Default to --mine when neither flag set; the more common ask.
    let perspective = if theirs {
        StaleBallInCourt::Theirs
    } else {
        let _ = mine;
        StaleBallInCourt::Mine
    };

    let account_id = account
        .as_deref()
        .map(AccountId::from_str)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid --account id: {e}"))?;

    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::ListStaleThreads {
            account_id,
            perspective,
            older_than_days,
            limit,
        })
        .await?;
    let fmt = resolve_format(format);
    let rows = crate::commands::expect_response(resp, |r| match r {
        Response::Ok {
            data: ResponseData::StaleThreads { rows },
        } => Some(rows),
        _ => None,
    })?;

    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
        OutputFormat::Jsonl => println!("{}", jsonl(&rows)?),
        OutputFormat::Csv => {
            println!("thread_id,counterparty_email,days_stale,latest_subject,latest_date");
            for row in &rows {
                let subject = row.latest_subject.replace('"', "\"\"");
                println!(
                    "{},{},{},\"{}\",{}",
                    row.thread_id,
                    row.counterparty_email,
                    row.days_stale,
                    subject,
                    row.latest_date.to_rfc3339(),
                );
            }
        }
        OutputFormat::Ids => {
            for row in &rows {
                println!("{}", row.thread_id);
            }
        }
        OutputFormat::Table => render_table(&rows, perspective),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{MessageId, ThreadId};

    fn sample() -> StaleThreadRow {
        StaleThreadRow {
            thread_id: ThreadId::new(),
            latest_message_id: MessageId::new(),
            latest_subject: "Project status".into(),
            counterparty_email: "alice@example.com".into(),
            latest_date: Utc::now(),
            days_stale: 30,
        }
    }

    #[test]
    fn json_render_round_trips() {
        let rendered = serde_json::to_string(&vec![sample()]).unwrap();
        assert!(rendered.contains("alice@example.com"));
    }

    #[test]
    fn table_render_handles_empty() {
        render_table(&[], StaleBallInCourt::Mine);
        render_table(&[], StaleBallInCourt::Theirs);
    }
}
