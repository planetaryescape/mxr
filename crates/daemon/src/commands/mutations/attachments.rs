use crate::ipc_client::IpcClient;
use mxr_protocol::*;
use std::path::PathBuf;

use super::helpers::{
    attachment_by_index, format_bytes, load_attachments, parse_message_id, request_attachment_file,
};

pub async fn attachments_list(message_id: String) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
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
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

pub async fn attachments_download(
    message_id: String,
    index: Option<usize>,
    dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
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
    let id = parse_message_id(&message_id)?;
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
