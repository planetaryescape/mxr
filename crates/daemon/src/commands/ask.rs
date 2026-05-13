use crate::cli::{ArchiveAskModeArg, OutputFormat};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    question: String,
    account: Option<String>,
    from: Option<String>,
    to: Option<String>,
    after: Option<String>,
    before: Option<String>,
    mode: ArchiveAskModeArg,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
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
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
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
                        println!("  - msg={} \"{}\"", c.msg_id, truncate(&c.quote, 80));
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
