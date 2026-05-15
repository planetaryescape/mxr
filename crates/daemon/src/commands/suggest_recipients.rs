use crate::cli::OutputFormat;
use crate::commands::resolve_account;
use crate::ipc_client::IpcClient;
use crate::output::resolve_format;
use mxr_protocol::*;
use std::io::Read;
use std::str::FromStr;

pub async fn run(
    draft_id: Option<String>,
    subject: Option<String>,
    body_stdin: bool,
    account: Option<String>,
    limit: u32,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_account(&mut client, account.as_deref()).await?;

    let draft = build_draft(&mut client, draft_id, subject, body_stdin, account_id).await?;

    let resp = client
        .request(Request::SuggestCollaborators { draft, limit })
        .await?;
    print(resp, resolve_format(format))
}

async fn build_draft(
    client: &mut IpcClient,
    draft_id: Option<String>,
    subject: Option<String>,
    body_stdin: bool,
    account_id: mxr_core::AccountId,
) -> anyhow::Result<mxr_core::Draft> {
    if let Some(id_str) = draft_id {
        let id = mxr_core::DraftId::from_str(&id_str)
            .map_err(|e| anyhow::anyhow!("invalid draft id: {e}"))?;
        let resp = client.request(Request::ListDrafts).await?;
        let drafts = match resp {
            Response::Ok {
                data: ResponseData::Drafts { drafts },
            } => drafts,
            Response::Error { message, .. } => anyhow::bail!(message),
            _ => anyhow::bail!("unexpected response listing drafts"),
        };
        return drafts
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| anyhow::anyhow!("draft not found: {id_str}"));
    }

    let subject = subject.ok_or_else(|| {
        anyhow::anyhow!("either --draft <id> or --subject \"...\" must be supplied")
    })?;
    let body = if body_stdin {
        let mut s = String::new();
        std::io::stdin()
            .read_to_string(&mut s)
            .map_err(|e| anyhow::anyhow!("read stdin: {e}"))?;
        s
    } else {
        String::new()
    };
    let now = chrono::Utc::now();
    Ok(mxr_core::Draft {
        id: mxr_core::DraftId::new(),
        account_id,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject,
        body_markdown: body,
        attachments: vec![],
        created_at: now,
        updated_at: now,
    })
}

fn print(resp: Response, fmt: OutputFormat) -> anyhow::Result<()> {
    match resp {
        Response::Ok {
            data: ResponseData::SuggestedCollaborators { suggestions },
        } => match fmt {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&suggestions)?),
            OutputFormat::Jsonl => {
                for s in suggestions {
                    println!("{}", serde_json::to_string(&s)?);
                }
            }
            OutputFormat::Ids => {
                for s in suggestions {
                    println!("{}", s.email);
                }
            }
            _ => {
                if suggestions.is_empty() {
                    println!("(no suggestions)");
                }
                for s in suggestions {
                    println!(
                        "  {} ({}, {} threads)",
                        s.display_name.as_deref().unwrap_or(s.email.as_str()),
                        s.confidence,
                        s.evidence_msg_ids.len()
                    );
                    println!("    {}", s.reason);
                }
            }
        },
        Response::Error { message, .. } => anyhow::bail!(message),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
