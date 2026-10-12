use crate::{
    cli::OutputFormat,
    commands::{expect_response, resolve_optional_account},
    ipc_client::IpcClient,
    output::resolve_format,
};
use mxr_protocol::{Request, Response, ResponseData, MAX_DRAFT_ATTACHMENT_BYTES};
use std::path::PathBuf;

pub async fn attach(
    draft_id: String,
    files: Vec<PathBuf>,
    expected_revision: Option<i64>,
    dry_run: bool,
    account: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account = resolve_optional_account(&mut client, account.as_deref()).await?;
    let mut draft = expect_response(
        client
            .request(Request::GetDraft {
                draft_id: draft_id.parse()?,
            })
            .await?,
        |response| match response {
            Response::Ok {
                data: ResponseData::Draft { draft },
            } => Some(draft),
            _ => None,
        },
    )?;
    if account.is_some_and(|account| account != draft.account_id) {
        anyhow::bail!("Draft belongs to another account");
    }
    if !dry_run && (expected_revision.is_none() || expected_revision != draft.revision) {
        anyhow::bail!("Draft revision changed; preview the current draft before attaching files");
    }
    let mut declarations = Vec::new();
    for path in &files {
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() || metadata.len() > MAX_DRAFT_ATTACHMENT_BYTES {
            anyhow::bail!("Attachment must be a regular file within the daemon's 25 MiB managed-resource limit");
        }
        declarations.push(serde_json::json!({"filename": path.file_name().and_then(|name| name.to_str()), "length_bytes": metadata.len()}));
    }
    let result = if dry_run {
        serde_json::json!({"dry_run": true, "draft_id": draft.id, "expected_revision": draft.revision, "attachments": declarations})
    } else {
        draft.attachments.extend(files);
        let saved = expect_response(
            client.request(Request::UpdateDraft { draft }).await?,
            |response| match response {
                Response::Ok {
                    data: ResponseData::Draft { draft },
                } => Some(draft),
                _ => None,
            },
        )?;
        serde_json::json!({"dry_run": false, "draft": saved})
    };
    match resolve_format(format) {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&result)?),
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}
