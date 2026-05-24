//! `mxr draft-assist <thread-id> <instruction>` — LLM-grounded draft
//! reply generation. Writes the body to stdout for the caller to edit
//! or pipe into compose. Accepts `--search QUERY` plus `--first` /
//! `--limit N` to draft for multiple threads in one go.

use crate::cli::OutputFormat;
use crate::commands::selection::{resolve_thread_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::ThreadId;
use mxr_protocol::*;

struct DraftAssistSuggestion {
    body: String,
    model: String,
    voice_match: Option<VoiceMatchData>,
    humanizer: Option<HumanizerReportSummaryData>,
    rewrite_iterations: u8,
}

pub struct DraftAssistRunOptions {
    pub thread_id: Option<String>,
    pub search: Option<String>,
    pub first: bool,
    pub limit: Option<u32>,
    pub instruction: String,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: DraftAssistRunOptions) -> anyhow::Result<()> {
    let DraftAssistRunOptions {
        thread_id,
        search,
        first,
        limit,
        instruction,
        format,
    } = options;
    let mut client = IpcClient::connect().await?;
    let ids = resolve_thread_ids(
        &mut client,
        thread_id.into_iter().collect(),
        search,
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No threads matched");
    }

    let fmt = resolve_format(format);
    let mut payloads: Vec<serde_json::Value> = Vec::with_capacity(ids.len());

    for (index, id) in ids.iter().enumerate() {
        let suggestion = match draft_one(&mut client, id, instruction.clone()).await {
            Ok(suggestion) => suggestion,
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
                    "model": suggestion.model,
                    "body": suggestion.body,
                    "voice_match": suggestion.voice_match,
                    "humanizer": suggestion.humanizer,
                    "rewrite_iterations": suggestion.rewrite_iterations,
                }));
            }
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                if index == 0 {
                    writer.write_record(["thread_id", "model", "body"])?;
                }
                writer.write_record(&[
                    id.to_string(),
                    suggestion.model.clone(),
                    suggestion.body.clone(),
                ])?;
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
                println!("{}", suggestion.body);
                eprintln!("\n[via {} — review before sending]", suggestion.model);
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

async fn draft_one(
    client: &mut IpcClient,
    thread_id: &ThreadId,
    instruction: String,
) -> anyhow::Result<DraftAssistSuggestion> {
    let resp = client
        .request(Request::DraftAssist {
            thread_id: thread_id.clone(),
            instruction,
        })
        .await?;
    match resp {
        Response::Ok {
            data:
                ResponseData::DraftSuggestion {
                    body,
                    model,
                    voice_match,
                    humanizer,
                    rewrite_iterations,
                },
        } => Ok(DraftAssistSuggestion {
            body,
            model,
            voice_match,
            humanizer,
            rewrite_iterations,
        }),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
