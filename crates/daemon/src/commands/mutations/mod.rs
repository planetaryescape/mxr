mod archive;
pub use archive::archive_operation;
mod attachments;
mod compose;
mod compose_html;
mod helpers;
mod unsubscribe_address;

pub use attachments::{attachments_download, attachments_list, attachments_open};
pub use compose::{
    cancel_scheduled_send, check_send, compose, compose_check, drafts, drafts_audience,
    drafts_discard, drafts_edit, drafts_push, drafts_recover, drafts_reply, drafts_resume,
    drafts_scheduled, forward, recover_send_outcome, reply, reply_all, schedule_send, send_draft,
    send_outcome, ComposeOptions, ForwardCommand, ReplyCommand, SavedSendReview,
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
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use helpers::{
    confirm_action, print_batch_mutation_output, print_dry_run_output, requires_confirmation,
    resolve_mutation_selection, resolve_mutation_selection_with_limit, run_simple_mutation,
    selection_counts, BatchMutationError, MutationRunOptions, MutationSelection,
};
use mxr_core::types::UnsubscribeMethod;
use mxr_protocol::*;
use serde::Serialize;

const BROWSER_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Serialize)]
struct UndoOutput<'a> {
    mutation_id: &'a str,
    dry_run: bool,
    undone: bool,
}

#[derive(Serialize)]
struct JobsOutput {
    jobs: Vec<JobData>,
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

pub async fn jobs(job_id: Option<String>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let resp = match job_id {
        Some(job_id) => client.request(Request::GetJob { job_id }).await?,
        None => client.request(Request::ListJobs).await?,
    };
    match resp {
        Response::Ok {
            data: ResponseData::Job { job },
        } => print_jobs_output(vec![job], format),
        Response::Ok {
            data: ResponseData::Jobs { jobs },
        } => print_jobs_output(jobs, format),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn print_jobs_output(jobs: Vec<JobData>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            if jobs.is_empty() {
                println!("No jobs");
                return Ok(());
            }
            println!(
                "{:<36}  {:<10}  {:<22}  {:>9}  undo",
                "job_id", "status", "kind", "progress"
            );
            for job in jobs {
                println!(
                    "{:<36}  {:<10}  {:<22}  {:>4}/{:<4}  {}{}",
                    job.job_id,
                    format!("{:?}", job.status).to_ascii_lowercase(),
                    job.kind,
                    job.progress.completed,
                    job.progress.total,
                    job.undo_ids.join(","),
                    job.error
                        .as_ref()
                        .map(|error| format!("  error={error}"))
                        .unwrap_or_default()
                );
            }
        }
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&JobsOutput { jobs })?),
        OutputFormat::Jsonl => println!("{}", jsonl(&jobs)?),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "job_id",
                "kind",
                "status",
                "total",
                "completed",
                "succeeded",
                "skipped",
                "failed",
                "undo_ids",
                "error",
            ])?;
            for job in jobs {
                writer.write_record([
                    job.job_id,
                    job.kind,
                    format!("{:?}", job.status).to_ascii_lowercase(),
                    job.progress.total.to_string(),
                    job.progress.completed.to_string(),
                    job.progress.succeeded.to_string(),
                    job.progress.skipped.to_string(),
                    job.progress.failed.to_string(),
                    job.undo_ids.join(","),
                    job.error.unwrap_or_default(),
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for job in jobs {
                println!("{}", job.job_id);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Simple mutations
// ---------------------------------------------------------------------------

pub async fn archive(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    if matches!(resolve_format(format.clone()), OutputFormat::Csv) {
        anyhow::bail!("Archive receipts support table, json, jsonl and ids; select a supported format before submission");
    }
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let address = address_positional(&message_ids, search.as_deref())?;
    let selection = if let Some(address) = address {
        resolve_mutation_selection_with_limit(
            &mut client,
            Vec::new(),
            Some(format!("from:{address}")),
            account_id.as_ref(),
            crate::commands::selection::SelectionLimit::First,
        )
        .await?
    } else {
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?
    };
    let operation = client
        .preview_archive(MutationCommand::Archive {
            message_ids: selection.ids.clone(),
        })
        .await?;
    if dry_run {
        return archive::print_operation(&operation, format);
    }
    if requires_confirmation(false, selection.used_search, selection.ids.len(), yes) {
        confirm_action("archive", &selection)?;
    }
    let operation_id = operation.identity.operation_id.clone();
    let result = client.submit_archive(&operation, !async_job).await;
    match result {
        Ok(operation) => {
            archive::print_operation(&operation, format)?;
            if !async_job {
                archive::require_confirmed_completion(&operation)?;
            }
            Ok(())
        }
        Err(error) => {
            archive::print_recovery_error(&operation, &error, format)?;
            anyhow::bail!("Archive response lost or refused: {error}. Recover with mxr archive-operation {operation_id}; do not create another operation")
        }
    }
}

pub async fn read_archive(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
        },
        |ids| Request::mutation(MutationCommand::ReadAndArchive { message_ids: ids }),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI route command mirrors user-facing flags"
)]
pub async fn route(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    to_label: String,
    from_queue_label: String,
    archive: bool,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
    let action = if archive {
        format!("route to {to_label}, remove {from_queue_label}, mark read and archive")
    } else {
        format!("route to {to_label}, remove {from_queue_label}")
    };
    run_simple_mutation(
        &mut client,
        selection,
        MutationRunOptions {
            action: &action,
            success_message: "Routed",
            yes,
            dry_run,
            format,
            destructive: false,
            async_job: false,
        },
        |ids| {
            Request::mutation(MutationCommand::Route {
                message_ids: ids,
                to_label: to_label.clone(),
                from_queue_label: from_queue_label.clone(),
                archive,
                dry_run: false,
            })
        },
    )
    .await
}

pub async fn trash(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
        },
        |ids| Request::mutation(MutationCommand::Trash { message_ids: ids }),
    )
    .await
}

