mod attachments;
mod compose;
mod helpers;

pub use attachments::{attachments_download, attachments_list, attachments_open};
pub use compose::{
    cancel_scheduled_send, check_send, compose, compose_check, drafts, drafts_discard,
    drafts_recover, drafts_resume, forward, reply, reply_all, schedule_send, send_draft,
    ComposeOptions, ForwardCommand, ReplyCommand,
};

/// CLI surface for `mxr undo <mutation_id>`.
///
/// Sends `Request::UndoMutation` and prints the success message or the
/// daemon's specific failure (NotFound / WindowExpired / Irreversible).
pub async fn undo(
    mutation_id: String,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if dry_run {
        print_undo_output(&mutation_id, true, format)?;
        return Ok(());
    }

    let mut client = IpcClient::connect().await?;
    let resp = client
        .request(Request::UndoMutation {
            mutation_id: mutation_id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => {
            print_undo_output(&mutation_id, false, format)?;
            Ok(())
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

use crate::cli::OutputFormat;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use helpers::{
    confirm_action, parse_snooze_until, print_batch_mutation_output, print_dry_run_output,
    requires_confirmation, resolve_mutation_selection, resolve_mutation_selection_with_limit,
    run_simple_mutation, BatchMutationError, MutationRunOptions,
};
use mxr_protocol::*;
use serde::Serialize;

const BROWSER_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Serialize)]
struct UndoOutput<'a> {
    mutation_id: &'a str,
    dry_run: bool,
    undone: bool,
}

fn print_undo_output(
    mutation_id: &str,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            if dry_run {
                println!("Would undo mutation {mutation_id}");
            } else {
                println!("Undone");
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&UndoOutput {
                mutation_id,
                dry_run,
                undone: !dry_run,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&UndoOutput {
                mutation_id,
                dry_run,
                undone: !dry_run,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["mutation_id", "dry_run", "undone"])?;
            writer.write_record([
                mutation_id,
                if dry_run { "true" } else { "false" },
                if dry_run { "false" } else { "true" },
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{mutation_id}"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Simple mutations
// ---------------------------------------------------------------------------

pub async fn archive(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let address = address_positional(&message_ids, search.as_deref())?;
    let selection = if let Some(address) = address {
        resolve_mutation_selection_with_limit(
            &mut client,
            Vec::new(),
            Some(format!("from:{address}")),
            crate::commands::selection::SelectionLimit::First,
        )
        .await?
    } else {
        resolve_mutation_selection(&mut client, message_ids, search).await?
    };
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "archive",
            success_message: "Archived",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| Request::mutation(MutationCommand::Archive { message_ids: ids }),
    )
    .await
}

pub async fn read_archive(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read and archive",
            success_message: "Marked as read and archived",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| Request::mutation(MutationCommand::ReadAndArchive { message_ids: ids }),
    )
    .await
}

pub async fn trash(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "trash",
            success_message: "Trashed",
            yes,
            dry_run,
            format,
            destructive: true,
        },
        |ids| Request::mutation(MutationCommand::Trash { message_ids: ids }),
    )
    .await
}

pub async fn spam(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as spam",
            success_message: "Marked as spam",
            yes,
            dry_run,
            format,
            destructive: true,
        },
        |ids| Request::mutation(MutationCommand::Spam { message_ids: ids }),
    )
    .await
}

pub async fn star(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "star",
            success_message: "Starred",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::Star {
                message_ids: ids,
                starred: true,
            })
        },
    )
    .await
}

pub async fn unstar(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "unstar",
            success_message: "Unstarred",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::Star {
                message_ids: ids,
                starred: false,
            })
        },
    )
    .await
}

pub async fn mark_read(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as read",
            success_message: "Marked as read",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::SetRead {
                message_ids: ids,
                read: true,
            })
        },
    )
    .await
}

