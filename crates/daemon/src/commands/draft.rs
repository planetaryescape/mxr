use crate::cli::{DraftAction, DraftLengthArg, OutputFormat, VoiceRegisterArg};
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_core::id::DraftId;
use mxr_core::types::Address;
use mxr_protocol::*;
use std::str::FromStr;

pub async fn run(
    action: Option<DraftAction>,
    to: Option<String>,
    purpose: Option<String>,
    account: Option<String>,
    register: Option<VoiceRegisterArg>,
    length: Option<DraftLengthArg>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let fmt = resolve_format(format);
    match action {
        Some(DraftAction::Refine {
            draft_id,
            shorter,
            warmer,
            more_formal,
            less_emoji,
            add_context,
        }) => {
            let draft_id = DraftId::from_str(&draft_id)?;
            let resp = client
                .request(Request::DraftRefine {
                    draft_id,
                    knobs: DraftRefineKnobsData {
                        shorter,
                        warmer,
                        more_formal,
                        less_emoji,
                        add_context,
                    },
                })
                .await?;
            print_draft_response(resp, fmt)
        }
        None => {
            let to = to.ok_or_else(|| anyhow::anyhow!("Pass --to for a new draft"))?;
            let purpose =
                purpose.ok_or_else(|| anyhow::anyhow!("Pass --purpose for a new draft"))?;
            let account_id = resolve_account(&mut client, account.as_deref()).await?;
            let resp = client
                .request(Request::DraftNew {
                    account_id,
                    to: Address {
                        name: None,
                        email: to,
                    },
                    purpose,
                    register: register.map(register_data),
                    length_hint: length.map(length_data),
                })
                .await?;
            print_draft_response(resp, fmt)
        }
    }
}

fn print_draft_response(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
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
        } => match fmt {
            OutputFormat::Json | OutputFormat::Jsonl => {
                let payload = serde_json::json!({
                    "body": body,
                    "model": model,
                    "voice_match": voice_match,
                    "humanizer": humanizer,
                    "rewrite_iterations": rewrite_iterations,
                });
                if matches!(fmt, OutputFormat::Json) {
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                } else {
                    println!("{}", serde_json::to_string(&payload)?);
                }
                Ok(())
            }
            OutputFormat::Ids => Ok(()),
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                writer.write_record(["model", "rewrite_iterations", "body"])?;
                writer.write_record([model, rewrite_iterations.to_string(), body])?;
                print!("{}", String::from_utf8(writer.into_inner()?)?);
                Ok(())
            }
            OutputFormat::Table => {
                println!("{body}");
                eprintln!("\n[via {model} — review before sending]");
                if let Some(voice_match) = voice_match {
                    eprintln!(
                        "voice_match={:.2} {:?}",
                        voice_match.score, voice_match.confidence
                    );
                }
                if let Some(humanizer) = humanizer {
                    eprintln!("humanizer={}/100", humanizer.score);
                }
                if rewrite_iterations > 0 {
                    eprintln!("rewritten {rewrite_iterations}x");
                }
                Ok(())
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn register_data(value: VoiceRegisterArg) -> VoiceRegisterData {
    match value {
        VoiceRegisterArg::Casual => VoiceRegisterData::Casual,
        VoiceRegisterArg::Neutral => VoiceRegisterData::Neutral,
        VoiceRegisterArg::Formal => VoiceRegisterData::Formal,
    }
}

fn length_data(value: DraftLengthArg) -> DraftLengthHintData {
    match value {
        DraftLengthArg::Short => DraftLengthHintData::Short,
        DraftLengthArg::Medium => DraftLengthHintData::Medium,
        DraftLengthArg::Long => DraftLengthHintData::Long,
    }
}
