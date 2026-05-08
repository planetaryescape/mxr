use crate::commands::selection::{resolve_message_ids, SelectionLimit};
use crate::ipc_client::IpcClient;
use mxr_core::MessageId;
use mxr_protocol::*;
use std::path::PathBuf;

use super::helpers::{
    attachment_by_index, format_bytes, load_attachments, parse_message_id as parse_id_for_legacy,
    request_attachment_file,
};

// `parse_id_for_legacy` is kept around for the download/open paths
// that still take a single positional string. The list path now
// resolves through the shared `selection` module.

pub async fn attachments_list(
    message_id: Option<String>,
    search: Option<String>,
    first: bool,
    limit: Option<u32>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let ids = resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    for (index, id) in ids.iter().enumerate() {
        if ids.len() > 1 {
            if index > 0 {
                println!();
            }
            println!("--- {} ---", id.as_str());
        }
        print_one_message_attachments(&mut client, id.clone()).await?;
    }
    Ok(())
}

async fn print_one_message_attachments(
    client: &mut IpcClient,
    id: MessageId,
) -> anyhow::Result<()> {
    let resp = client.request(Request::GetBody { message_id: id }).await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            if body.attachments.is_empty() {
                println!("No attachments");
            } else {
                println!(
                    "{:<4} {:<40} {:<25} {:>10}",
                    "#", "FILENAME", "TYPE", "SIZE"
                );
                println!("{}", "-".repeat(82));
                for (i, att) in body.attachments.iter().enumerate() {
                    println!(
                        "{:<4} {:<40} {:<25} {:>10}",
                        i + 1,
                        att.filename,
                        att.mime_type,
                        format_bytes(att.size_bytes),
                    );
                }
                println!("\n{} attachment(s)", body.attachments.len());
            }
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn attachments_download(
    message_id: String,
    index: Option<usize>,
    dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let id = parse_id_for_legacy(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let attachments = load_attachments(&mut client, &id).await?;

    let selected: Vec<(usize, &mxr_core::AttachmentMeta)> = match index {
        Some(index) => vec![(index, attachment_by_index(&attachments, index)?)],
        None => attachments
            .iter()
            .enumerate()
            .map(|(idx, attachment)| (idx + 1, attachment))
            .collect(),
    };

    for (display_index, attachment) in selected {
        let path = request_attachment_file(
            &mut client,
            Request::DownloadAttachment {
                message_id: id.clone(),
                attachment_id: attachment.id.clone(),
            },
        )
        .await?;
        let final_path = if let Some(target_dir) = dir.as_ref() {
            std::fs::create_dir_all(target_dir)?;
            let target = target_dir.join(&attachment.filename);
            std::fs::copy(&path, &target)?;
            target
        } else {
            path
        };
        println!(
            "#{} {} -> {}",
            display_index,
            attachment.filename,
            final_path.display()
        );
    }

    Ok(())
}

pub async fn attachments_open(message_id: String, index: usize) -> anyhow::Result<()> {
    let id = parse_id_for_legacy(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let attachments = load_attachments(&mut client, &id).await?;
    let attachment = attachment_by_index(&attachments, index)?;

    let path = request_attachment_file(
        &mut client,
        Request::OpenAttachment {
            message_id: id,
            attachment_id: attachment.id.clone(),
        },
    )
    .await?;
    println!("Opened {} ({})", attachment.filename, path.display());
    Ok(())
}
