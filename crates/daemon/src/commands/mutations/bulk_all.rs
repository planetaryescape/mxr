//! `--all`: act on every match of a `--search` as one daemon-side job.
//!
//! The client never holds the message ids. It asks the daemon to preview the
//! selection (exact count, thread and account counts, a sample, a token),
//! shows that, and on confirmation starts the job with the token. The daemon
//! chunks and runs it; this command only waits and reports, so a lost reply,
//! a partial failure or a frame that would be too large cannot be mistaken
//! for a result.

use super::helpers::prompt_to_continue;
use crate::cli::OutputFormat;
use crate::commands::progress::format_thousands;
use crate::commands::resolve_optional_account;
use crate::ipc_client::IpcClient;
use crate::output::{jsonl, resolve_format};
use mxr_protocol::{
    JobData, JobStatusData, MutationCommand, Request, Response, ResponseData, SelectionAccountData,
    SelectionData, SelectionPreviewData, SelectionSampleData,
};
use serde::Serialize;
use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// How often a waiting command asks the daemon how the job is going.
const POLL: Duration = Duration::from_millis(500);
/// Progress lines on a non-terminal stderr are at most this far apart.
const PROGRESS_EVERY: Duration = Duration::from_secs(2);
/// Failed polls in a row (a restarting daemon) before the command gives up
/// waiting. The job's id is in the error, so it can be followed with
/// `mxr jobs`.
const MAX_FAILED_POLLS: u32 = 8;

/// What `--all` does to every match.
#[derive(Debug, Clone)]
pub enum BulkAction {
    Archive,
    ReadArchive,
    Label(String),
    Unlabel(String),
}

impl BulkAction {
    fn mutation(&self) -> MutationCommand {
        let message_ids = Vec::new();
        match self {
            Self::Archive => MutationCommand::Archive { message_ids },
            Self::ReadArchive => MutationCommand::ReadAndArchive { message_ids },
            Self::Label(name) => MutationCommand::ModifyLabels {
                message_ids,
                add: vec![name.clone()],
                remove: Vec::new(),
            },
            Self::Unlabel(name) => MutationCommand::ModifyLabels {
                message_ids,
                add: Vec::new(),
                remove: vec![name.clone()],
            },
        }
    }

    /// The action as the other bulk output names it.
    fn action(&self) -> String {
        match self {
            Self::Archive => "archive".to_owned(),
            Self::ReadArchive => "mark as read and archive".to_owned(),
            Self::Label(name) => format!("add label '{name}'"),
            Self::Unlabel(name) => format!("remove label '{name}'"),
        }
    }

    /// "Would archive 12 message(s)" / "Archived 12 message(s)".
    fn would(&self) -> String {
        match self {
            Self::Archive => "archive".to_owned(),
            Self::ReadArchive => "mark as read and archive".to_owned(),
            Self::Label(name) => format!("add label '{name}' to"),
            Self::Unlabel(name) => format!("remove label '{name}' from"),
        }
    }

    fn did(&self) -> String {
        match self {
            Self::Archive => "Archived".to_owned(),
            Self::ReadArchive => "Marked as read and archived".to_owned(),
            Self::Label(name) => format!("Added label '{name}' to"),
            Self::Unlabel(name) => format!("Removed label '{name}' from"),
        }
    }
}

pub struct BulkAllRun {
    pub action: BulkAction,
    pub search: String,
    pub account: Option<String>,
    pub yes: bool,
    pub dry_run: bool,
    pub format: Option<OutputFormat>,
    pub no_wait: bool,
    pub preview_token: Option<String>,
}

impl BulkAllRun {
    /// The run a command's flags describe. `search` is present whenever
    /// `--all` is (clap requires it).
    pub fn from_flags(
        action: BulkAction,
        search: Option<String>,
        account: Option<String>,
        yes: bool,
        dry_run: bool,
        format: Option<OutputFormat>,
        bulk: &crate::cli::BulkAllArgs,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            action,
            search: search.ok_or_else(|| anyhow::anyhow!("--all needs --search"))?,
            account,
            yes,
            dry_run,
            format,
            no_wait: bulk.no_wait,
            preview_token: bulk.preview_token.clone(),
        })
    }
}

