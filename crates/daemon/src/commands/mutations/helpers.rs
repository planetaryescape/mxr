use crate::cli::OutputFormat;
use crate::commands::daemon_response_error;
use crate::commands::selection::{
    parse_message_id as selection_parse_id, CappedFields, CappedSelection, SelectionLimit,
};
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_core::id::{MessageId, ThreadId};
use mxr_core::types::{Envelope, UnsubscribeMethod};
use mxr_protocol::*;
use serde::Serialize;
use std::collections::HashSet;
use std::io::{IsTerminal, Write};

pub(super) fn parse_message_id(id_str: &str) -> anyhow::Result<MessageId> {
    selection_parse_id(id_str)
}

pub(super) struct MutationSelection {
    pub(super) ids: Vec<MessageId>,
    pub(super) envelopes: Vec<Envelope>,
    pub(super) used_search: bool,
    /// Set when the search matched more than the hard cap let through.
    pub(super) capped: Option<CappedSelection>,
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
    let crate::commands::selection::MessageSelection { ids, capped } =
        crate::commands::selection::resolve_message_selection(
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
            Response::Error { message, kind, .. } => {
                return Err(daemon_response_error(message, kind))
            }
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
        capped,
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
    lines.push(selection_counts(selection).selection_line());
    if let Some(capped) = selection.capped {
        lines.push(capped.notice());
    }

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
            let unsubscribe = if action == "unsubscribe" {
                format!(
                    " | Unsubscribe: {}",
                    unsubscribe_method_label(&envelope.unsubscribe)
                )
            } else {
                String::new()
            };
            lines.push(format!(
                "- {} | {} | {}{}",
                envelope.id.as_str(),
                from,
                subject,
                unsubscribe
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

#[derive(Clone, Copy, Serialize)]
pub(super) struct SelectionCounts {
    pub(super) selected_messages: usize,
    pub(super) selected_threads: usize,
    /// Carried so every output built from the counts can add the capped
    /// fields; the counts themselves serialise without it.
    #[serde(skip)]
    pub(super) capped: Option<CappedFields>,
}

impl SelectionCounts {
    pub(super) fn selection_line(self) -> String {
        format!(
            "Selection: {} thread(s) / {} message(s)",
            self.selected_threads, self.selected_messages
        )
    }

    fn changed_line(self, changed_messages: usize) -> String {
        format!(
            "Selection: {} thread(s) / {} message(s); changed {} message(s).",
            self.selected_threads, self.selected_messages, changed_messages
        )
    }
}

#[derive(Serialize)]
struct MutationPreviewOutput {
    action: String,
    dry_run: bool,
    requested: usize,
    selected_messages: usize,
    selected_threads: usize,
    message_ids: Vec<String>,
    messages: Vec<MutationPreviewRecord>,
    #[serde(flatten)]
    capped: Option<CappedFields>,
}

#[derive(Clone, Serialize)]
pub(super) struct MutationPreviewRecord {
    pub(super) message_id: String,
    pub(super) from: String,
    pub(super) subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) unsubscribe_method: Option<String>,
}

#[derive(Serialize)]
struct MutationPreviewLine {
    action: String,
    dry_run: bool,
    message_id: String,
    from: String,
    subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    unsubscribe_method: Option<String>,
    #[serde(flatten)]
    capped: Option<CappedFields>,
}

#[derive(Serialize)]
struct MutationResultOutput {
    action: String,
    dry_run: bool,
    selected_messages: usize,
    selected_threads: usize,
    message_ids: Vec<String>,
    result: MutationResultData,
    #[serde(flatten)]
    capped: Option<CappedFields>,
}

#[derive(Serialize)]
struct MutationJobOutput {
    action: String,
    dry_run: bool,
    message_ids: Vec<String>,
    job: JobData,
    #[serde(flatten)]
    capped: Option<CappedFields>,
}

#[derive(Serialize)]
struct MutationAckOutput {
    action: String,
    dry_run: bool,
    selected_messages: usize,
    selected_threads: usize,
    message_ids: Vec<String>,
    ok: bool,
    #[serde(flatten)]
    capped: Option<CappedFields>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_messages: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_threads: Option<usize>,
    message_ids: Vec<String>,
    errors: Vec<BatchMutationError>,
    #[serde(flatten)]
    capped: Option<CappedFields>,
}

pub(super) fn print_dry_run_output(
    action: &str,
    selection: &MutationSelection,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let message_ids = selection_message_ids(selection);
    let counts = selection_counts(selection);
    let messages = preview_records(selection, action == "unsubscribe");

    match resolve_format(format) {
        OutputFormat::Table => print_selection_preview(action, selection),
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&preview_output(action, selection, messages))?
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
                    unsubscribe_method: message.unsubscribe_method,
                    capped: counts.capped,
                })
                .collect();
            println!("{}", jsonl(&lines)?);
        }
        OutputFormat::Csv => {
            announce_cap_on_stderr(selection)?;
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
            announce_cap_on_stderr(selection)?;
            for id in message_ids {
                println!("{id}");
            }
        }
    }
    Ok(())
}