pub async fn unread(
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: "mark as unread",
            success_message: "Marked as unread",
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::SetRead {
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
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("add label '{name}'"),
            success_message: &format!("Added label '{name}'"),
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::ModifyLabels {
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
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("remove label '{name}'"),
            success_message: &format!("Removed label '{name}'"),
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::ModifyLabels {
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
    message_ids: Vec<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &format!("move to '{target_label}'"),
            success_message: &format!("Moved to '{target_label}'"),
            yes,
            dry_run,
            format,
            destructive: false,
        },
        |ids| {
            Request::mutation(MutationCommand::Move {
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
    message_ids: Vec<String>,
    until: String,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let wake_at = parse_snooze_until(&until)?;
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    let action = format!("snooze until {}", wake_at.to_rfc3339());
    if dry_run {
        print_dry_run_output(&action, &selection, format)?;
        return Ok(());
    }
    if requires_confirmation(false, selection.used_search, selection.ids.len(), yes) {
        confirm_action(&action, &selection)?;
    }
    let mut succeeded = 0usize;
    let mut errors = Vec::new();
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
            } => succeeded += 1,
            Response::Error { message, .. } => {
                eprintln!("Skipped {} ({message})", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str().clone(),
                    error: message,
                });
            }
            _ => {
                eprintln!("Skipped {} (unexpected response)", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str().clone(),
                    error: "unexpected response".to_owned(),
                });
            }
        }
    }
    print_batch_mutation_output(
        "snooze",
        false,
        &format!(
            "Snoozed {} message(s) until {}",
            succeeded,
            wake_at.to_rfc3339()
        ),
        &selection.ids,
        succeeded,
        errors.clone(),
        format,
    )?;
    if succeeded == 0 {
        anyhow::bail!("No messages snoozed ({} failed)", errors.len());
    }
    Ok(())
}

fn address_positional<'a>(
    message_ids: &'a [String],
    search: Option<&str>,
) -> anyhow::Result<Option<&'a str>> {
    if search.is_some() || message_ids.is_empty() {
        return Ok(None);
    }
    if message_ids.len() > 1 {
        return Ok(None);
    }
    let candidate = message_ids[0].trim();
    if candidate.contains('@') {
        Ok(Some(candidate))
    } else {
        Ok(None)
    }
}

pub async fn unsnooze(
    message_ids: Vec<String>,
    all: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    if all {
        let resp = client.request(Request::ListSnoozed).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            } => {
                let ids: Vec<_> = snoozed.iter().map(|s| s.message_id.clone()).collect();
                if snoozed.is_empty() {
                    print_batch_mutation_output(
                        "unsnooze",
                        dry_run,
                        "No snoozed messages",
                        &ids,
                        0,
                        Vec::new(),
                        format,
                    )?;
                    return Ok(());
                }
                if dry_run {
                    if matches!(resolve_format(format.clone()), OutputFormat::Table) {
                        println!("DRY-RUN — would unsnooze {} message(s):", snoozed.len());
                        for s in &snoozed {
                            println!("  {} (wakes {})", s.message_id, s.wake_at.to_rfc3339());
                        }
                    } else {
                        print_batch_mutation_output(
                            "unsnooze",
                            true,
                            &format!("Would unsnooze {} message(s)", ids.len()),
                            &ids,
                            0,
                            Vec::new(),
                            format,
                        )?;
                    }
                    return Ok(());
                }
                let mut succeeded = 0usize;
                let mut errors = Vec::new();
                for s in &snoozed {
                    let resp = client
                        .request(Request::Unsnooze {
                            message_id: s.message_id.clone(),
                        })
                        .await?;
                    match resp {
                        Response::Ok {
                            data: ResponseData::Ack,
                        } => succeeded += 1,
                        Response::Error { message, .. } => {
                            eprintln!("Failed to unsnooze {}: {message}", s.message_id);
                            errors.push(BatchMutationError {
                                message_id: s.message_id.as_str().clone(),
                                error: message,
                            });
                        }
                        _ => {
                            eprintln!("Failed to unsnooze {}: unexpected response", s.message_id);
                            errors.push(BatchMutationError {
                                message_id: s.message_id.as_str().clone(),
                                error: "unexpected response".to_owned(),
                            });
                        }
                    }
                }
                print_batch_mutation_output(
                    "unsnooze",
                    false,
                    &format!("Unsnoozed {succeeded} message(s)"),
                    &ids,
                    succeeded,
                    errors.clone(),
                    format,
                )?;
                if succeeded == 0 && !errors.is_empty() {
                    anyhow::bail!("No messages unsnoozed ({} failed)", errors.len());
                }
            }
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    } else {
        let selection = resolve_mutation_selection(&mut client, message_ids, None).await?;
        if selection.ids.is_empty() {
            anyhow::bail!("No messages matched");
        }
        if dry_run {
            print_dry_run_output("unsnooze", &selection, format)?;
            return Ok(());
        }
        let mut succeeded = 0usize;
        let mut errors = Vec::new();
        for id in &selection.ids {
            let resp = client
                .request(Request::Unsnooze {
                    message_id: id.clone(),
                })
                .await?;
            match resp {
                Response::Ok {
                    data: ResponseData::Ack,
                } => succeeded += 1,
                Response::Error { message, .. } => {
                    errors.push(BatchMutationError {
                        message_id: id.as_str().clone(),
                        error: message,
                    });
                }
                _ => errors.push(BatchMutationError {
                    message_id: id.as_str().clone(),
                    error: "unexpected response".to_owned(),
                }),
            }
        }
        print_batch_mutation_output(
            "unsnooze",
            false,
            &format!("Unsnoozed {succeeded} message(s)"),
            &selection.ids,
            succeeded,
            errors.clone(),
            format,
        )?;
        if succeeded == 0 {
            anyhow::bail!("No messages unsnoozed ({} failed)", errors.len());
        }
    }
    Ok(())
}