pub async fn run(run: BulkAllRun) -> anyhow::Result<()> {
    let format = resolve_format(run.format.clone());
    if run.dry_run && matches!(format, OutputFormat::Csv | OutputFormat::Ids) {
        anyhow::bail!(
            "--all --dry-run has no message rows for {}: the daemon counts the selection and never sends the whole list. Use --format table, json or jsonl.",
            if matches!(format, OutputFormat::Csv) {
                "CSV"
            } else {
                "ids"
            }
        );
    }
    if run.search.trim().is_empty() {
        anyhow::bail!("--search needs a query");
    }

    let mut client = IpcClient::connect().await?;
    let account_id = resolve_optional_account(&mut client, run.account.as_deref()).await?;
    let selection = SelectionData::Search {
        query: run.search.clone(),
        account_id,
    };
    let mutation = run.action.mutation();

    // The token from an earlier `--all --dry-run` is the confirmation: the
    // user already saw that preview.
    let (token, counts) = match run.preview_token.clone() {
        Some(token) => (token, None),
        None => {
            let preview = preview(&mut client, &selection, &mutation).await?;
            if run.dry_run {
                return print_preview(&run.action, &selection, &preview, format);
            }
            if preview.count == 0 {
                anyhow::bail!("No messages matched");
            }
            if !run.yes {
                confirm(&run.action, &preview)?;
            } else {
                eprintln!(
                    "{} {} message(s) ({} thread(s)) as one job.",
                    capitalise(&run.action.would()),
                    format_thousands(preview.count as usize),
                    format_thousands(preview.threads as usize),
                );
            }
            (
                preview.preview_token.clone(),
                Some((preview.count, preview.threads)),
            )
        }
    };

    let job_id = uuid::Uuid::now_v7().to_string();
    let started = start_job(&mut client, &job_id, &token, &selection, &mutation).await?;
    let (selected_messages, selected_threads) = match counts {
        Some((messages, threads)) => (messages, Some(threads)),
        None => (started.progress.total, None),
    };
    if run.no_wait {
        return print_job(
            &run.action,
            &selection,
            selected_messages,
            selected_threads,
            &started,
            format,
            false,
        );
    }

    let job = wait_for_job(&mut client, started, &run.action.action()).await?;
    print_job(
        &run.action,
        &selection,
        selected_messages,
        selected_threads,
        &job,
        format,
        true,
    )?;
    fail_if_job_failed(&job)
}

async fn preview(
    client: &mut IpcClient,
    selection: &SelectionData,
    mutation: &MutationCommand,
) -> anyhow::Result<SelectionPreviewData> {
    let response = client
        .request(Request::PreviewSelection {
            selection: selection.clone(),
            mutation: mutation.clone(),
            sample_size: None,
        })
        .await?;
    match response {
        Response::Ok {
            data: ResponseData::SelectionPreview { preview },
        } => Ok(preview),
        Response::Error { message, .. } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected response from the selection preview"),
    }
}

/// Starts the job. If the connection drops before the reply arrives, the job
/// may exist anyway: its id was chosen here, so ask for it. Found means it
/// started; not found means nothing happened.
async fn start_job(
    client: &mut IpcClient,
    job_id: &str,
    token: &str,
    selection: &SelectionData,
    mutation: &MutationCommand,
) -> anyhow::Result<JobData> {
    let request = Request::StartSelectionJob {
        job_id: job_id.to_owned(),
        preview_token: token.to_owned(),
        selection: selection.clone(),
        mutation: mutation.clone(),
        client_correlation_id: None,
    };
    match client.request(request).await {
        Ok(Response::Ok {
            data: ResponseData::JobStarted { job },
        }) => Ok(job),
        Ok(Response::Error { message, .. }) => anyhow::bail!("{message}"),
        Ok(_) => anyhow::bail!("Unexpected response from the daemon"),
        Err(error) => recover_lost_start(job_id, &error.to_string()).await,
    }
}