/// Formats with no room for extra fields get the cap notice on stderr.
fn announce_cap_on_stderr(selection: &MutationSelection) -> anyhow::Result<()> {
    if let Some(capped) = selection.capped {
        capped.announce(&mut std::io::stderr())?;
    }
    Ok(())
}

fn preview_output(
    action: &str,
    selection: &MutationSelection,
    messages: Vec<MutationPreviewRecord>,
) -> MutationPreviewOutput {
    let counts = selection_counts(selection);
    MutationPreviewOutput {
        action: action.to_owned(),
        dry_run: true,
        requested: selection.ids.len(),
        selected_messages: counts.selected_messages,
        selected_threads: counts.selected_threads,
        message_ids: selection_message_ids(selection),
        messages,
        capped: counts.capped,
    }
}

fn selection_message_ids(selection: &MutationSelection) -> Vec<String> {
    selection.ids.iter().map(|id| id.as_str().clone()).collect()
}

pub(super) fn preview_records(
    selection: &MutationSelection,
    include_unsubscribe_method: bool,
) -> Vec<MutationPreviewRecord> {
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
                unsubscribe_method: envelope
                    .filter(|_| include_unsubscribe_method)
                    .map(|envelope| unsubscribe_method_label(&envelope.unsubscribe).to_owned()),
            }
        })
        .collect()
}

pub(super) fn selection_counts(selection: &MutationSelection) -> SelectionCounts {
    let mut threads = HashSet::<ThreadId>::new();
    for envelope in &selection.envelopes {
        threads.insert(envelope.thread_id.clone());
    }
    SelectionCounts {
        selected_messages: selection.ids.len(),
        selected_threads: if threads.is_empty() {
            selection.ids.len()
        } else {
            threads.len()
        },
        capped: selection.capped.map(|capped| capped.fields()),
    }
}

fn unsubscribe_method_label(method: &UnsubscribeMethod) -> &'static str {
    match method {
        UnsubscribeMethod::OneClick { .. } => "OneClick",
        UnsubscribeMethod::HttpLink { .. } => "HttpLink",
        UnsubscribeMethod::Mailto { .. } => "Mailto",
        UnsubscribeMethod::BodyLink { .. } => "BodyLink",
        UnsubscribeMethod::None => "None",
    }
}

pub(super) fn confirm_action(action: &str, selection: &MutationSelection) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "Confirmation required for `{action}`. Re-run with --yes or inspect with --dry-run."
        );
    }

    print_selection_preview(action, selection);
    prompt_to_continue()
}

/// The gate every `--search` mutation passes before it sends a request:
/// confirms with a prompt that shows the preview (cap notice included), or,
/// when `--yes` skips the prompt, writes the cap notice to `notices` first.
pub(super) fn confirm_selection_to(
    action: &str,
    selection: &MutationSelection,
    destructive: bool,
    yes: bool,
    notices: &mut dyn Write,
) -> anyhow::Result<()> {
    if requires_confirmation(destructive, selection.used_search, selection.ids.len(), yes) {
        return confirm_action(action, selection);
    }
    if let Some(capped) = selection.capped {
        capped.announce(notices)?;
    }
    Ok(())
}

pub(super) fn confirm_selection(
    action: &str,
    selection: &MutationSelection,
    destructive: bool,
    yes: bool,
) -> anyhow::Result<()> {
    confirm_selection_to(action, selection, destructive, yes, &mut std::io::stderr())
}

/// Confirms a mutation whose preview the caller has already rendered as lines.
pub(super) fn confirm_prompt(preview_lines: &[String]) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "Confirmation required for `unsubscribe`. Re-run with --yes or inspect with --dry-run."
        );
    }

    for line in preview_lines {
        println!("{line}");
    }
    prompt_to_continue()
}

fn prompt_to_continue() -> anyhow::Result<()> {
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
    run_simple_mutation_to(
        client,
        selection,
        options,
        build_request,
        &mut std::io::stderr(),
    )
    .await
}