pub async fn snoozed(format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = client.request(Request::ListSnoozed).await?;
    match resp {
        Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        } => match resolve_format(format) {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&snoozed)?),
            OutputFormat::Jsonl => println!("{}", jsonl(&snoozed)?),
            OutputFormat::Csv => {
                let mut writer = csv::Writer::from_writer(Vec::new());
                writer.write_record(["message_id", "account_id", "snoozed_at", "wake_at"])?;
                for s in &snoozed {
                    writer.write_record([
                        s.message_id.as_str(),
                        s.account_id.as_str(),
                        s.snoozed_at.to_rfc3339(),
                        s.wake_at.to_rfc3339(),
                    ])?;
                }
                println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
            }
            OutputFormat::Ids => {
                for s in &snoozed {
                    println!("{}", s.message_id);
                }
            }
            OutputFormat::Table => {
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
        },
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsubscribe / Open
// ---------------------------------------------------------------------------

pub async fn unsubscribe(
    message_ids: Vec<String>,
    yes: bool,
    search: Option<String>,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let (message_ids, search) = rewrite_unsubscribe_positional(message_ids, search);
    let mut client = IpcClient::connect().await?;
    let selection = resolve_mutation_selection(&mut client, message_ids, search).await?;
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if dry_run {
        print_dry_run_output("unsubscribe", &selection, format)?;
        return Ok(());
    }
    if requires_confirmation(true, selection.used_search, selection.ids.len(), yes) {
        confirm_action("unsubscribe", &selection)?;
    }
    let mut succeeded = 0usize;
    let mut errors = Vec::new();
    for id in &selection.ids {
        let resp = client
            .request(Request::Unsubscribe {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Ack,
            } => succeeded += 1,
            Response::Error { message, .. } => {
                eprintln!("Skipped {} ({message})", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str().clone(),
                    error: message,
                });
            }
            _ => {
                eprintln!("Skipped {} (unexpected response)", id.as_str());
                errors.push(BatchMutationError {
                    message_id: id.as_str().clone(),
                    error: "unexpected response".to_owned(),
                });
            }
        }
    }
    print_batch_mutation_output(
        "unsubscribe",
        false,
        &format!("Unsubscribed from {succeeded} message(s)"),
        &selection.ids,
        succeeded,
        errors.clone(),
        format,
    )?;
    if succeeded == 0 {
        anyhow::bail!("No messages unsubscribed ({} failed)", errors.len());
    }
    Ok(())
}