async fn recover_lost_start(job_id: &str, error: &str) -> anyhow::Result<JobData> {
    let lookup = async {
        let mut client = IpcClient::connect().await?;
        client
            .request(Request::GetJob {
                job_id: job_id.to_owned(),
            })
            .await
    };
    match lookup.await {
        Ok(Response::Ok {
            data: ResponseData::Job { job },
        }) => Ok(job),
        Ok(Response::Error { message, .. }) if message.contains("job not found") => {
            anyhow::bail!(
                "The daemon did not receive the request ({error}); nothing was started. Run the command again."
            )
        }
        _ => anyhow::bail!(
            "Lost the connection to the daemon ({error}) before it replied; the job may have started. Check with: mxr jobs {job_id}"
        ),
    }
}

/// Polls the job until it ends, writing progress to stderr. A daemon that
/// restarts under the poll is waited for; one that stays away ends the wait
/// with the job's id.
pub(super) async fn wait_for_job(
    client: &mut IpcClient,
    mut job: JobData,
    what: &str,
) -> anyhow::Result<JobData> {
    let terminal = std::io::stderr().is_terminal();
    let mut last_line = Instant::now() - PROGRESS_EVERY;
    let mut shown = false;
    let mut failed_polls = 0u32;
    while matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
        if terminal || last_line.elapsed() >= PROGRESS_EVERY {
            write_progress(&job, what, terminal);
            last_line = Instant::now();
            shown = true;
        }
        tokio::time::sleep(POLL).await;
        match client
            .request(Request::GetJob {
                job_id: job.job_id.clone(),
            })
            .await
        {
            Ok(Response::Ok {
                data: ResponseData::Job { job: latest },
            }) => {
                failed_polls = 0;
                job = latest;
            }
            Ok(Response::Error { message, .. }) => {
                anyhow::bail!(
                    "Could not read job {}: {message}. It may still be running; check with: mxr jobs {}",
                    job.job_id,
                    job.job_id
                );
            }
            Ok(_) => anyhow::bail!("Unexpected response while waiting for job {}", job.job_id),
            Err(error) => {
                failed_polls += 1;
                if failed_polls >= MAX_FAILED_POLLS {
                    anyhow::bail!(
                        "Lost contact with the daemon while job {} was running ({error}). It may still be running; check with: mxr jobs {}",
                        job.job_id,
                        job.job_id
                    );
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                if let Ok(fresh) = IpcClient::connect().await {
                    *client = fresh;
                }
            }
        }
    }
    if shown && terminal {
        eprint!("\r\x1b[2K");
        let _ = std::io::stderr().flush();
    }
    Ok(job)
}

fn write_progress(job: &JobData, what: &str, terminal: bool) {
    let mut line = format!(
        "{what}: {} of {} done ({} changed, {} skipped, {} failed)",
        format_thousands(job.progress.completed as usize),
        format_thousands(job.progress.total as usize),
        format_thousands(job.progress.succeeded as usize),
        format_thousands(job.progress.skipped as usize),
        format_thousands(job.progress.failed as usize),
    );
    if let Some(until) = job
        .paused_until
        .and_then(chrono::DateTime::from_timestamp_millis)
    {
        line.push_str(&format!(
            "; waiting out a provider rate limit until {}",
            until.with_timezone(&chrono::Local).format("%H:%M:%S")
        ));
    }
    if terminal {
        eprint!("\r\x1b[2K{line}");
        let _ = std::io::stderr().flush();
    } else {
        eprintln!("{line}");
    }
}