/// [`run_simple_mutation`] with the sink for the `--yes` cap notice injected,
/// so a test can see the notice land before the mutation request.
pub(super) async fn run_simple_mutation_to<F>(
    client: &mut IpcClient,
    selection: MutationSelection,
    options: MutationRunOptions<'_>,
    build_request: F,
    notices: &mut dyn Write,
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

    confirm_selection_to(
        options.action,
        &selection,
        options.destructive,
        options.yes,
        notices,
    )?;

    let counts = selection_counts(&selection);
    let message_ids = selection.ids.clone();
    let request = build_request(selection.ids);
    let use_async_job = options.async_job || (selection.used_search && message_ids.len() >= 200);
    let resp = if use_async_job {
        client.request(start_job_request(request)?).await?
    } else {
        client.request(request).await?
    };
    handle_mutation_response(
        resp,
        options.success_message,
        options.action,
        &message_ids,
        counts,
        options.format,
    )
}

fn start_job_request(request: Request) -> anyhow::Result<Request> {
    match request {
        Request::Mutation {
            mutation,
            client_correlation_id,
        } => Ok(Request::StartMutationJob {
            mutation,
            client_correlation_id,
        }),
        _ => anyhow::bail!("async jobs are only supported for mutation requests"),
    }
}

pub(super) fn handle_mutation_response(
    resp: Response,
    success_message: &str,
    action: &str,
    message_ids: &[MessageId],
    counts: SelectionCounts,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    let format = resolve_format(format);
    match resp {
        Response::Ok {
            data: ResponseData::Ack,
        } => print_ack_output(action, success_message, message_ids, counts, format)?,
        Response::Ok {
            data: ResponseData::MutationResult { result },
        } => {
            let none_succeeded = result.succeeded == 0;
            let requested = result.requested;
            let skipped = result.skipped;
            let failed = result.failed;
            print_mutation_result_output(
                action,
                success_message,
                message_ids,
                counts,
                result,
                format,
            )?;
            if none_succeeded {
                anyhow::bail!(
                    "No messages changed (requested {requested}, skipped {skipped}, failed {failed})"
                );
            }
        }
        Response::Ok {
            data: ResponseData::JobStarted { job },
        } => print_job_started_output(action, message_ids, counts, job, format)?,
        Response::Error { message, kind, .. } => return Err(daemon_response_error(message, kind)),
        _ => anyhow::bail!("Unexpected response"),
    }
    Ok(())
}

fn print_job_started_output(
    action: &str,
    message_ids: &[MessageId],
    counts: SelectionCounts,
    job: JobData,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Table => {
            println!(
                "Started {action} job {} for {} message(s).",
                job.job_id, job.progress.total
            );
            println!("Inspect with: mxr jobs {}", job.job_id);
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationJobOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                job,
                capped: counts.capped,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&MutationJobOutput {
                action: action.to_owned(),
                dry_run: false,
                message_ids: message_ids_to_strings(message_ids),
                job,
                capped: counts.capped,
            })?
        ),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record(["action", "job_id", "status", "total", "completed"])?;
            writer.write_record([
                action.to_owned(),
                job.job_id,
                format!("{:?}", job.status).to_ascii_lowercase(),
                job.progress.total.to_string(),
                job.progress.completed.to_string(),
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{}", job.job_id),
    }
    Ok(())
}

