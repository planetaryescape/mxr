//! `mxr summarize <thread-id>` — LLM-driven thread summary. Accepts
//! `--search QUERY` plus `--first` or `--limit N` to summarize multiple
//! threads in one go (one LLM call per thread).

use crate::cli::OutputFormat;
use crate::commands::resolve_optional_account;
use crate::commands::selection::{resolve_thread_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::ThreadId;
use mxr_protocol::*;

pub async fn run(
    thread_id: Option<String>,
    search: Option<String>,
    account: Option<String>,
    first: bool,
    limit: Option<u32>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let ids = resolve_thread_ids(
        &mut client,
        thread_id.into_iter().collect(),
        search,
        account_id.as_ref(),
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No threads matched");
    }

    let fmt = resolve_format(format);
    let mut payloads: Vec<serde_json::Value> = Vec::with_capacity(ids.len());

    for (index, id) in ids.iter().enumerate() {
        let (text, model) = match summarize_one(&mut client, id).await {
            Ok(pair) => pair,
            Err(error) => {
                if matches!(fmt, OutputFormat::Json | OutputFormat::Jsonl) {
                    payloads.push(serde_json::json!({
                        "thread_id": id.to_string(),
                        "error": error.to_string(),
                    }));
                    continue;
                } else {
                    anyhow::bail!("{error}");
                }
            }
        };

        match fmt {
            OutputFormat::Json | OutputFormat::Jsonl => {
                payloads.push(serde_json::json!({
                    "thread_id": id.to_string(),
                    "model": model,
                    "summary": text,
                }));
            }
            OutputFormat::Csv => {
                // Single-line CSV record per thread (text quoted by csv).
                let mut writer = csv::Writer::from_writer(Vec::new());
                if index == 0 {
                    writer.write_record(["thread_id", "model", "summary"])?;
                }
                writer.write_record(&[id.to_string(), model.clone(), text.clone()])?;
                let bytes = writer.into_inner()?;
                let line = String::from_utf8(bytes)?;
                print!("{line}");
            }
            OutputFormat::Ids => {
                println!("{id}");
            }
            OutputFormat::Table => {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {id} ---");
                }
                println!("{text}");
                eprintln!("\n[via {model}]");
            }
        }
    }

    match fmt {
        OutputFormat::Json => {
            if ids.len() == 1 {
                println!("{}", serde_json::to_string_pretty(&payloads[0])?);
            } else {
                println!("{}", serde_json::to_string_pretty(&payloads)?);
            }
        }
        OutputFormat::Jsonl => {
            for payload in &payloads {
                println!("{}", serde_json::to_string(payload)?);
            }
        }
        _ => {}
    }

    Ok(())
}

async fn summarize_one(
    client: &mut IpcClient,
    thread_id: &ThreadId,
) -> anyhow::Result<(String, String)> {
    let resp = client
        .request(Request::SummarizeThread {
            thread_id: thread_id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::ThreadSummary { text, model },
        } => Ok((text, model)),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
