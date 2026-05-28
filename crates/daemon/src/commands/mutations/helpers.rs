use crate::cli::OutputFormat;
use crate::commands::selection::{parse_message_id as selection_parse_id, SelectionLimit};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use chrono::Utc;
use mxr_core::id::MessageId;
use mxr_core::types::Envelope;
use mxr_protocol::*;
use serde::Serialize;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;

pub(super) fn parse_message_id(id_str: &str) -> anyhow::Result<MessageId> {
    selection_parse_id(id_str)
}

pub(super) struct MutationSelection {
    pub(super) ids: Vec<MessageId>,
    pub(super) envelopes: Vec<Envelope>,
    pub(super) used_search: bool,
}

pub(super) async fn resolve_mutation_selection(
    client: &mut IpcClient,
    message_ids: Vec<String>,
    search: Option<String>,
    account_id: Option<&mxr_core::AccountId>,
) -> anyhow::Result<MutationSelection> {
    resolve_mutation_selection_with_limit(
        client,
        message_ids,
        search,
        account_id,
        SelectionLimit::Unbounded,
    )
    .await
}

pub(super) async fn resolve_mutation_selection_with_limit(
    client: &mut IpcClient,
    message_ids: Vec<String>,
    search: Option<String>,
    account_id: Option<&mxr_core::AccountId>,
    limit: SelectionLimit,
) -> anyhow::Result<MutationSelection> {
    let used_search = message_ids.is_empty() && search.is_some();
    let ids = crate::commands::selection::resolve_message_ids(
        client,
        message_ids,
        search,
        account_id,
        limit,
    )
    .await?;
    let envelopes = if ids.is_empty() {
        Vec::new()
    } else {
        let resp = client
            .request(Request::ListEnvelopesByIds {
                message_ids: ids.clone(),
            })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => envelopes,
            Response::Error { message, .. } => anyhow::bail!("{message}"),
            _ => anyhow::bail!("Unexpected response from envelope lookup"),
        }
    };
    if let Some(account_id) = account_id {
        if let Some(envelope) = envelopes.iter().find(|env| &env.account_id != account_id) {
            anyhow::bail!(
                "Message {} belongs to a different account",
                envelope.id.as_str()
            );
        }
    }

    Ok(MutationSelection {
        ids,
        envelopes,
        used_search,
    })
}

pub(super) fn requires_confirmation(
    destructive: bool,
    used_search: bool,
    matched_count: usize,
    yes: bool,
) -> bool {
    !yes && (destructive || used_search || matched_count > 1)
}

pub(super) fn render_selection_preview_lines(
    action: &str,
    selection: &MutationSelection,
) -> Vec<String> {
    let preview_limit = 8usize;
    let mut lines = vec![format!("Would {action} {} message(s)", selection.ids.len())];

    if !selection.envelopes.is_empty() {
        lines.push(String::new());
        for envelope in selection.envelopes.iter().take(preview_limit) {
            let from = envelope
                .from
                .name
                .as_deref()
                .unwrap_or(&envelope.from.email);
            let subject = if envelope.subject.is_empty() {
                "(no subject)"
            } else {
                &envelope.subject
            };
            lines.push(format!(
                "- {} | {} | {}",
                envelope.id.as_str(),
                from,
                subject
            ));
        }
        if selection.envelopes.len() > preview_limit {
            lines.push(format!(
                "... and {} more",
                selection.envelopes.len() - preview_limit
            ));
        }
    }

    lines
}

pub(super) fn print_selection_preview(action: &str, selection: &MutationSelection) {
    for line in render_selection_preview_lines(action, selection) {
        println!("{line}");
    }
}

#[derive(Serialize)]
struct MutationPreviewOutput {
    action: String,
    dry_run: bool,
    requested: usize,
    message_ids: Vec<String>,
    messages: Vec<MutationPreviewRecord>,
}

#[derive(Clone, Serialize)]
struct MutationPreviewRecord {
    message_id: String,
    from: String,
    subject: String,
}

#[derive(Serialize)]
struct MutationPreviewLine {
    action: String,
    dry_run: bool,
    message_id: String,
    from: String,
    subject: String,
}

#[derive(Serialize)]
struct MutationResultOutput {
    action: String,
    dry_run: bool,
    message_ids: Vec<String>,
    result: MutationResultData,
}

#[derive(Serialize)]
struct MutationAckOutput {
    action: String,
    dry_run: bool,
    message_ids: Vec<String>,
    ok: bool,
}

#[derive(Clone, Serialize)]
pub(super) struct BatchMutationError {
    pub(super) message_id: String,
    pub(super) error: String,
}

#[derive(Serialize)]
struct BatchMutationOutput {
    action: String,
    dry_run: bool,
    requested: usize,
    succeeded: usize,
    failed: usize,
    message_ids: Vec<String>,
    errors: Vec<BatchMutationError>,
}