/// Translate positional `EMAIL_ADDRESS` arguments to `mxr unsubscribe`
/// into a synthetic `--search "from:<addr>"` query, so that
/// `mxr unsubscribe alice@example.com` does what a user expects
/// (unsubscribe from that sender's most recent message).
///
/// Positional arguments that look like message IDs (no `@`) are left
/// alone; an explicit `--search` flag also wins, in which case the
/// positional addresses are merged into the same query as additional
/// `from:` terms.
fn rewrite_unsubscribe_positional(
    positional: Vec<String>,
    search: Option<String>,
) -> (Vec<String>, Option<String>) {
    let mut ids = Vec::with_capacity(positional.len());
    let mut addresses = Vec::with_capacity(positional.len());
    for arg in positional {
        if looks_like_email(&arg) {
            addresses.push(arg);
        } else {
            ids.push(arg);
        }
    }
    if addresses.is_empty() {
        return (ids, search);
    }
    let addr_query = addresses
        .iter()
        .map(|addr| format!("from:{addr}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let combined = match search {
        Some(existing) if !existing.trim().is_empty() => {
            // Combine the user's explicit search with the synthetic
            // address query. AND-style: both must match.
            format!("({existing}) AND ({addr_query})")
        }
        _ => addr_query,
    };
    (ids, Some(combined))
}

fn looks_like_email(value: &str) -> bool {
    // Conservative: must contain exactly one `@`, have at least one
    // character on each side, and contain at least one dot in the
    // domain. We're not validating RFC 5322 here, just disambiguating
    // a message-id (often a UUID/hash) from a sender address.
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if value.matches('@').count() != 1 {
        return false;
    }
    domain.contains('.')
}

pub async fn open_in_browser(
    message_id: Option<String>,
    search: Option<String>,
    first: bool,
    limit: Option<u32>,
    yes: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let ids = crate::commands::selection::resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        crate::commands::selection::SelectionLimit::from_flags(first, limit),
    )
    .await?;
    if ids.is_empty() {
        anyhow::bail!("No messages matched");
    }
    if ids.len() > 1 && !yes {
        anyhow::bail!(
            "{} messages matched — pass `--yes` to open all of them in your browser",
            ids.len()
        );
    }

    let dir = browser_cache_dir();
    for id in &ids {
        let path = browser_cache_path(&dir, id);
        if path.exists() {
            open_browser_path(&path)?;
            maybe_cleanup_browser_cache(&dir, &path);
            println!("Opened in browser: {}", path.display());
            continue;
        }

        let resp = client
            .request(Request::GetBody {
                message_id: id.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Body { body },
            } => {
                let Some(document) = browser_document(&body) else {
                    anyhow::bail!("No readable body available for {}", id.as_str());
                };
                ensure_browser_cache_file(&dir, id, &document)?;
                open_browser_path(&path)?;
                maybe_cleanup_browser_cache(&dir, &path);
                println!("Opened in browser: {}", path.display());
            }
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response"),
        }
    }
    Ok(())
}

fn browser_cache_dir() -> std::path::PathBuf {
    mxr_config::data_dir().join("browser")
}

fn browser_cache_path(
    dir: &std::path::Path,
    message_id: &mxr_core::MessageId,
) -> std::path::PathBuf {
    dir.join(format!("{}.html", message_id.as_str()))
}

fn ensure_browser_cache_file(
    dir: &std::path::Path,
    message_id: &mxr_core::MessageId,
    document: &str,
) -> anyhow::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = browser_cache_path(dir, message_id);
    if path.exists() {
        return Ok(path);
    }

    std::fs::write(&path, document)?;
    Ok(path)
}

fn maybe_cleanup_browser_cache(dir: &std::path::Path, keep_path: &std::path::Path) {
    match cleanup_browser_cache_dir(dir, keep_path, std::time::SystemTime::now()) {
        Ok(removed) if removed > 0 => {
            tracing::debug!(removed, keep = %keep_path.display(), "browser cache cleanup removed stale files");
        }
        Ok(_) => {}
        Err(error) => {
            tracing::debug!(
                dir = %dir.display(),
                keep = %keep_path.display(),
                error = %error,
                "browser cache cleanup failed"
            );
        }
    }
}

fn cleanup_browser_cache_dir(
    dir: &std::path::Path,
    keep_path: &std::path::Path,
    now: std::time::SystemTime,
) -> std::io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::debug!(dir = %dir.display(), error = %error, "browser cache cleanup skipped unreadable entry");
                continue;
            }
        };

        let path = entry.path();
        if path == keep_path {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("html") {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::debug!(path = %path.display(), error = %error, "browser cache cleanup skipped entry without metadata");
                continue;
            }
        };
        if !is_browser_cache_stale(&metadata, now) {
            continue;
        }

        match std::fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) => {
                tracing::debug!(path = %path.display(), error = %error, "browser cache cleanup failed to remove stale file");
            }
        }
    }
    Ok(removed)
}

