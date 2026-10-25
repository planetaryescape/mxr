//! `mxr compose assist` and `mxr compose apply`: writing help from the CLI.
//! `assist` prints the daemon's proposal and changes nothing; `apply` writes a
//! proposal into a stored draft.

use crate::cli::{ComposeAction, ComposeAssistArgs, OutputFormat};
use crate::commands::resolve_account;
use crate::commands::selection::parse_message_id;
use crate::handler::compose_assist::PROGRESS_OPERATION;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use anyhow::Context;
use mxr_composer::check::split_body;
use mxr_core::id::DraftId;
use mxr_core::types::{Address, DraftContent};
use mxr_protocol::{
    AssistActionData, AssistAdjustData, ComposeProposalData, DaemonEvent, DraftTargetData, Request,
    Response, ResponseData, TextChangeKindData, TextRangeData,
};
use std::io::Read;
use std::str::FromStr;

pub async fn run(action: ComposeAction, parent_format: Option<OutputFormat>) -> anyhow::Result<()> {
    match action {
        ComposeAction::Models { format } => models(format.or(parent_format)).await,
        ComposeAction::Assist(args) => assist(*args, parent_format).await,
        ComposeAction::Apply {
            proposal_id,
            draft,
            format,
        } => apply(proposal_id, &draft, format.or(parent_format)).await,
    }
}

async fn models(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let format = resolve_format(format);
    let mut client = IpcClient::connect().await?;
    match client.request(Request::GetComposeModels).await? {
        Response::Ok {
            data: ResponseData::ComposeModels { models },
        } => match format {
            OutputFormat::Json | OutputFormat::Jsonl => {
                println!("{}", serde_json::to_string(&models)?);
                Ok(())
            }
            OutputFormat::Table | OutputFormat::Ids | OutputFormat::Csv => {
                for choice in models.choices {
                    let place = if choice.cloud {
                        "cloud"
                    } else {
                        "this machine"
                    };
                    let status = choice
                        .unavailable_reason
                        .as_deref()
                        .map_or("available", |reason| reason);
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        choice.id, choice.label, place, choice.sees, status
                    );
                }
                Ok(())
            }
        },
        Response::Ok { .. } => anyhow::bail!("Unexpected response to GetComposeModels"),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn assist(
    args: ComposeAssistArgs,
    parent_format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let fmt = resolve_format(args.format.clone().or(parent_format));
    let mut client = IpcClient::connect().await?;
    let draft = target(&mut client, &args).await?;
    let text = match (&args.text, &draft) {
        (Some(path), _) => read_text(path)?,
        (None, DraftTargetData::Stored { draft_id }) => stored_text(&mut client, draft_id).await?,
        (None, _) => String::new(),
    };
    let selection = args.select.as_deref().map(parse_selection).transpose()?;
    let stored = match &draft {
        DraftTargetData::Stored { draft_id } => Some(draft_id.clone()),
        _ => None,
    };
    let request = Request::ComposeAssist {
        draft,
        text,
        selection,
        action: explicit_action(&args),
        instruction: args.instruction.clone(),
        adjust: AssistAdjustData {
            shorter: args.shorter,
            warmer: args.warmer,
            more_formal: args.more_formal,
            proofread: args.proofread,
        },
        model: args.model.clone(),
        revise: args.revise.clone(),
    };
    // The model sets the pace and a local one can take minutes, so this takes
    // the no-deadline path. Stage lines go to stderr, leaving stdout for the
    // proposal.
    let response = client
        .request_with_events(request, |event| {
            if let DaemonEvent::OperationProgress {
                operation,
                current,
                total,
                message,
                ..
            } = event
            {
                if operation == PROGRESS_OPERATION {
                    eprintln!("[{current}/{}] {message}", total.unwrap_or(current));
                }
            }
        })
        .await?;
    match response {
        Response::Ok {
            data: ResponseData::ComposeProposal { proposal },
        } => print_proposal(&proposal, stored.as_ref(), fmt),
        Response::Ok { .. } => anyhow::bail!("Unexpected response"),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

async fn apply(
    proposal_id: String,
    draft: &str,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let fmt = resolve_format(format);
    let draft_id =
        DraftId::from_str(draft).with_context(|| format!("Invalid draft ID '{draft}'"))?;
    let mut client = IpcClient::connect().await?;
    let response = client
        .request(Request::ApplyComposeProposal {
            proposal_id,
            draft_id,
        })
        .await?;
    match response {
        Response::Ok {
            data:
                ResponseData::ComposeApplied {
                    draft_id,
                    text_hash,
                },
        } => {
            match fmt {
                OutputFormat::Table | OutputFormat::Ids | OutputFormat::Csv => {
                    println!("Wrote the proposal into draft {draft_id}.");
                }
                OutputFormat::Json | OutputFormat::Jsonl => println!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "draft_id": draft_id,
                        "text_hash": text_hash,
                    }))?
                ),
            }
            Ok(())
        }
        Response::Ok { .. } => anyhow::bail!("Unexpected response"),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

/// Which draft the request is about, from the flags.
async fn target(
    client: &mut IpcClient,
    args: &ComposeAssistArgs,
) -> anyhow::Result<DraftTargetData> {
    let recipients = |args: &ComposeAssistArgs| -> Vec<Address> {
        args.to
            .iter()
            .flat_map(|raw| mxr_mail_parse::parse_address_list(raw))
            .collect()
    };
    if let Some(draft) = &args.draft {
        let draft_id =
            DraftId::from_str(draft).with_context(|| format!("Invalid draft ID '{draft}'"))?;
        return Ok(DraftTargetData::Stored { draft_id });
    }
    if let Some(message) = &args.reply_to {
        return Ok(DraftTargetData::Reply {
            message_id: parse_message_id(message)?,
            all: args.all,
        });
    }
    if let Some(message) = &args.forward {
        let to = recipients(args);
        anyhow::ensure!(!to.is_empty(), "--forward needs a recipient: pass --to");
        return Ok(DraftTargetData::Forward {
            message_id: parse_message_id(message)?,
            to,
        });
    }
    let to = recipients(args);
    anyhow::ensure!(!to.is_empty(), "--new needs a recipient: pass --to");
    let account_id = resolve_account(client, args.account.as_deref()).await?;
    Ok(DraftTargetData::New {
        account_id,
        to,
        subject: args.subject.clone().unwrap_or_default(),
    })
}

fn explicit_action(args: &ComposeAssistArgs) -> Option<AssistActionData> {
    if args.write {
        Some(AssistActionData::Write)
    } else if args.finish {
        Some(AssistActionData::Finish)
    } else if args.polish {
        Some(AssistActionData::Polish)
    } else {
        None
    }
}

/// The text from a file, or from stdin for `-`.
fn read_text(path: &str) -> anyhow::Result<String> {
    if path == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("Could not read the text from stdin")?;
        Ok(text)
    } else {
        std::fs::read_to_string(path).with_context(|| format!("Could not read {path}"))
    }
}