pub async fn spam(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
        },
        |ids| Request::mutation(MutationCommand::Spam { message_ids: ids }),
    )
    .await
}

pub async fn star(
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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

#[allow(clippy::too_many_arguments)]
pub async fn label(
    name: String,
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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

#[allow(clippy::too_many_arguments)]
pub async fn unlabel(
    name: String,
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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

#[allow(clippy::too_many_arguments)]
pub async fn move_msg(
    target_label: String,
    message_ids: Vec<String>,
    search: Option<String>,
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
    async_job: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
            async_job,
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
    account: Option<String>,
    yes: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let wake_at = crate::commands::time::parse_time_arg(&until, chrono::Utc::now())?;
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
        Some(selection_counts(&selection)),
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
    account: Option<String>,
    all: bool,
    dry_run: bool,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    if all {
        let resp = client.request(Request::ListSnoozed).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            } => {
                let snoozed: Vec<_> = snoozed
                    .into_iter()
                    .filter(|s| {
                        account_id
                            .as_ref()
                            .is_none_or(|account_id| &s.account_id == account_id)
                    })
                    .collect();
                let ids: Vec<_> = snoozed.iter().map(|s| s.message_id.clone()).collect();
                if snoozed.is_empty() {
                    print_batch_mutation_output(
                        "unsnooze",
                        dry_run,
                        "No snoozed messages",
                        &ids,
                        0,
                        Vec::new(),
                        None,
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
                            None,
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
                    None,
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
        let selection =
            resolve_mutation_selection(&mut client, message_ids, None, account_id.as_ref()).await?;
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
            Some(selection_counts(&selection)),
            format,
        )?;
        if succeeded == 0 {
            anyhow::bail!("No messages unsnoozed ({} failed)", errors.len());
        }
    }
    Ok(())
}

pub async fn snoozed(account: Option<String>, format: Option<OutputFormat>) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let resp = client.request(Request::ListSnoozed).await?;
    match resp {
        Response::Ok {
            data: ResponseData::SnoozedMessages { snoozed },
        } => {
            let snoozed: Vec<_> = snoozed
                .into_iter()
                .filter(|s| {
                    account_id
                        .as_ref()
                        .is_none_or(|account_id| &s.account_id == account_id)
                })
                .collect();
            match resolve_format(format) {
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
            }
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unsubscribe / Open
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub async fn unsubscribe(
    message_ids: Vec<String>,
    yes: bool,
    search: Option<String>,
    account: Option<String>,
    dry_run: bool,
    purge: bool,
    archive_on_no_method: bool,
    keep_mail: bool,
    preview_token: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    // Classified before connecting, so a mixed command fails before the
    // daemon is asked for anything. Purge has its own argument rule.
    let addresses = if purge {
        Vec::new()
    } else {
        unsubscribe_addresses(&message_ids)?
    };
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    if purge {
        return unsubscribe_purge(
            &mut client,
            message_ids,
            account_id,
            yes,
            dry_run,
            archive_on_no_method,
            !keep_mail,
            preview_token,
            format,
        )
        .await;
    }

    if !addresses.is_empty() {
        return unsubscribe_address::unsubscribe_by_address(
            &mut client,
            &addresses,
            search.as_deref(),
            account_id.as_ref(),
            yes,
            dry_run,
            format,
        )
        .await;
    }
    let selection =
        resolve_mutation_selection(&mut client, message_ids, search, account_id.as_ref()).await?;
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
        Some(selection_counts(&selection)),
        format,
    )?;
    if succeeded == 0 {
        anyhow::bail!("No messages unsubscribed ({} failed)", errors.len());
    }
    Ok(())
}

#[derive(Serialize)]
struct UnsubscribePurgeOutput<'a> {
    address: &'a str,
    query: &'a str,
    dry_run: bool,
    method: String,
    status: UnsubscribePurgeStatusData,
    message_count: u32,
    archived_count: u32,
    mutation_id: Option<&'a str>,
    error: Option<&'a str>,
    preview_token: Option<&'a str>,
}

#[allow(clippy::too_many_arguments)]
async fn unsubscribe_purge(
    client: &mut IpcClient,
    message_ids: Vec<String>,
    account_id: Option<mxr_core::AccountId>,
    yes: bool,
    dry_run: bool,
    archive_on_no_method: bool,
    archive: bool,
    preview_token: Option<String>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    if message_ids.len() != 1 || !looks_like_email(&message_ids[0]) {
        anyhow::bail!("--purge requires exactly one sender email address, e.g. mxr unsubscribe sender@example.com --purge");
    }
    let address = message_ids[0].clone();
    if let Some(token) = preview_token {
        let result = request_unsubscribe_purge(
            client,
            &address,
            account_id,
            false,
            archive_on_no_method,
            archive,
            Some(token),
        )
        .await?;
        print_unsubscribe_purge_output(&result, format)?;
        return check_unsubscribe_purge_result(result);
    }
    let preview = request_unsubscribe_purge(
        client,
        &address,
        account_id.clone(),
        true,
        archive_on_no_method,
        archive,
        None,
    )
    .await?;
    print_unsubscribe_purge_output(&preview, format.clone())?;
    if dry_run {
        return Ok(());
    }
    if preview.message_count == 0 {
        anyhow::bail!("No messages matched sender {address}");
    }
    if matches!(preview.method, UnsubscribeMethod::None) && !archive_on_no_method {
        if !archive {
            anyhow::bail!("No unsubscribe method available for {address}; existing mail kept");
        }
        anyhow::bail!("No unsubscribe method available for {}; rerun with --archive-on-no-method to read-archive {} message(s)", address, preview.message_count);
    }
    if requires_confirmation(false, true, preview.message_count as usize, yes) {
        confirm_action(
            &format!(
                "{} sender {} (method: {})",
                if archive {
                    "unsubscribe and read-archive"
                } else {
                    "unsubscribe from"
                },
                address,
                unsubscribe_method_text(&preview.method)
            ),
            &MutationSelection {
                ids: preview.message_ids.clone(),
                envelopes: Vec::new(),
                used_search: true,
            },
        )?;
    }
    let result = request_unsubscribe_purge(
        client,
        &address,
        account_id,
        false,
        archive_on_no_method,
        archive,
        preview.preview_token,
    )
    .await?;
    print_unsubscribe_purge_output(&result, format)?;
    check_unsubscribe_purge_result(result)
}

fn check_unsubscribe_purge_result(result: UnsubscribePurgeResultData) -> anyhow::Result<()> {
    if matches!(
        result.status,
        UnsubscribePurgeStatusData::Failed | UnsubscribePurgeStatusData::NoMethod
    ) {
        anyhow::bail!(
            "{}",
            result
                .error
                .unwrap_or_else(|| "unsubscribe purge failed".into())
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn request_unsubscribe_purge(
    client: &mut IpcClient,
    address: &str,
    account_id: Option<mxr_core::AccountId>,
    dry_run: bool,
    archive_on_no_method: bool,
    archive: bool,
    preview_token: Option<String>,
) -> anyhow::Result<UnsubscribePurgeResultData> {
    let request = if dry_run {
        Request::UnsubscribePurge {
            address: address.to_string(),
            account_id,
            dry_run: true,
            archive_on_no_method,
            preview_token: None,
        }
    } else {
        let preview_token = preview_token.ok_or_else(|| {
            anyhow::Error::msg(
                "Daemon did not provide an unsubscribe preview token; upgrade mxr and preview again",
            )
        })?;
        Request::CommitUnsubscribePreview {
            address: address.to_string(),
            account_id,
            archive,
            archive_on_no_method,
            preview_token,
        }
    };
    let response = if dry_run {
        client.request(request).await?
    } else {
        // Large sender footprints can exceed the default 120s IPC timeout;
        // use the long-running request path so the daemon can complete and
        // return the single undo id for the read+archive batch.
        client.request_with_events(request, |_| {}).await?
    };
    match response {
        Response::Ok {
            data: ResponseData::UnsubscribePurgeResult { result },
        } => Ok(result),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

fn print_unsubscribe_purge_output(
    result: &UnsubscribePurgeResultData,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            let method = unsubscribe_method_text(&result.method);
            if result.dry_run {
                println!(
                    "DRY-RUN — would unsubscribe and read-archive {} message(s) from {}",
                    result.message_count, result.address
                );
                println!("Method: {method}");
                if matches!(result.method, UnsubscribeMethod::None) {
                    println!("No usable List-Unsubscribe method; use --archive-on-no-method to clear only.");
                }
            } else {
                println!(
                    "{}: archived {}/{} message(s) from {} (method: {method})",
                    unsubscribe_purge_status_text(result.status),
                    result.archived_count,
                    result.message_count,
                    result.address
                );
                if let Some(mutation_id) = &result.mutation_id {
                    println!("Undo: mxr undo {mutation_id}");
                }
                if let Some(error) = &result.error {
                    println!("Note: {error}");
                }
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&UnsubscribePurgeOutput {
                address: &result.address,
                query: &result.query,
                dry_run: result.dry_run,
                method: unsubscribe_method_text(&result.method),
                status: result.status,
                message_count: result.message_count,
                archived_count: result.archived_count,
                mutation_id: result.mutation_id.as_deref(),
                error: result.error.as_deref(),
                preview_token: result.preview_token.as_deref(),
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&UnsubscribePurgeOutput {
                address: &result.address,
                query: &result.query,
                dry_run: result.dry_run,
                method: unsubscribe_method_text(&result.method),
                status: result.status,
                message_count: result.message_count,
                archived_count: result.archived_count,
                mutation_id: result.mutation_id.as_deref(),
                error: result.error.as_deref(),
                preview_token: result.preview_token.as_deref(),
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "address",
                "dry_run",
                "method",
                "status",
                "message_count",
                "archived_count",
                "mutation_id",
                "error",
            ])?;
            writer.write_record([
                result.address.as_str(),
                if result.dry_run { "true" } else { "false" },
                &unsubscribe_method_text(&result.method),
                unsubscribe_purge_status_text(result.status),
                &result.message_count.to_string(),
                &result.archived_count.to_string(),
                result.mutation_id.as_deref().unwrap_or(""),
                result.error.as_deref().unwrap_or(""),
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in &result.message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

fn unsubscribe_method_text(method: &UnsubscribeMethod) -> String {
    match method {
        UnsubscribeMethod::OneClick { .. } => "one-click".into(),
        UnsubscribeMethod::HttpLink { .. } => "http-link".into(),
        UnsubscribeMethod::Mailto { address, .. } => format!("mailto:{address}"),
        UnsubscribeMethod::BodyLink { .. } => "body-link".into(),
        UnsubscribeMethod::None => "none".into(),
    }
}

fn unsubscribe_purge_status_text(status: UnsubscribePurgeStatusData) -> &'static str {
    match status {
        UnsubscribePurgeStatusData::Preview => "preview",
        UnsubscribePurgeStatusData::Unsubscribed => "unsubscribed",
        UnsubscribePurgeStatusData::NoMethod => "no-method",
        UnsubscribePurgeStatusData::ArchiveOnly => "archive-only",
        UnsubscribePurgeStatusData::Failed => "failed",
    }
}

/// The distinct addresses among the positional arguments. A repeat that differs
/// only in case is dropped, and the first spelling is kept.
fn unique_addresses(arguments: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    arguments
        .iter()
        .filter(|arg| looks_like_email(arg))
        .filter(|arg| seen.insert(arg.to_ascii_lowercase()))
        .cloned()
        .collect()
}

/// The addresses named by `mxr unsubscribe` arguments, or none when every
/// argument is a message id. Ids and addresses cannot be mixed, and a mixed
/// command fails before anything is sent.
fn unsubscribe_addresses(arguments: &[String]) -> anyhow::Result<Vec<String>> {
    let addresses = unique_addresses(arguments);
    let ids_present = arguments.iter().any(|arg| !looks_like_email(arg));
    if !addresses.is_empty() && ids_present {
        anyhow::bail!("Pass message ids or email addresses, not both");
    }
    Ok(addresses)
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
    account: Option<String>,
    first: bool,
    limit: Option<u32>,
    yes: bool,
) -> anyhow::Result<()> {
    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, account.as_deref()).await?;
    let ids = crate::commands::selection::resolve_message_ids(
        &mut client,
        message_id.into_iter().collect(),
        search,
        account_id.as_ref(),
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

    #[test]
    fn repeated_addresses_are_planned_once_and_message_ids_are_not_addresses() {
        let args = vec![
            "Alice@Example.com".to_owned(),
            "alice@example.com".to_owned(),
            "bob@example.com".to_owned(),
        ];
        assert_eq!(
            unsubscribe_addresses(&args).unwrap(),
            vec!["Alice@Example.com".to_owned(), "bob@example.com".to_owned()]
        );
        assert!(unsubscribe_addresses(&["abc-123".to_owned()])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn an_address_mixed_with_a_message_id_is_rejected_before_anything_is_sent() {
        let args = vec![
            "alice@example.com".to_owned(),
            "8c9d3a02-2e9a-4af6-bf80-4b0d7bb6a4e2".to_owned(),
        ];
        let error = unsubscribe_addresses(&args).unwrap_err().to_string();
        assert_eq!(error, "Pass message ids or email addresses, not both");
    }

    #[tokio::test]
    async fn a_mixed_command_fails_without_connecting_to_the_daemon() {
        // No daemon runs in this test. A mixed command must fail on the
        // argument check, not on the connection.
        let error = unsubscribe(
            vec![
                "alice@example.com".into(),
                "8c9d3a02-2e9a-4af6-bf80-4b0d7bb6a4e2".into(),
            ],
            true,
            None,
            None,
            false,
            false,
            false,
            false,
            None,
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        assert_eq!(error, "Pass message ids or email addresses, not both");
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
