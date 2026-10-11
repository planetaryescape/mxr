use crate::cli::OutputFormat;
use crate::commands::selection::{resolve_message_ids, SelectionLimit};
use crate::commands::{ensure_message_account, resolve_optional_account};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::{AttachmentMeta, MessageId};
use mxr_protocol::*;
use std::path::PathBuf;

use super::helpers::{
    attachment_by_index, format_bytes, load_attachments, parse_message_id as parse_id_for_legacy,
};

// Attachment commands retain positional message IDs while the list path uses
// the shared selection module.

pub async fn attachments_list(
    message_id: Option<String>,
    search: Option<String>,
    account: Option<String>,
    first: bool,
    limit: Option<u32>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let ids = resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        account_id.as_ref(),
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    match resolve_format(format) {
        OutputFormat::Table => {
            for (index, id) in ids.iter().enumerate() {
                if ids.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("--- {} ---", id.as_str());
                }
                print_one_message_attachments(&mut client, id.clone()).await?;
            }
        }
        OutputFormat::Json => {
            let rows = collect_attachment_rows(&mut client, &ids).await?;
            println!("{}", serde_json::to_string_pretty(&rows)?);
        }
        OutputFormat::Jsonl => {
            let rows = collect_attachment_rows(&mut client, &ids).await?;
            println!("{}", jsonl(&rows)?);
        }
        OutputFormat::Csv => {
            let rows = collect_attachment_rows(&mut client, &ids).await?;
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "index",
                "attachment_id",
                "filename",
                "mime_type",
                "size_bytes",
                "provider_id",
                "local_path",
            ])?;
            for row in &rows {
                writer.write_record(vec![
                    row.message_id.clone(),
                    row.index.to_string(),
                    row.attachment_id.clone(),
                    row.filename.clone(),
                    row.mime_type.clone(),
                    row.size_bytes.to_string(),
                    row.provider_id.clone(),
                    row.local_path.clone().unwrap_or_default(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            let rows = collect_attachment_rows(&mut client, &ids).await?;
            for row in &rows {
                println!("{}", row.attachment_id);
            }
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct AttachmentListRow {
    message_id: String,
    index: usize,
    attachment_id: String,
    filename: String,
    mime_type: String,
    size_bytes: u64,
    provider_id: String,
    local_path: Option<String>,
}

async fn collect_attachment_rows(
    client: &mut IpcClient,
    ids: &[MessageId],
) -> anyhow::Result<Vec<AttachmentListRow>> {
    let mut rows = Vec::new();
    for id in ids {
        let attachments = fetch_attachments(client, id.clone()).await?;
        for (index, attachment) in attachments.into_iter().enumerate() {
            rows.push(AttachmentListRow {
                message_id: id.as_str(),
                index: index + 1,
                attachment_id: attachment.id.as_str(),
                filename: attachment.filename,
                mime_type: attachment.mime_type,
                size_bytes: attachment.size_bytes,
                provider_id: attachment.provider_id,
                local_path: attachment
                    .local_path
                    .map(|path| path.to_string_lossy().to_string()),
            });
        }
    }
    Ok(rows)
}

async fn fetch_attachments(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<Vec<AttachmentMeta>> {
    let resp = client.request(Request::GetBody { message_id: id }).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => Ok(body.attachments),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

async fn print_one_message_attachments(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<()> {
    let attachments = fetch_attachments(client, id).await?;
    if attachments.is_empty() {
        println!("No attachments");
    } else {
        println!(
            "{:<4} {:<40} {:<25} {:>10}",
            "#", "FILENAME", "TYPE", "SIZE"
        );
        println!("{}", "-".repeat(82));
        for (i, att) in attachments.iter().enumerate() {
            println!(
                "{:<4} {:<40} {:<25} {:>10}",
                i + 1,
                att.filename,
                att.mime_type,
                format_bytes(att.size_bytes),
            );
        }
        println!("\n{} attachment(s)", attachments.len());
    }
    Ok(())
}

/// Reduce a mail-controlled attachment filename to a single, safe path
/// component so a `--dir` download can never escape the chosen directory.
///
/// `attachment.filename` comes from provider/email metadata and is therefore
/// attacker-controlled. `Path::join` discards the base when the joined value is
/// absolute, and `../` segments walk upward, so a hostile filename such as
/// `../../../.ssh/authorized_keys` or `/etc/crontab` would otherwise let a
/// sender write outside the user's `--dir` (an arbitrary-file-overwrite /
/// code-exec vector). Taking only the final path component — and falling back
/// to a stable per-attachment default when there isn't a usable one (`..`, `.`,
/// empty, or non-UTF-8) — guarantees `target_dir.join(name)` stays a direct
/// child of `target_dir`. Normal filenames pass through unchanged.
fn safe_download_name(raw: &str, attachment_id: &mxr_core::AttachmentId) -> String {
    std::path::Path::new(raw)
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(
            || format!("attachment-{}", attachment_id.as_str()),
            str::to_owned,
        )
}

#[derive(serde::Serialize)]
struct AttachmentTransferResult {
    message_id: String,
    attachment_id: String,
    filename: String,
    path: String,
    length_bytes: u64,
    sha256: String,
}

fn print_transfer_results(
    results: &[AttachmentTransferResult],
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            for (index, item) in results.iter().enumerate() {
                println!("#{} {} -> {}", index + 1, item.filename, item.path);
            }
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(results)?),
        OutputFormat::Jsonl => println!("{}", jsonl(results)?),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "message_id",
                "attachment_id",
                "filename",
                "path",
                "length_bytes",
                "sha256",
            ])?;
            for item in results {
                writer.write_record([
                    &item.message_id,
                    &item.attachment_id,
                    &item.filename,
                    &item.path,
                    &item.length_bytes.to_string(),
                    &item.sha256,
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for item in results {
                println!("{}", item.attachment_id);
            }
        }
    }
    Ok(())
}

pub async fn attachments_download(
    message_id: String,
    account: Option<String>,
    index: Option<usize>,
    dir: Option<PathBuf>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let id = parse_id_for_legacy(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    ensure_message_account(&mut client, &id, account_id.as_ref()).await?;
    let attachments = load_attachments(&mut client, &id).await?;
    let selected: Vec<(usize, &mxr_core::AttachmentMeta)> = match index {
        Some(index) => vec![(index, attachment_by_index(&attachments, index)?)],
        None => attachments
            .iter()
            .enumerate()
            .map(|(idx, item)| (idx + 1, item))
            .collect(),
    };
    let target_dir = dir.unwrap_or(std::env::current_dir()?);
    std::fs::create_dir_all(&target_dir)?;
    let mut results = Vec::new();
    for (_display_index, attachment) in selected {
        let target = target_dir.join(safe_download_name(&attachment.filename, &attachment.id));
        let downloaded = client
            .transfer_attachment_to(&id, &attachment.id, &target, false)
            .await?;
        results.push(AttachmentTransferResult {
            message_id: id.as_str(),
            attachment_id: attachment.id.as_str(),
            filename: downloaded.filename,
            path: downloaded.path.display().to_string(),
            length_bytes: downloaded.length_bytes,
            sha256: downloaded.sha256,
        });
    }
    print_transfer_results(&results, format)
}

pub async fn attachments_open(
    message_id: String,
    account: Option<String>,
    index: usize,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let id = parse_id_for_legacy(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    ensure_message_account(&mut client, &id, account_id.as_ref()).await?;
    let attachments = load_attachments(&mut client, &id).await?;
    let attachment = attachment_by_index(&attachments, index)?;
    let target_dir = mxr_config::data_dir().join("attachments");
    std::fs::create_dir_all(&target_dir)?;
    let target = target_dir.join(format!(
        "{}-{}",
        attachment.id,
        safe_download_name(&attachment.filename, &attachment.id)
    ));
    let downloaded = client
        .transfer_attachment_to(&id, &attachment.id, &target, true)
        .await?;
    open::that(&downloaded.path)?;
    print_transfer_results(
        &[AttachmentTransferResult {
            message_id: id.as_str(),
            attachment_id: attachment.id.as_str(),
            filename: downloaded.filename,
            path: downloaded.path.display().to_string(),
            length_bytes: downloaded.length_bytes,
            sha256: downloaded.sha256,
        }],
        format,
    )
}

#[cfg(test)]
mod tests {
    use super::safe_download_name;
    use mxr_core::AttachmentId;
    use std::path::Path;

    #[test]
    fn hostile_filenames_stay_inside_target_dir() {
        let target_dir = Path::new("/tmp/mxr-downloads");
        let id = AttachmentId::default();

        // Absolute paths, `../` traversal, `..`, and empty names must all
        // collapse to a single component whose parent is exactly `target_dir`,
        // so the CLI copy can never write outside the user's chosen `--dir`.
        for hostile in ["../../evil", "/etc/evil", "..", ""] {
            let name = safe_download_name(hostile, &id);
            let target = target_dir.join(&name);
            assert_eq!(
                target.parent(),
                Some(target_dir),
                "{hostile:?} escaped target_dir as {target:?} (name {name:?})",
            );
        }
    }

    #[test]
    fn normal_filename_is_preserved() {
        let target_dir = Path::new("/tmp/mxr-downloads");
        let id = AttachmentId::default();

        assert_eq!(safe_download_name("report.pdf", &id), "report.pdf");
        let target = target_dir.join(safe_download_name("report.pdf", &id));
        assert_eq!(target.parent(), Some(target_dir));
    }
}