/// The user's text region of a stored draft: its body without the signature
/// and forward header, which writing help never sees.
async fn stored_text(client: &mut IpcClient, draft_id: &DraftId) -> anyhow::Result<String> {
    let response = client
        .request(Request::GetDraft {
            draft_id: draft_id.clone(),
        })
        .await?;
    match response {
        Response::Ok {
            data: ResponseData::Draft { draft },
        } => match &draft.content {
            DraftContent::Markdown { source } => Ok(split_body(source).text.to_string()),
            DraftContent::Html { .. } => {
                anyhow::bail!(
                    "Draft {draft_id} has an HTML body; writing help works on markdown drafts"
                )
            }
        },
        Response::Ok { .. } => anyhow::bail!("Unexpected response"),
        Response::Error { message, .. } => anyhow::bail!(message),
    }
}

/// `START..END` as byte offsets.
fn parse_selection(raw: &str) -> anyhow::Result<TextRangeData> {
    let (start, end) = raw
        .split_once("..")
        .with_context(|| format!("--select takes START..END (byte offsets), got '{raw}'"))?;
    let parse = |part: &str| {
        part.trim()
            .parse::<u32>()
            .with_context(|| format!("--select takes START..END (byte offsets), got '{raw}'"))
    };
    Ok(TextRangeData {
        start: parse(start)?,
        end: parse(end)?,
    })
}

fn print_proposal(
    proposal: &ComposeProposalData,
    stored: Option<&DraftId>,
    fmt: OutputFormat,
) -> anyhow::Result<()> {
    match fmt {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(proposal)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(proposal)?),
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Ids => {
            print!("{}", render_proposal(proposal, stored));
        }
    }
    Ok(())
}