/// A job that ended failed is a failed command, with its counts and its first
/// failing message, and the undo that is still open.
pub(super) fn fail_if_job_failed(job: &JobData) -> anyhow::Result<()> {
    if job.status != JobStatusData::Failed {
        return Ok(());
    }
    let mut message = format!(
        "Job {} failed after {} of {} messages: {} succeeded, {} skipped, {} failed.",
        job.job_id,
        format_thousands(job.progress.completed as usize),
        format_thousands(job.progress.total as usize),
        format_thousands(job.progress.succeeded as usize),
        format_thousands(job.progress.skipped as usize),
        format_thousands(job.progress.failed as usize),
    );
    if let Some(error) = &job.error {
        message.push_str(&format!(" {error}."));
    }
    if let Some(first) = job.failures.first() {
        message.push_str(&format!(
            " First failure: message {}: {}.",
            first.message_id.as_str(),
            first.error
        ));
    }
    if job.undo_expires_at.is_some() && job.progress.succeeded > 0 && job.undo_of.is_none() {
        message.push_str(&format!(
            " Reverse what it did with: mxr undo {}",
            job.job_id
        ));
    }
    anyhow::bail!("{message}")
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Confirmation and output
// ---------------------------------------------------------------------------

/// How many sample rows the table shows.
const TABLE_SAMPLE: usize = 8;

fn preview_lines(action: &BulkAction, preview: &SelectionPreviewData) -> Vec<String> {
    let mut lines = vec![format!(
        "Would {} {} message(s)",
        action.would(),
        format_thousands(preview.count as usize)
    )];
    lines.push(format!(
        "Selection: {} thread(s) / {} message(s)",
        format_thousands(preview.threads as usize),
        format_thousands(preview.count as usize)
    ));
    if preview.accounts.len() > 1 {
        let accounts: Vec<String> = preview
            .accounts
            .iter()
            .map(|account| {
                format!(
                    "{} {}",
                    account.account_name,
                    format_thousands(account.count as usize)
                )
            })
            .collect();
        lines.push(format!("Accounts: {}", accounts.join(", ")));
    }
    if !preview.sample.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "The newest {}:",
            preview.sample.len().min(TABLE_SAMPLE)
        ));
        for message in preview.sample.iter().take(TABLE_SAMPLE) {
            lines.push(format!(
                "- {} | {} | {}",
                message.message_id.as_str(),
                message.from,
                if message.subject.is_empty() {
                    "(no subject)"
                } else {
                    &message.subject
                }
            ));
        }
        let shown = preview.sample.len().min(TABLE_SAMPLE);
        if preview.count as usize > shown {
            lines.push(format!(
                "... and {} more",
                format_thousands(preview.count as usize - shown)
            ));
        }
    }
    lines
}

fn confirm(action: &BulkAction, preview: &SelectionPreviewData) -> anyhow::Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        anyhow::bail!(
            "Confirmation required to {} {} message(s). Re-run with --yes, or inspect with --dry-run.",
            action.would(),
            format_thousands(preview.count as usize)
        );
    }
    for line in preview_lines(action, preview) {
        println!("{line}");
    }
    prompt_to_continue()
}

#[derive(Serialize)]
struct PreviewOutput<'a> {
    action: String,
    dry_run: bool,
    all: bool,
    selection: &'a SelectionData,
    selected_messages: u32,
    selected_threads: u32,
    accounts: &'a [SelectionAccountData],
    /// The newest messages; never the whole selection.
    messages: &'a [SelectionSampleData],
    sample_size: u32,
    capped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<i64>,
}

#[derive(Serialize)]
struct PreviewLine<'a> {
    action: String,
    dry_run: bool,
    all: bool,
    message_id: String,
    from: &'a str,
    subject: &'a str,
    date: chrono::DateTime<chrono::Utc>,
    selected_messages: u32,
    selected_threads: u32,
    capped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview_token: Option<&'a str>,
}

