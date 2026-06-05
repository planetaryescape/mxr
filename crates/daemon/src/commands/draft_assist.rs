//! `mxr draft-assist <thread-id> <instruction>` — LLM-grounded draft
//! reply generation. Writes the body to stdout for the caller to edit
//! or pipe into compose. Accepts `--search QUERY` plus `--first` /
//! `--limit N` to draft for multiple threads in one go, and
//! `--register` / `--length` to override the inferred tone.

use crate::cli::{DraftLengthArg, OutputFormat, VoiceRegisterArg};
use crate::commands::draft_output::{
    draft_suggestion_json, eprint_draft_notes, length_data, register_data, DraftSuggestionView,
};
use crate::commands::resolve_optional_account;
use crate::commands::selection::{resolve_thread_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::ThreadId;
use mxr_protocol::*;

pub struct DraftAssistRunOptions {
    pub thread_id: Option<String>,
    pub search: Option<String>,
    pub account: Option<String>,
    pub first: bool,
    pub limit: Option<u32>,
    pub instruction: String,
    pub register: Option<VoiceRegisterArg>,
    pub length: Option<DraftLengthArg>,
    pub format: Option<OutputFormat>,
}

pub async fn run(options: DraftAssistRunOptions) -> anyhow::Result<()> {
    let DraftAssistRunOptions {
        thread_id,
        search,
        account,
        first,
        limit,
        instruction,
        register,
        length,
        format,
    } = options;
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

    let register = register.map(register_data);
    let length = length.map(length_data);
    let fmt = resolve_format(format);
    let mut payloads: Vec<serde_json::Value> = Vec::with_capacity(ids.len());

    for (index, id) in ids.iter().enumerate() {
        let view = match draft_one(&mut client, id, instruction.clone(), register, length).await {
            Ok(view) => view,
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
                let mut payload = draft_suggestion_json(&view);
                payload["thread_id"] = serde_json::json!(id.to_string());
                payloads.push(payload);
            }
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                if index == 0 {
                    writer.write_record(["thread_id", "model", "body"])?;
                }
                writer.write_record(&[id.to_string(), view.model.clone(), view.body.clone()])?;
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
                println!("{}", view.body);
                eprint_draft_notes(&view);
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
    register: Option<VoiceRegisterData>,
    length: Option<DraftLengthHintData>,
) -> anyhow::Result<DraftSuggestionView> {
    let resp = client
        .request(Request::DraftCompose {
            account_id: None,
            to: None,
            instruction,
            source_message_id: None,
            thread_id: Some(thread_id.clone()),
            register,
            length_hint: length,
        })
        .await?;
    match resp {
        Response::Ok { data } => DraftSuggestionView::from_response(data)
            .ok_or_else(|| anyhow::anyhow!("Unexpected response")),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
    }
}