/// The proposal as a person reads it: your words plain, what was added in
/// `{+…+}`, what was dropped in `[-…-]`, then the gaps and which of your
/// sentences moved.
fn render_proposal(proposal: &ComposeProposalData, stored: Option<&DraftId>) -> String {
    let mut out = String::new();
    let provenance = &proposal.provenance;
    out.push_str(&format!(
        "Proposal {} · {} · {}{}\n\n",
        proposal.proposal_id,
        action_name(proposal.action),
        provenance.model,
        if provenance.history_used {
            ", read your other mail"
        } else {
            ""
        },
    ));
    for change in &proposal.changes {
        match change.kind {
            TextChangeKindData::Kept => out.push_str(&change.text),
            TextChangeKindData::Added => out.push_str(&format!("{{+{}+}}", change.text)),
            TextChangeKindData::Removed => out.push_str(&format!("[-{}-]", change.text)),
        }
    }
    out.push_str("\n\n");
    if !proposal.yours_changed.is_empty() {
        let numbers: Vec<String> = proposal
            .yours_changed
            .iter()
            .map(|index| (index + 1).to_string())
            .collect();
        out.push_str(&format!(
            "Your sentences changed or dropped: {} (counting from 1)\n",
            numbers.join(", ")
        ));
    }
    if !proposal.gaps.is_empty() {
        out.push_str(&format!(
            "{} to fill in before this can be sent:\n",
            proposal.gaps.len()
        ));
        for gap in &proposal.gaps {
            out.push_str(&format!("  - {}\n", gap.label));
        }
    }
    match stored {
        Some(draft_id) => out.push_str(&format!(
            "Accept: mxr compose apply {} --draft {draft_id}\n",
            proposal.proposal_id
        )),
        None => out.push_str(&format!(
            "Accept: mxr compose apply {} --draft <draft-id>\n",
            proposal.proposal_id
        )),
    }
    out
}

fn action_name(action: AssistActionData) -> &'static str {
    match action {
        AssistActionData::Write => "write",
        AssistActionData::Finish => "finish",
        AssistActionData::Polish => "polish",
        AssistActionData::EditSelection => "edit selection",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{AiLocalityData, DraftProvenanceData, GapData, TextChangeData};

    fn proposal() -> ComposeProposalData {
        ComposeProposalData {
            proposal_id: "p1".into(),
            action: AssistActionData::Finish,
            base: String::new(),
            text: "Hi Sam,\n\nThursday works.".into(),
            changes: vec![
                TextChangeData {
                    kind: TextChangeKindData::Added,
                    text: "Hi Sam,\n\n".into(),
                },
                TextChangeData {
                    kind: TextChangeKindData::Kept,
                    text: "Thursday works".into(),
                },
                TextChangeData {
                    kind: TextChangeKindData::Removed,
                    text: " ok".into(),
                },
                TextChangeData {
                    kind: TextChangeKindData::Added,
                    text: ". Price: [[?: the price]]".into(),
                },
            ],
            gaps: vec![GapData {
                label: "the price".into(),
                start: 0,
                end: 1,
            }],
            provenance: DraftProvenanceData {
                model: "gemma4".into(),
                locality: AiLocalityData::Local,
                history_used: true,
                voice_examples: Vec::new(),
                conversation: Vec::new(),
                rewrite: None,
            },
            yours_changed: vec![1],
            composer_version: "c1".into(),
        }
    }

    #[test]
    fn the_readable_proposal_marks_added_removed_and_gaps() {
        let draft_id = DraftId::new();
        let text = render_proposal(&proposal(), Some(&draft_id));
        assert!(text.starts_with("Proposal p1 · finish · gemma4, read your other mail\n"));
        assert!(text.contains("{+Hi Sam,\n\n+}Thursday works[- ok-]{+. Price: [[?: the price]]+}"));
        assert!(text.contains("Your sentences changed or dropped: 2 (counting from 1)"));
        assert!(text.contains("1 to fill in before this can be sent:\n  - the price\n"));
        assert!(text.contains(&format!("mxr compose apply p1 --draft {draft_id}")));
    }

    #[test]
    fn a_selection_is_start_dot_dot_end() {
        let range = parse_selection("16..31").unwrap();
        assert_eq!((range.start, range.end), (16, 31));
        assert!(parse_selection("16").is_err());
        assert!(parse_selection("a..3").is_err());
        assert!(parse_selection("-1..3").is_err());
    }
}