fn print_preview(
    action: &BulkAction,
    selection: &SelectionData,
    preview: &SelectionPreviewData,
    format: OutputFormat,
) -> anyhow::Result<()> {
    let token = (!preview.preview_token.is_empty()).then_some(preview.preview_token.as_str());
    match format {
        OutputFormat::Table => {
            for line in preview_lines(action, preview) {
                println!("{line}");
            }
            if let Some(token) = token {
                println!();
                println!(
                    "Run exactly this with: --all --preview-token {token} (valid for 10 minutes, once)"
                );
            }
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&PreviewOutput {
                action: action.action(),
                dry_run: true,
                all: true,
                selection,
                selected_messages: preview.count,
                selected_threads: preview.threads,
                accounts: &preview.accounts,
                messages: &preview.sample,
                sample_size: preview.sample_size,
                capped: preview.capped,
                preview_token: token,
                expires_at: token.map(|_| preview.expires_at),
            })?
        ),
        OutputFormat::Jsonl => {
            let lines: Vec<_> = preview
                .sample
                .iter()
                .map(|message| PreviewLine {
                    action: action.action(),
                    dry_run: true,
                    all: true,
                    message_id: message.message_id.as_str(),
                    from: &message.from,
                    subject: &message.subject,
                    date: message.date,
                    selected_messages: preview.count,
                    selected_threads: preview.threads,
                    capped: preview.capped,
                    preview_token: token,
                })
                .collect();
            println!("{}", jsonl(&lines)?);
        }
        // Refused before the daemon is asked.
        OutputFormat::Csv | OutputFormat::Ids => {
            anyhow::bail!("--all --dry-run does not support this format")
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct JobOutput<'a> {
    action: String,
    dry_run: bool,
    all: bool,
    selection: &'a SelectionData,
    selected_messages: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected_threads: Option<u32>,
    job: &'a JobData,
    capped: bool,
}

#[allow(clippy::too_many_arguments)]
fn print_job(
    action: &BulkAction,
    selection: &SelectionData,
    selected_messages: u32,
    selected_threads: Option<u32>,
    job: &JobData,
    format: OutputFormat,
    waited: bool,
) -> anyhow::Result<()> {
    let output = JobOutput {
        action: action.action(),
        dry_run: false,
        all: true,
        selection,
        selected_messages,
        selected_threads,
        job,
        capped: false,
    };
    match format {
        OutputFormat::Table => print_job_table(action, job, waited),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        OutputFormat::Jsonl => println!("{}", serde_json::to_string(&output)?),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(Vec::new());
            writer.write_record([
                "action",
                "job_id",
                "status",
                "total",
                "completed",
                "succeeded",
                "skipped",
                "failed",
            ])?;
            writer.write_record([
                action.action(),
                job.job_id.clone(),
                format!("{:?}", job.status).to_ascii_lowercase(),
                job.progress.total.to_string(),
                job.progress.completed.to_string(),
                job.progress.succeeded.to_string(),
                job.progress.skipped.to_string(),
                job.progress.failed.to_string(),
            ])?;
            println!("{}", String::from_utf8(writer.into_inner()?)?.trim_end());
        }
        OutputFormat::Ids => println!("{}", job.job_id),
    }
    Ok(())
}

