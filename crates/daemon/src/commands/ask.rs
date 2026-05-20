use crate::cli::{ArchiveAskModeArg, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

pub struct ArchiveAskRunOptions {
    pub question: String,
    pub account: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    pub mode: ArchiveAskModeArg,
    pub limit: u32,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: ArchiveAskRunOptions) -> anyhow::Result<()> {
    let ArchiveAskRunOptions {
        question,
        account,
        from,
        to,
        after,
        before,
        mode,
        limit,
        format,
    } = options;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await.ok();
    let after = after.as_deref().and_then(parse_dt);
    let before = before.as_deref().and_then(parse_dt);

    let filters = ArchiveAskFiltersData {
        account_id,
        from,
        to,
        after,
        before,
        mode: match mode {
            ArchiveAskModeArg::Hybrid => ArchiveAskMode::Hybrid,
            ArchiveAskModeArg::Lexical => ArchiveAskMode::Lexical,
            ArchiveAskModeArg::Semantic => ArchiveAskMode::Semantic,
        },
    };

    let resp = client
        .request(Request::ArchiveAsk {
            question,
            filters,
            limit,
        })
        .await?;
    print(resp, resolve_format(format))
}

fn parse_dt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .map(|d| {
            d.and_hms_opt(0, 0, 0)
                .expect("midnight is a valid time literal")
                .and_utc()
        })
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::ArchiveAnswer { answer },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&answer)?),
            OutputFormat::Jsonl => println!("{}", serde_json::to_string(&answer)?),
            _ => {
                println!("{}", answer.text);
                if !answer.citations.is_empty() {
                    println!();
                    println!("Citations:");
                    for c in &answer.citations {
                        println!(
                            "  - msg={} thread={} {} \"{}\"",
                            c.message_id,
                            c.thread_id,
                            c.date.format("%Y-%m-%d"),
                            truncate(&c.quote, 80)
                        );
                    }
                }
                println!(
                    "\n(retrieval: requested={:?}, executed={:?}, candidates={})",
                    answer.retrieval.requested_mode,
                    answer.retrieval.executed_mode,
                    answer.retrieval.candidate_count,
                );
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