pub(super) fn print_dry_run_output(
    action: &str,
    selection: &MutationSelection,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let message_ids = selection_message_ids(selection);
    let messages = preview_records(selection);

    match resolve_format(format) {
        OutputFormat::Table => print_selection_preview(action, selection),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationPreviewOutput {
                action: action.to_owned(),
                dry_run: true,
                requested: selection.ids.len(),
                message_ids,
                messages,
            })?
        ),
        OutputFormat::Jsonl => {
            let lines: Vec<_> = messages
                .into_iter()
                .map(|message| MutationPreviewLine {
                    action: action.to_owned(),
                    dry_run: true,
                    message_id: message.message_id,
                    from: message.from,
                    subject: message.subject,
                })
                .collect();
            println!("{}", jsonl(&lines)?);
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["action", "dry_run", "message_id", "from", "subject"])?;
            for message in messages {
                writer.write_record([
                    action,
                    "true",
                    &message.message_id,
                    &message.from,
                    &message.subject,
                ])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

fn selection_message_ids(selection: &MutationSelection) -> Vec<String> {
    selection.ids.iter().map(|id| id.as_str().clone()).collect()
}

fn preview_records(selection: &MutationSelection) -> Vec<MutationPreviewRecord> {
    selection
        .ids
        .iter()
        .map(|id| {
            let envelope = selection
                .envelopes
                .iter()
                .find(|envelope| envelope.id == *id);
            MutationPreviewRecord {
                message_id: id.as_str().clone(),
                from: envelope
                    .map(|envelope| {
                        envelope
                            .from
                            .name
                            .as_deref()
                            .unwrap_or(&envelope.from.email)
                            .to_owned()
                    })
                    .unwrap_or_default(),
                subject: envelope
                    .map(|envelope| {
                        if envelope.subject.is_empty() {
                            "(no subject)".to_owned()
                        } else {
                            envelope.subject.clone()
                        }
                    })
                    .unwrap_or_default(),
            }
        })
        .collect()
}

pub(super) fn confirm_action(action: &str, selection: &MutationSelection) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "Confirmation required for `{action}`. Re-run with --yes or inspect with --dry-run."
        );
    }

    print_selection_preview(action, selection);
    print!("\nContinue? [y/N] ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        return Ok(());
    }

    anyhow::bail!("Aborted")
}

pub(super) async fn run_simple_mutation<F>(
    client: &mut IpcClient,
    selection: MutationSelection,
    options: MutationRunOptions<'_>,
    build_request: F,
) -> anyhow::Result<()>
where
    F: FnOnce(Vec<MessageId>) -> Request,
{
    if selection.ids.is_empty() {
        anyhow::bail!("No messages matched");
    }

    if options.dry_run {
        print_dry_run_output(options.action, &selection, options.format)?;
        return Ok(());
    }

    if requires_confirmation(
        options.destructive,
        selection.used_search,
        selection.ids.len(),
        options.yes,
    ) {
        confirm_action(options.action, &selection)?;
    }

    let message_ids = selection.ids.clone();
    let resp = client.request(build_request(selection.ids)).await?;
    handle_mutation_response(
        resp,
        options.success_message,
        options.action,
        &message_ids,
        options.format,
    )
}

