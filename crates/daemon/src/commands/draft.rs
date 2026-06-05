use crate::cli::{DraftAction, DraftLengthArg, OutputFormat, VoiceRegisterArg};
use crate::commands::draft_output::{
    draft_suggestion_json, eprint_draft_notes, length_data, register_data, DraftSuggestionView,
};
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
                .request(Request::DraftCompose {
                    account_id: Some(account_id),
                    to: Some(Address {
                        name: None,
                        email: to,
                    }),
                    instruction: purpose,
                    source_message_id: None,
                    thread_id: None,
                    register: register.map(register_data),
                    length_hint: length.map(length_data),
                })
                .await?;
            print_draft_response(resp, fmt)
        }
    }
}

fn print_draft_response(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    let view = match resp {
        Response::Ok { data } => DraftSuggestionView::from_response(data)
            .ok_or_else(|| anyhow::anyhow!("Unexpected response"))?,
        Response::Error { message, .. } => anyhow::bail!(message),
    };
    match fmt {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&draft_suggestion_json(&view))?
            );
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&draft_suggestion_json(&view))?);
        }
        OutputFormat::Ids => {}
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["model", "rewrite_iterations", "body"])?;
            writer.write_record([
                view.model.clone(),
                view.rewrite_iterations.to_string(),
                view.body.clone(),
            ])?;
            print!("{}", String::from_utf8(writer.into_inner()?)?);
        }
        OutputFormat::Table => {
            println!("{}", view.body);
            eprint_draft_notes(&view);
        }
    }
    Ok(())
}