fn print_job_table(action: &BulkAction, job: &JobData, waited: bool) {
    let n = |value: u32| format_thousands(value as usize);
    if !waited {
        println!(
            "Started job {} to {} {} message(s).",
            job.job_id,
            action.would(),
            n(job.progress.total)
        );
        println!("Follow it with: mxr jobs {}", job.job_id);
        return;
    }
    match job.status {
        JobStatusData::Succeeded => {
            println!("{} {} message(s).", action.did(), n(job.progress.succeeded));
            if job.progress.skipped > 0 {
                println!(
                    "Skipped {} message(s) that were no longer in the mailbox.",
                    n(job.progress.skipped)
                );
            }
        }
        // The caller turns a failed job into an error after this prints.
        _ => println!(
            "{} {} of {} message(s) before the job stopped.",
            action.did(),
            n(job.progress.succeeded),
            n(job.progress.total)
        ),
    }
    println!("Job: {}", job.job_id);
    if job.progress.succeeded > 0 {
        println!("Undo with: mxr undo {} (works for one hour)", job.job_id);
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixtures for direct failures"
    )]

    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{JobFailureData, JobProgressData};

    fn job(status: JobStatusData) -> JobData {
        JobData {
            job_id: "job-1".into(),
            kind: "mutation.archive".into(),
            status,
            progress: JobProgressData {
                total: 1_250,
                completed: 400,
                succeeded: 300,
                skipped: 99,
                failed: 1,
            },
            undo_ids: vec!["u1".into(), "u2".into()],
            error: Some("mutation job stopped after partial progress".into()),
            started_at: 1,
            finished_at: Some(2),
            result: None,
            selection: None,
            failures: vec![JobFailureData {
                message_id: MessageId::new(),
                error: "provider said no".into(),
            }],
            paused_until: None,
            undo_expires_at: Some(3),
            undo_of: None,
            action: None,
            undo_id_count: 0,
        }
    }

    #[test]
    fn a_failed_job_is_an_error_naming_its_counts_first_failure_and_undo() {
        let failed = job(JobStatusData::Failed);
        let error = fail_if_job_failed(&failed).unwrap_err().to_string();
        assert!(
            error.contains("Job job-1 failed after 400 of 1,250 messages"),
            "{error}"
        );
        assert!(
            error.contains("300 succeeded, 99 skipped, 1 failed"),
            "{error}"
        );
        assert!(error.contains("partial progress"), "{error}");
        assert!(error.contains("provider said no"), "{error}");
        assert!(
            error.contains(&failed.failures[0].message_id.as_str()),
            "{error}"
        );
        assert!(error.contains("mxr undo job-1"), "{error}");
    }

    #[test]
    fn a_finished_job_is_not_an_error() {
        fail_if_job_failed(&job(JobStatusData::Succeeded)).unwrap();
    }

    #[test]
    fn a_failed_undo_does_not_offer_to_undo_itself() {
        let mut undo = job(JobStatusData::Failed);
        undo.undo_of = Some("job-0".into());
        let error = fail_if_job_failed(&undo).unwrap_err().to_string();
        assert!(!error.contains("mxr undo"), "{error}");
    }

    fn preview(count: u32, accounts: usize) -> SelectionPreviewData {
        SelectionPreviewData {
            count,
            threads: count / 2,
            accounts: (0..accounts)
                .map(|n| SelectionAccountData {
                    account_id: AccountId::new(),
                    account_name: format!("Account {n}"),
                    count: count / accounts as u32,
                })
                .collect(),
            sample: (0..3)
                .map(|n| SelectionSampleData {
                    message_id: MessageId::new(),
                    account_id: AccountId::new(),
                    thread_id: ThreadId::new(),
                    from: "Alice".into(),
                    subject: if n == 1 {
                        String::new()
                    } else {
                        format!("Subject {n}")
                    },
                    date: chrono::Utc::now(),
                })
                .collect(),
            sample_size: 20,
            capped: false,
            preview_token: "token".into(),
            expires_at: 0,
        }
    }

    #[test]
    fn the_prompt_shows_the_exact_count_the_counts_and_the_sample() {
        let lines = preview_lines(&BulkAction::Archive, &preview(70_412, 2)).join("\n");
        assert!(lines.contains("Would archive 70,412 message(s)"), "{lines}");
        assert!(
            lines.contains("Selection: 35,206 thread(s) / 70,412 message(s)"),
            "{lines}"
        );
        assert!(
            lines.contains("Accounts: Account 0 35,206, Account 1 35,206"),
            "{lines}"
        );
        assert!(lines.contains("| Alice | Subject 0"), "{lines}");
        assert!(lines.contains("| Alice | (no subject)"), "{lines}");
        assert!(lines.contains("... and 70,409 more"), "{lines}");
    }

    #[test]
    fn one_account_is_not_listed_and_a_short_selection_has_no_more_line() {
        let lines = preview_lines(&BulkAction::Label("Work".into()), &preview(3, 1)).join("\n");
        assert!(
            lines.contains("Would add label 'Work' to 3 message(s)"),
            "{lines}"
        );
        assert!(!lines.contains("Accounts:"), "{lines}");
        assert!(!lines.contains("more"), "{lines}");
    }

    #[test]
    fn each_action_builds_its_mutation_with_no_ids() {
        for (action, expected) in [
            (BulkAction::Archive, "Archive"),
            (BulkAction::ReadArchive, "ReadAndArchive"),
            (BulkAction::Label("x".into()), "ModifyLabels"),
            (BulkAction::Unlabel("x".into()), "ModifyLabels"),
        ] {
            let value = serde_json::to_value(action.mutation()).unwrap();
            assert_eq!(value["mutation"], expected);
            assert_eq!(value["message_ids"], serde_json::json!([]));
        }
        let label = serde_json::to_value(BulkAction::Label("x".into()).mutation()).unwrap();
        assert_eq!(
            (
                label["add"][0].as_str(),
                label["remove"].as_array().map(Vec::len)
            ),
            (Some("x"), Some(0))
        );
        let unlabel = serde_json::to_value(BulkAction::Unlabel("x".into()).mutation()).unwrap();
        assert_eq!(
            (
                unlabel["remove"][0].as_str(),
                unlabel["add"].as_array().map(Vec::len)
            ),
            (Some("x"), Some(0))
        );
    }
}