fn is_browser_cache_stale(metadata: &std::fs::Metadata, now: std::time::SystemTime) -> bool {
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(age) = now.duration_since(modified) else {
        return false;
    };
    age > BROWSER_CACHE_TTL
}

fn open_browser_path(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let program = "open";

    let status = std::process::Command::new(program).arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{program} exited with status {status}")
    }
}

fn browser_document(body: &mxr_core::MessageBody) -> Option<String> {
    body.text_html
        .clone()
        .or_else(|| {
            body.text_plain
                .as_deref()
                .map(render_plain_text_browser_document)
        })
        .or_else(|| {
            body.best_effort_readable_summary()
                .map(|text| render_plain_text_browser_document(&text))
        })
}

fn render_plain_text_browser_document(text: &str) -> String {
    let escaped = htmlescape::encode_minimal(text);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>mxr message</title><style>body{{margin:2rem;font:16px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:#fafafa;color:#111;}}pre{{white-space:pre-wrap;word-break:break-word;}}</style></head><body><pre>{escaped}</pre></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration as StdDuration, SystemTime};

    /// Phase 2.6: `mxr unsubscribe alice@example.com` is the shape a
    /// user will reach for. We rewrite the positional address into a
    /// `from:` search query so the existing search→mutate pipeline does
    /// the rest. The address must NOT survive as a message-id, which
    /// the daemon would then fail to resolve.
    #[test]
    fn unsubscribe_positional_address_becomes_from_search() {
        let (ids, search) = rewrite_unsubscribe_positional(vec!["alice@example.com".into()], None);

        assert!(ids.is_empty(), "address positional is consumed");
        assert_eq!(search.as_deref(), Some("from:alice@example.com"));
    }

    /// Two addresses (or more) combine with `OR` so the user can
    /// unsubscribe from multiple senders in one invocation. This is the
    /// power-user shape; same call as Gmail's bulk-select but keyboard-
    /// driven.
    #[test]
    fn unsubscribe_multiple_addresses_combine_with_or() {
        let (ids, search) = rewrite_unsubscribe_positional(
            vec!["alice@example.com".into(), "bob@example.com".into()],
            None,
        );

        assert!(ids.is_empty());
        assert_eq!(
            search.as_deref(),
            Some("from:alice@example.com OR from:bob@example.com")
        );
    }

    /// Message-ID-shaped positionals (no `@` / no domain dot) are left
    /// untouched. The address shorthand is additive; it must not
    /// regress the existing `mxr unsubscribe <MESSAGE_ID>` shape.
    #[test]
    fn unsubscribe_message_id_positionals_pass_through_unchanged() {
        let (ids, search) =
            rewrite_unsubscribe_positional(vec!["abc-123".into(), "xyz-789".into()], None);

        assert_eq!(ids, vec!["abc-123".to_string(), "xyz-789".to_string()]);
        assert!(search.is_none(), "no synthesized search for plain ids");
    }

    /// A user-supplied `--search` and a positional address combine via
    /// `AND` so the user can scope an address-wide unsubscribe to a
    /// label or date range (e.g. `--search "label:promos"` +
    /// `marketing@x.com`).
    #[test]
    fn unsubscribe_positional_address_intersects_explicit_search() {
        let (ids, search) = rewrite_unsubscribe_positional(
            vec!["marketing@example.com".into()],
            Some("label:promos".into()),
        );

        assert!(ids.is_empty());
        assert_eq!(
            search.as_deref(),
            Some("(label:promos) AND (from:marketing@example.com)")
        );
    }

    /// Addresses without a domain dot (`foo@bar`) — including the dummy
    /// addresses the test fixtures sometimes use — should not be
    /// classified as email; that would silently turn a typo'd ID into a
    /// no-match search. The fallback path errs on the side of "treat as
    /// id and let the daemon raise No-Match if needed".
    #[test]
    fn unsubscribe_local_only_handle_is_not_treated_as_email() {
        let (ids, search) = rewrite_unsubscribe_positional(vec!["foo@bar".into()], None);

        assert_eq!(ids, vec!["foo@bar".to_string()]);
        assert!(search.is_none());
    }

    /// A bare `@`-less message id is plainly not an email even if it
    /// looks UUID-ish. Belt-and-braces.
    #[test]
    fn unsubscribe_uuid_shaped_id_is_not_an_email() {
        assert!(!looks_like_email("8c9d3a02-2e9a-4af6-bf80-4b0d7bb6a4e2"));
        assert!(looks_like_email("alice@example.com"));
        assert!(!looks_like_email("@example.com"));
        assert!(!looks_like_email("alice@"));
    }

    #[test]
    fn browser_document_prefers_html_when_available() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: Some("plain".into()),
            text_html: Some("<p>html</p>".into()),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        };

        assert_eq!(browser_document(&body).as_deref(), Some("<p>html</p>"));
    }

    #[test]
    fn browser_document_wraps_plain_text_for_browser_rendering() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: Some("hello <world>\nline 2".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        };

        let document = browser_document(&body).expect("plain text fallback");
        assert!(document.contains("<pre>hello &lt;world&gt;\nline 2</pre>"));
        assert!(document.contains("<!doctype html>"));
    }

    #[test]
    fn browser_document_wraps_best_effort_fallback_for_browser_rendering() {
        let body = mxr_core::MessageBody {
            message_id: mxr_core::MessageId::new(),
            text_plain: None,
            text_html: None,
            attachments: vec![mxr_core::AttachmentMeta {
                id: mxr_core::AttachmentId::new(),
                message_id: mxr_core::MessageId::new(),
                filename: "invite.ics".into(),
                mime_type: "text/calendar".into(),
                disposition: mxr_core::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::MessageMetadata {
                calendar: Some(mxr_core::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Demo call".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        };

        let document = browser_document(&body).expect("best-effort fallback");
        assert!(document.contains("Calendar invite"));
        assert!(document.contains("Summary: Demo call"));
        assert!(document.contains("<!doctype html>"));
    }

    #[test]
    fn ensure_browser_cache_file_reuses_existing_file_without_overwriting() {
        let dir = unique_browser_test_dir("reuse");
        std::fs::create_dir_all(&dir).expect("create browser cache dir");
        let message_id = mxr_core::MessageId::new();
        let path = browser_cache_path(&dir, &message_id);
        std::fs::write(&path, "cached").expect("write cached browser file");

        let reused = ensure_browser_cache_file(&dir, &message_id, "new content")
            .expect("reuse cached browser file");

        assert_eq!(reused, path);
        assert_eq!(
            std::fs::read_to_string(&path).expect("read cached browser file"),
            "cached"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_browser_cache_dir_removes_stale_html_only() {
        let dir = unique_browser_test_dir("cleanup");
        std::fs::create_dir_all(&dir).expect("create browser cache dir");

        let keep_path = dir.join("keep.html");
        let stale_html = dir.join("stale.html");
        let fresh_html = dir.join("fresh.html");
        let stale_text = dir.join("stale.txt");
        for path in [&keep_path, &stale_html, &fresh_html, &stale_text] {
            std::fs::write(path, path.display().to_string()).expect("write browser cache fixture");
        }

        let now = SystemTime::now();
        set_modified_time(
            &keep_path,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );
        set_modified_time(
            &stale_html,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );
        set_modified_time(&fresh_html, now - StdDuration::from_secs(60));
        set_modified_time(
            &stale_text,
            now - BROWSER_CACHE_TTL - StdDuration::from_secs(60),
        );

        let removed =
            cleanup_browser_cache_dir(&dir, &keep_path, now).expect("cleanup browser cache dir");

        assert_eq!(removed, 1, "only one stale html file should be removed");
        assert!(
            keep_path.exists(),
            "currently opened file must be preserved"
        );
        assert!(!stale_html.exists(), "stale html should be removed");
        assert!(fresh_html.exists(), "fresh html should be kept");
        assert!(stale_text.exists(), "non-html files should be ignored");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unique_browser_test_dir(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mxr-daemon-browser-cache-{label}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn set_modified_time(path: &std::path::Path, modified: SystemTime) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open browser cache fixture");
        let times = std::fs::FileTimes::new().set_modified(modified);
        file.set_times(times)
            .expect("set browser cache fixture mtime");
    }
}
