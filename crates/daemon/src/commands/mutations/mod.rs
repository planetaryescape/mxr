mod attachments;
mod compose;
mod helpers;

pub use attachments::{attachments_download, attachments_list, attachments_open};
pub use compose::{compose, drafts, forward, reply, reply_all, send_draft, ComposeOptions};

use crate::ipc_client::IpcClient;
use crate::mxr_protocol::*;
use helpers::{
    confirm_action, parse_message_id, parse_snooze_until, print_selection_preview,
    requires_confirmation, resolve_mutation_selection, run_simple_mutation, MutationRunOptions,
};

// ---------------------------------------------------------------------------
// Simple mutations
// ---------------------------------------------------------------------------

pub async fn archive(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "archive",
            success_message: "Archived",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| Request::Mutation(MutationCommand::Archive { message_ids: ids }),
    )
    .await
}

pub async fn read_archive(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read and archive",
            success_message: "Marked as read and archived",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| Request::Mutation(MutationCommand::ReadAndArchive { message_ids: ids }),
    )
    .await
}

pub async fn trash(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "trash",
            success_message: "Trashed",
            yes,
            dry_run,
            destructive: true,
        },
        |ids| Request::Mutation(MutationCommand::Trash { message_ids: ids }),
    )
    .await
}

pub async fn spam(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as spam",
            success_message: "Marked as spam",
            yes,
            dry_run,
            destructive: true,
        },
        |ids| Request::Mutation(MutationCommand::Spam { message_ids: ids }),
    )
    .await
}

pub async fn star(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "star",
            success_message: "Starred",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: true,
            })
        },
    )
    .await
}

pub async fn unstar(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "unstar",
            success_message: "Unstarred",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Star {
                message_ids: ids,
                starred: false,
            })
        },
    )
    .await
}

pub async fn mark_read(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read",
            success_message: "Marked as read",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: true,
            })
        },
    )
    .await
}

pub async fn unread(
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as unread",
            success_message: "Marked as unread",
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: false,
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Label mutations
// ---------------------------------------------------------------------------

pub async fn label(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("add label '{name}'"),
            success_message: &format!("Added label '{name}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![name.clone()],
                remove: vec![],
            })
        },
    )
    .await
}

pub async fn unlabel(
    name: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("remove label '{name}'"),
            success_message: &format!("Removed label '{name}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::ModifyLabels {
                message_ids: ids,
                add: vec![],
                remove: vec![name.clone()],
            })
        },
    )
    .await
}

pub async fn move_msg(
    target_label: String,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("move to '{target_label}'"),
            success_message: &format!("Moved to '{target_label}'"),
            yes,
            dry_run,
            destructive: false,
        },
        |ids| {
            Request::Mutation(MutationCommand::Move {
                message_ids: ids,
                target_label: target_label.clone(),
            })
        },
    )
    .await
}

// ---------------------------------------------------------------------------
// Snooze
// ---------------------------------------------------------------------------

pub async fn snooze(
    message_id: Option<String>,
    until: String,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
) -> anyhow::Result<()> {
    let wake_at = parse_snooze_until(&until)?;
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        );
        return Ok(());
    }
    if requires_confirmation(false, selection.used_search, selection.ids.len(), yes) {
        confirm_action(
            &format!("snooze until {}", wake_at.to_rfc3339()),
            &selection,
        )?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!(
        "Snoozed {} message(s) until {}",
        selection.ids.len(),
        wake_at.to_rfc3339()
    );
    Ok(())
}

pub async fn unsnooze(message_id: Option<String>, all: bool) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    if all {
        let resp = client.request(Request::ListSnoozed).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            } => {
                if snoozed.is_empty() {
                    println!("No snoozed messages");
                    return Ok(());
                }
                for s in &snoozed {
                    let resp = client
                        .request(Request::Unsnooze {
                            message_id: s.message_id.clone(),
                        })
                        .await?;
                    match resp {
                        Response::Ok {
                            data: ResponseData::Ack,
                        } => {}
                        Response::Error { message } => {
                            eprintln!("Failed to unsnooze {}: {message}", s.message_id);
                        }
                        _ => {}
                    }
                }
                println!("Unsnoozed {} message(s)", snoozed.len());
            }
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    } else {
        let id_str = message_id.ok_or_else(|| anyhow::anyhow!("Provide a message ID or --all"))?;
        let id = parse_message_id(&id_str)?;
        let resp = client.request(Request::Unsnooze { message_id: id }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => println!("Unsnoozed"),
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    Ok(())
}

pub async fn snoozed() -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListSnoozed).await?;
    match resp {
        Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        } => {
            if snoozed.is_empty() {
                println!("No snoozed messages");
            } else {
                println!(
                    "{:<38} {:<25} {:<25}",
                    "MESSAGE ID", "SNOOZED AT", "WAKE AT"
                );
                println!("{}", "-".repeat(88));
                for s in &snoozed {
                    println!(
                        "{:<38} {:<25} {:<25}",
                        s.message_id.as_str(),
                        s.snoozed_at.to_rfc3339(),
                        s.wake_at.to_rfc3339(),
                    );
                }
                println!("\n{} snoozed message(s)", snoozed.len());
            }
        }
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsubscribe / Open
// ---------------------------------------------------------------------------

pub async fn unsubscribe(
    message_id: Option<String>,
    yes: bool,
    search: Option<String>,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_id, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_selection_preview("unsubscribe", &selection);
        return Ok(());
    }
    if requires_confirmation(true, selection.used_search, selection.ids.len(), yes) {
        confirm_action("unsubscribe", &selection)?;
    }
    for id in &selection.ids {
        let resp = client
            .request(Request::Unsubscribe {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => {}
            Response::Error { message } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    println!("Unsubscribed from {} message(s)", selection.ids.len());
    Ok(())
}

pub async fn open_in_browser(message_id: String) -> anyhow::Result<()> {
    let id = parse_message_id(&message_id)?;
    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::GetBody {
            message_id: id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            let dir = crate::mxr_config::data_dir().join("source");
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(format!("{}.txt", id.as_str()));

            let mut source = String::new();
            if let Some(headers) = body.metadata.raw_headers.as_deref() {
                source.push_str(headers.trim_end());
                source.push_str("\n\n");
            }
            if let Some(html) = body.text_html.as_deref() {
                source.push_str(html);
            } else if let Some(text) = body.text_plain.as_deref() {
                source.push_str(text);
            }

            std::fs::write(&path, source)?;
            let editor = crate::mxr_compose::editor::resolve_editor(None);
            std::process::Command::new(&editor).arg(&path).spawn()?;
            println!("Opened local source: {}", path.display());
        }
        Response::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}