fn print_ack_output(
    action: &str,
    success_message: &str,
    message_ids: &[MessageId],
    counts: SelectionCounts,
    format: OutputFormat,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Table => {
            println!("{success_message}");
            println!("{}", counts.changed_line(message_ids.len()));
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationAckOutput {
                action: action.to_owned(),
                dry_run: false,
                selected_messages: counts.selected_messages,
                selected_threads: counts.selected_threads,
                message_ids: message_ids_to_strings(message_ids),
                ok: true,
                capped: counts.capped,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&MutationAckOutput {
                action: action.to_owned(),
                dry_run: false,
                selected_messages: counts.selected_messages,
                selected_threads: counts.selected_threads,
                message_ids: message_ids_to_strings(message_ids),
                ok: true,
                capped: counts.capped,
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
    counts: SelectionCounts,
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
            println!("{}", counts.changed_line(result.succeeded as usize));
            if let Some(mutation_id) = result.mutation_id.as_deref() {
                println!("Undo with: mxr undo {mutation_id}");
            } else if result.undo_unavailable {
                eprintln!(
                    "Note: undo is unavailable for this mutation — recording the undo entry failed, \
                     so it can't be reversed with `mxr undo`."
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&MutationResultOutput {
                action: action.to_owned(),
                dry_run: false,
                selected_messages: counts.selected_messages,
                selected_threads: counts.selected_threads,
                message_ids: message_ids_to_strings(message_ids),
                result,
                capped: counts.capped,
            })?
        ),
        OutputFormat::Jsonl => println!(
            "{}",
            serde_json::to_string(&MutationResultOutput {
                action: action.to_owned(),
                dry_run: false,
                selected_messages: counts.selected_messages,
                selected_threads: counts.selected_threads,
                message_ids: message_ids_to_strings(message_ids),
                result,
                capped: counts.capped,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn print_batch_mutation_output(
    action: &str,
    dry_run: bool,
    table_message: &str,
    message_ids: &[MessageId],
    succeeded: usize,
    errors: Vec<BatchMutationError>,
    selection_counts: Option<SelectionCounts>,
    format: Option<OutputFormat>,
) -> anyhow::Result<()> {
    match resolve_format(format) {
        OutputFormat::Table => {
            println!("{table_message}");
            if let Some(counts) = selection_counts {
                println!("{}", counts.changed_line(succeeded));
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&BatchMutationOutput {
                action: action.to_owned(),
                dry_run,
                requested: message_ids.len(),
                succeeded,
                failed: errors.len(),
                selected_messages: selection_counts.map(|counts| counts.selected_messages),
                selected_threads: selection_counts.map(|counts| counts.selected_threads),
                message_ids: message_ids_to_strings(message_ids),
                errors,
                capped: selection_counts.and_then(|counts| counts.capped),
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
                selected_messages: selection_counts.map(|counts| counts.selected_messages),
                selected_threads: selection_counts.map(|counts| counts.selected_threads),
                message_ids: message_ids_to_strings(message_ids),
                errors,
                capped: selection_counts.and_then(|counts| counts.capped),
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
    pub(super) async_job: bool,
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
        Response::Error { message, kind, .. } => Err(daemon_response_error(message, kind)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::selection::CappedSelection;
    use crate::test_fixtures::{FakeBulkDaemon, TestEnvelopeBuilder};

    const NOTICE: &str =
        "notice: More than 1,000 messages match; only the newest 1,000 are selected. Narrow the search to reach the rest.";

    fn run_options(yes: bool, dry_run: bool) -> MutationRunOptions<'static> {
        MutationRunOptions {
            action: "trash",
            success_message: "Trashed",
            yes,
            dry_run,
            format: Some(OutputFormat::Json),
            destructive: true,
            async_job: false,
        }
    }

    fn trash_request(ids: Vec<MessageId>) -> Request {
        Request::mutation(MutationCommand::Trash { message_ids: ids })
    }

    #[tokio::test]
    async fn the_cap_notice_is_written_before_any_mutation_request_with_yes() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;
        let selection = resolve_mutation_selection(&mut client, Vec::new(), Some("*".into()), None)
            .await
            .unwrap();
        let selected_ids = selection.ids.clone();
        assert_eq!(selected_ids, daemon.message_ids[..1_000]);

        run_simple_mutation_to(
            &mut client,
            selection,
            run_options(true, false),
            trash_request,
            &mut daemon.notice_sink(),
        )
        .await
        .unwrap();

        let events = daemon.events.lock().unwrap().clone();
        assert_eq!(
            events,
            ["search", "envelopes", NOTICE, "mutation"],
            "the user is told about the cap after the selection and before anything changes"
        );
        // The run changes exactly the messages the selection holds, which are
        // the ones the dry run lists.
        let requests = daemon.requests.lock().unwrap().clone();
        let Some(Request::StartMutationJob {
            mutation: MutationCommand::Trash { message_ids },
            ..
        }) = requests.last()
        else {
            unreachable!("expected a trash job, got {requests:?}");
        };
        assert_eq!(message_ids, &selected_ids);
    }

    #[tokio::test]
    async fn a_dry_run_sends_no_mutation_and_lists_the_ids_a_real_run_would_use() {
        let daemon = FakeBulkDaemon::start(1_523);
        let mut client = daemon.client().await;
        let selection = resolve_mutation_selection(&mut client, Vec::new(), Some("*".into()), None)
            .await
            .unwrap();

        let preview = serde_json::to_value(preview_output(
            "trash",
            &selection,
            preview_records(&selection, false),
        ))
        .unwrap();
        run_simple_mutation_to(
            &mut client,
            selection,
            run_options(false, true),
            trash_request,
            &mut daemon.notice_sink(),
        )
        .await
        .unwrap();

        assert!(preview.get("matched_total").is_none());
        assert_eq!(preview["capped"], true);
        assert_eq!(
            preview["message_ids"],
            serde_json::to_value(
                daemon.message_ids[..1_000]
                    .iter()
                    .map(MessageId::as_str)
                    .collect::<Vec<_>>()
            )
            .unwrap()
        );
        // The dry run says so in its own output, not through the notice, and
        // changes nothing.
        assert_eq!(
            daemon.events.lock().unwrap().clone(),
            ["search", "envelopes"]
        );
    }

    #[tokio::test]
    async fn no_notice_is_written_when_the_search_fits_under_the_cap() {
        let daemon = FakeBulkDaemon::start(300);
        let mut client = daemon.client().await;
        let selection = resolve_mutation_selection(&mut client, Vec::new(), Some("*".into()), None)
            .await
            .unwrap();

        run_simple_mutation_to(
            &mut client,
            selection,
            run_options(true, false),
            trash_request,
            &mut daemon.notice_sink(),
        )
        .await
        .unwrap();

        assert_eq!(
            daemon.events.lock().unwrap().clone(),
            ["search", "envelopes", "mutation"]
        );
    }

    #[test]
    fn the_preview_and_the_prompt_carry_the_cap_notice() {
        let envelope = TestEnvelopeBuilder::new().build();
        let selection = MutationSelection {
            ids: vec![envelope.id.clone()],
            envelopes: vec![envelope],
            used_search: true,
            capped: Some(CappedSelection { selected: 1_000 }),
        };

        // The same lines are the dry-run table and the confirmation prompt.
        let lines = render_selection_preview_lines("trash", &selection);

        assert!(
            lines.contains(&NOTICE.trim_start_matches("notice: ").to_owned()),
            "{lines:?}"
        );
    }

    #[test]
    fn capped_fields_reach_every_output_built_from_the_counts_and_only_then() {
        let capped = CappedSelection { selected: 1_000 };
        let uncapped_counts = SelectionCounts {
            selected_threads: 2,
            selected_messages: 3,
            capped: None,
        };
        let capped_counts = SelectionCounts {
            capped: Some(capped.fields()),
            ..uncapped_counts
        };
        let ack = |counts: SelectionCounts| {
            serde_json::to_value(MutationAckOutput {
                action: "trash".into(),
                dry_run: false,
                selected_messages: counts.selected_messages,
                selected_threads: counts.selected_threads,
                message_ids: Vec::new(),
                ok: true,
                capped: counts.capped,
            })
            .unwrap()
        };

        let with = ack(capped_counts);
        let without = ack(uncapped_counts);

        assert_eq!(with["capped"], true);
        assert_eq!(
            with["selected_messages"], 3,
            "existing fields keep their names"
        );
        assert!(without.get("capped").is_none());
    }

    #[test]
    fn dry_run_preview_labels_threads_and_messages_separately() {
        let thread_id = ThreadId::new();
        let first = TestEnvelopeBuilder::new()
            .thread_id(thread_id.clone())
            .provider_id("one")
            .build();
        let second = TestEnvelopeBuilder::new()
            .thread_id(thread_id)
            .provider_id("two")
            .build();
        let selection = MutationSelection {
            ids: vec![first.id.clone(), second.id.clone()],
            envelopes: vec![first, second],
            used_search: true,
            capped: None,
        };

        let lines = render_selection_preview_lines("archive", &selection);
        assert!(
            lines
                .iter()
                .any(|line| line == "Selection: 1 thread(s) / 2 message(s)"),
            "preview should distinguish collapsed thread count from selected messages: {lines:?}"
        );
    }

    #[test]
    fn apply_summary_labels_threads_messages_and_changed_messages() {
        let counts = SelectionCounts {
            selected_threads: 5,
            selected_messages: 7,
            capped: None,
        };

        assert_eq!(
            counts.changed_line(5),
            "Selection: 5 thread(s) / 7 message(s); changed 5 message(s)."
        );
    }

    #[test]
    fn unsubscribe_dry_run_preview_reports_resolved_method_or_none() {
        let with_method = TestEnvelopeBuilder::new()
            .provider_id("with-method")
            .unsubscribe(UnsubscribeMethod::HttpLink {
                url: "https://example.com/unsubscribe".into(),
            })
            .build();
        let without_method = TestEnvelopeBuilder::new()
            .provider_id("without-method")
            .unsubscribe(UnsubscribeMethod::None)
            .build();
        let selection = MutationSelection {
            ids: vec![with_method.id.clone(), without_method.id.clone()],
            envelopes: vec![with_method, without_method],
            used_search: true,
            capped: None,
        };

        let rendered = render_selection_preview_lines("unsubscribe", &selection).join("\n");
        assert!(rendered.contains("Unsubscribe: HttpLink"), "{rendered}");
        assert!(rendered.contains("Unsubscribe: None"), "{rendered}");
    }
}