pub(super) fn handle_mutation_response(
    resp: Response,
    success_message: &str,
    action: &str,
    message_ids: &[MessageId],
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => print_ack_output(action, success_message, message_ids, format)?,
        Response::Ok {
            data: ResponseData::MutationResult { result },
        } => {
            let none_succeeded = result.succeeded == 0;
            let requested = result.requested;
            let skipped = result.skipped;
            let failed = result.failed;
            print_mutation_result_output(action, success_message, message_ids, result, format)?;
            if none_succeeded {
                anyhow::bail!(
                    "No messages changed (requested {requested}, skipped {skipped}, failed {failed})"
                );
            }
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn print_ack_output(
    action: &str,
    success_message: &str,
    message_ids: &[MessageId],
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Table => println!("{success_message}"),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationAckOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                ok: true,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&MutationAckOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                ok: true,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["action", "ok", "message_id"])?;
            for id in message_ids {
                writer.write_record([action.to_owned(), "true".to_owned(), id.as_str()])?;
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

fn print_mutation_result_output(
    action: &str,
    success_message: &str,
    message_ids: &[MessageId],
    result: MutationResultData,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Table => {
            for account in &result.accounts {
                if account.succeeded > 0 {
                    println!(
                        "{} {} message(s) on '{}'.",
                        success_message, account.succeeded, account.account_name
                    );
                }
                if account.skipped > 0 {
                    eprintln!(
                        "Skipped {} message(s) on '{}' ({}).",
                        account.skipped,
                        account.account_name,
                        account.error.as_deref().unwrap_or("account unavailable")
                    );
                }
                if account.failed > 0 {
                    eprintln!(
                        "Failed {} message(s) on '{}' ({}).",
                        account.failed,
                        account.account_name,
                        account.error.as_deref().unwrap_or("mutation failed")
                    );
                }
            }
            if let Some(mutation_id) = result.mutation_id.as_deref() {
                println!("Undo with: mxr undo {mutation_id}");
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationResultOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                result,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&MutationResultOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                result,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "requested",
                "succeeded",
                "skipped",
                "failed",
                "account_id",
                "account_name",
                "account_succeeded",
                "account_skipped",
                "account_failed",
                "error",
                "mutation_id",
            ])?;
            if result.accounts.is_empty() {
                writer.write_record([
                    action.to_owned(),
                    result.requested.to_string(),
                    result.succeeded.to_string(),
                    result.skipped.to_string(),
                    result.failed.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    result.mutation_id.clone().unwrap_or_default(),
                ])?;
            } else {
                for account in &result.accounts {
                    writer.write_record([
                        action.to_owned(),
                        result.requested.to_string(),
                        result.succeeded.to_string(),
                        result.skipped.to_string(),
                        result.failed.to_string(),
                        account.account_id.as_str().clone(),
                        account.account_name.clone(),
                        account.succeeded.to_string(),
                        account.skipped.to_string(),
                        account.failed.to_string(),
                        account.error.clone().unwrap_or_default(),
                        result.mutation_id.clone().unwrap_or_default(),
                    ])?;
                }
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

pub(super) fn print_batch_mutation_output(
    action: &str,
    dry_run: bool,
    table_message: &str,
    message_ids: &[MessageId],
    succeeded: usize,
    errors: Vec<BatchMutationError>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => println!("{table_message}"),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&BatchMutationOutput {
                action: action.to_owned(),
                dry_run,
                requested: message_ids.len(),
                succeeded,
                failed: errors.len(),
                message_ids: message_ids_to_strings(message_ids),
                errors,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&BatchMutationOutput {
                action: action.to_owned(),
                dry_run,
                requested: message_ids.len(),
                succeeded,
                failed: errors.len(),
                message_ids: message_ids_to_strings(message_ids),
                errors,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "dry_run",
                "requested",
                "succeeded",
                "failed",
                "message_id",
                "error",
            ])?;
            if errors.is_empty() {
                for id in message_ids {
                    writer.write_record([
                        action.to_owned(),
                        dry_run.to_string(),
                        message_ids.len().to_string(),
                        succeeded.to_string(),
                        "0".to_owned(),
                        id.as_str(),
                        String::new(),
                    ])?;
                }
            } else {
                for error in &errors {
                    writer.write_record([
                        action.to_owned(),
                        dry_run.to_string(),
                        message_ids.len().to_string(),
                        succeeded.to_string(),
                        errors.len().to_string(),
                        error.message_id.clone(),
                        error.error.clone(),
                    ])?;
                }
            }
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => {
            for id in message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

fn message_ids_to_strings(message_ids: &[MessageId]) -> Vec<String> {
    message_ids.iter().map(|id| id.as_str().clone()).collect()
}

pub(super) struct MutationRunOptions<'a> {
    pub(super) action: &'a str,
    pub(super) success_message: &'a str,
    pub(super) yes: bool,
    pub(super) dry_run: bool,
    pub(super) format: Option<OutputFormat>,
    pub(super) destructive: bool,
}

pub(super) fn parse_snooze_until(until: &str) -> anyhow::Result<chrono::DateTime<Utc>> {
    // Try the config-driven preset parser first ("tomorrow", "weekend",
    // "tonight" — uses the user's configured wake hour for each preset).
    let config = mxr_config::load_config().unwrap_or_default().snooze;
    if let Some(absolute) = mxr_config::snooze::parse_snooze_until(until, &config) {
        return Ok(absolute);
    }
    // Fall through to the conversational parser which handles richer
    // forms: `in 2h`, `monday 5pm`, `tomorrow 9am`, RFC3339, etc.
    mxr_core::parse_relative_time(until, chrono::Utc::now()).map_err(|e| {
        anyhow::anyhow!(
            "Cannot parse '{until}': {e}. Try: `in 2h`, `tomorrow 9am`, `monday 17:00`, or ISO 8601."
        )
    })
}

pub(super) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub(super) async fn load_attachments(
    client: &mut IpcClient,
    message_id: &MessageId,
) -> anyhow::Result<Vec<mxr_core::AttachmentMeta>> {
    let resp = client
        .request(Request::GetBody {
            message_id: message_id.clone(),
        })
        .await?;
    match resp {
        Response::Ok {
            data: ResponseData::Body { body },
        } => {
            if body.attachments.is_empty() {
                anyhow::bail!("No attachments");
            }
            Ok(body.attachments)
        }
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}

pub(super) fn attachment_by_index(
    attachments: &[mxr_core::AttachmentMeta],
    index: usize,
) -> anyhow::Result<&mxr_core::AttachmentMeta> {
    attachments
        .get(index.saturating_sub(1))
        .ok_or_else(|| anyhow::anyhow!("Attachment index {index} out of range"))
}

pub(super) async fn request_attachment_file(
    client: &mut IpcClient,
    request: Request,
) -> anyhow::Result<PathBuf> {
    let resp = client.request(request).await?;
    match resp {
        Response::Ok {
            data: ResponseData::AttachmentFile { file },
        } => Ok(PathBuf::from(file.path)),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
