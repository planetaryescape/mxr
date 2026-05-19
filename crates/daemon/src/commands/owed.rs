use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub async fn run(
    account: Option<String>,
    older_than_days: Option<u32>,
    within_days: Option<u32>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;
    let resp = client
        .request(Request::ListOwedReplies {
            account_id,
            older_than_days,
            within_days,
            limit,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::OwedReplies { rows },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&rows)?),
            OutputFormat::Jsonl => {
                for row in rows {
                    println!("{}", serde_json::to_string(&row)?);
                }
            }
            OutputFormat::Ids => {
                for row in rows {
                    println!("{}", row.thread_id);
                }
            }
            OutputFormat::Csv => {
                println!(
                    "thread_id,latest_inbound_msg_id,from_email,subject,latest_inbound_at,waiting_days,expected_days,overdue_score"
                );
                for row in rows {
                    println!(
                        "{},{},{},{},{},{:.2},{:.2},{:.2}",
                        row.thread_id,
                        row.latest_inbound_msg_id,
                        csv_escape(&row.from_email),
                        csv_escape(&row.subject),
                        row.latest_inbound_at.to_rfc3339(),
                        row.waiting_days,
                        row.expected_days,
                        row.overdue_score,
                    );
                }
            }
            OutputFormat::Table => {
                println!(
                    "{:>5}  {:<28}  {:>6}  {:>6}  {:>6}  subject",
                    "score", "from", "wait", "exp", "days"
                );
                for row in rows {
                    let from = truncate(row.from_name.as_deref().unwrap_or(&row.from_email), 28);
                    println!(
                        "{:>5.2}  {:<28}  {:>5.1}d  {:>5.1}d  {:>5.0}  {}",
                        row.overdue_score,
                        from,
                        row.waiting_days,
                        row.expected_days,
                        row.waiting_days,
                        truncate(&row.subject, 60),
                    );
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
