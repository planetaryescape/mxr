//! Reversing a whole bulk job (`mxr undo <job_id>`).
//!
//! The job's chunks each wrote an undo entry holding every message's state
//! from before the job, and the job anchored all their expiries to one
//! hour after it ended. Undoing the job is its own background job: it walks
//! those entries, puts each message back as it was, and reports progress and
//! the outcome the same way any job does. A message whose snapshot has
//! expired, is missing, or whose mail is gone is skipped and counted, never
//! dropped silently.

use super::mutations::{
    persist_job, persist_job_checked, rate_limit_wait, restore_snapshots, LabelCountSchedule,
    RestoreOutcome, MAX_CONSECUTIVE_RATE_LIMIT_WAITS, MAX_JOB_FAILURES,
};
use super::selection_jobs::{record_job_accepted, record_job_finished, JobActivity};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_protocol::{
    AccountMutationResultData, ClientKind, DaemonEvent, JobData, JobFailureData, JobProgressData,
    JobStatusData, MutationResultData, ResponseData,
};
use mxr_store::UndoEntry;
use std::sync::Arc;

const UNDO_JOB_KIND: &str = "mutation.undo";
/// Jobs looked at for one already undoing this job.
const RECENT_JOBS: i64 = 100;

pub(super) async fn start_undo_job(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
) -> HandlerResult {
    start_undo_job_at(state, source, job_id, chrono::Utc::now().timestamp_millis()).await
}

/// [`start_undo_job`] as of `now_ms` (unix milliseconds), so a test can stand
/// either side of the one-hour boundary.
pub(super) async fn start_undo_job_at(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
    now_ms: i64,
) -> HandlerResult {
    let job = load_job(state, job_id).await?;
    if job.undo_of.is_some() {
        return Err(format!("job `{job_id}` is an undo; it cannot be undone").into());
    }
    if matches!(job.status, JobStatusData::Queued | JobStatusData::Running) {
        return Err(format!(
            "job `{job_id}` is still running; undo it once it has finished (see `mxr jobs {job_id}`)"
        )
        .into());
    }
    let Some(expires_at) = job.undo_expires_at else {
        return Err(format!(
            "job `{job_id}` has no whole-job undo (it changed nothing, or it was not started with --all); undo its mutations with the ids in `mxr jobs {job_id}`"
        )
        .into());
    };
    if now_ms >= expires_at {
        return Err(format!(
            "undo: the window for job `{job_id}` closed {}",
            chrono::DateTime::from_timestamp_millis(expires_at)
                .map_or_else(|| expires_at.to_string(), |at| at.to_rfc3339())
        )
        .into());
    }

    // An undo of this job is already running: it is the answer, not a second
    // one racing it over the same entries.
    for json in state.store.list_mutation_jobs(RECENT_JOBS).await? {
        let Ok(running) = serde_json::from_str::<JobData>(&json) else {
            continue;
        };
        if running.undo_of.as_deref() == Some(job_id)
            && matches!(
                running.status,
                JobStatusData::Queued | JobStatusData::Running
            )
        {
            return Ok(ResponseData::JobStarted { job: running });
        }
    }

    // Messages the job changed, or may have: the ones it applied and the one
    // per failed account whose undo snapshot is uncertain.
    let total = job.progress.succeeded + job.progress.failed;
    let accounts = job
        .result
        .as_ref()
        .map(|result| {
            result
                .accounts
                .iter()
                .map(|account| AccountMutationResultData {
                    account_id: account.account_id.clone(),
                    account_name: account.account_name.clone(),
                    succeeded: 0,
                    skipped: 0,
                    failed: 0,
                    error: None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let undo = JobData {
        job_id: uuid::Uuid::now_v7().to_string(),
        kind: UNDO_JOB_KIND.to_string(),
        status: JobStatusData::Queued,
        progress: JobProgressData {
            total,
            completed: 0,
            succeeded: 0,
            skipped: 0,
            failed: 0,
        },
        undo_ids: Vec::new(),
        error: None,
        started_at: chrono::Utc::now().timestamp_millis(),
        finished_at: None,
        result: Some(MutationResultData {
            requested: total,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            accounts,
            mutation_id: None,
            undo_unavailable: false,
        }),
        selection: None,
        failures: Vec::new(),
        paused_until: None,
        undo_expires_at: None,
        undo_of: Some(job_id.to_string()),
    };
    persist_job_checked(state, &undo)
        .await
        .map_err(|error| HandlerError::from(format!("could not record the undo job: {error}")))?;

    let activity = JobActivity {
        source,
        action: "mail.archive",
        account_id: None,
        query: None,
        undo_of: Some(job_id.to_string()),
    };
    record_job_accepted(state, &activity, &undo);

    let started = undo.clone();
    let state = state.clone();
    let entry_ids = job.undo_ids;
    tokio::spawn(async move {
        run_undo_job(state, undo, entry_ids, activity).await;
    });
    Ok(ResponseData::JobStarted { job: started })
}

async fn load_job(state: &AppState, job_id: &str) -> Result<JobData, HandlerError> {
    let json = state
        .store
        .get_mutation_job(job_id)
        .await?
        .ok_or_else(|| HandlerError::from(format!("job not found: {job_id}")))?;
    serde_json::from_str(&json).map_err(|e| HandlerError::from(e.to_string()))
}

/// Running totals of an undo.
struct Tally {
    aggregate: MutationResultData,
    failures: Vec<JobFailureData>,
    /// Label counts the restores left stale, refreshed on a schedule.
    label_counts: LabelCountSchedule,
}

async fn run_undo_job(
    state: Arc<AppState>,
    mut undo: JobData,
    entry_ids: Vec<String>,
    activity: JobActivity,
) {
    undo.status = JobStatusData::Running;
    persist_job(&state, &undo).await;
    crate::chimes::emit_daemon_event(
        &state,
        DaemonEvent::OperationStarted {
            operation_id: undo.job_id.clone(),
            operation: UNDO_JOB_KIND.to_string(),
            account_id: None,
            message: format!("Undoing job over {} message(s)", undo.progress.total),
        },
    );

    let mut tally = Tally {
        aggregate: undo.result.clone().unwrap_or_else(|| MutationResultData {
            requested: undo.progress.total,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            accounts: Vec::new(),
            mutation_id: None,
            undo_unavailable: false,
        }),
        failures: Vec::new(),
        label_counts: LabelCountSchedule::new(),
    };
    let mut terminal_error: Option<String> = None;

    // Newest chunk first, the way a person would take it back.
    for entry_id in entry_ids.iter().rev() {
        let entry = match state.store.read_undo_entry(entry_id).await {
            Ok(Some(entry)) => entry,
            // Gone: expired and pruned, or already undone. Counted below
            // from what the job changed and what came back.
            Ok(None) => continue,
            Err(error) => {
                terminal_error = Some(format!("could not read an undo entry: {error}"));
                break;
            }
        };
        if entry.expires_at <= chrono::Utc::now().timestamp() {
            // Past its window: skipped, and dropped like any expired entry.
            tally.aggregate.skipped += entry.snapshots.len() as u32;
            if let Err(error) = state.store.delete_undo_entry(&entry.mutation_id).await {
                tracing::warn!(%error, "undo job: failed to delete an expired undo entry");
            }
        } else {
            restore_entry(&state, entry, &mut tally).await;
        }
        tally.label_counts.refresh_if_due(&state).await;
        undo.progress = progress_of(&tally.aggregate, undo.progress.total);
        undo.result = Some(tally.aggregate.clone());
        undo.failures.clone_from(&tally.failures);
        undo.paused_until = None;
        persist_job(&state, &undo).await;
        crate::chimes::emit_daemon_event(
            &state,
            DaemonEvent::OperationProgress {
                operation_id: undo.job_id.clone(),
                operation: UNDO_JOB_KIND.to_string(),
                account_id: None,
                current: undo.progress.completed,
                total: Some(undo.progress.total),
                message: format!(
                    "{} of {} restored",
                    tally.aggregate.succeeded, undo.progress.total
                ),
            },
        );
    }

    tally.label_counts.refresh(&state).await;

    // Messages the job changed whose snapshot is no longer there.
    let accounted = tally.aggregate.succeeded + tally.aggregate.skipped + tally.aggregate.failed;
    let unaccounted = undo.progress.total.saturating_sub(accounted);
    tally.aggregate.skipped += unaccounted;

    let finished_at = chrono::Utc::now().timestamp_millis();
    let aggregate = &tally.aggregate;
    if terminal_error.is_none() && aggregate.failed > 0 {
        terminal_error = Some(format!(
            "undo restored {} of {}; {} could not be restored right now. Run `mxr undo {}` again to retry them",
            aggregate.succeeded,
            undo.progress.total,
            aggregate.failed,
            undo.undo_of.as_deref().unwrap_or_default()
        ));
    } else if terminal_error.is_none() && aggregate.succeeded == 0 && aggregate.skipped > 0 {
        terminal_error = Some(format!(
            "nothing was restored: {} message(s) skipped because their snapshot expired, is missing, or was already undone",
            aggregate.skipped
        ));
    }
    undo.status = if terminal_error.is_some() {
        JobStatusData::Failed
    } else {
        JobStatusData::Succeeded
    };
    undo.progress = progress_of(&tally.aggregate, undo.progress.total);
    undo.result = Some(tally.aggregate.clone());
    undo.failures = tally.failures;
    undo.finished_at = Some(finished_at);
    undo.paused_until = None;
    // A finished undo that skipped some messages says so, even when it
    // restored the rest.
    undo.error = terminal_error.clone().or_else(|| {
        (tally.aggregate.skipped > 0).then(|| {
            format!(
                "{} message(s) were not restored: their snapshot expired or is missing, or the mail is gone",
                tally.aggregate.skipped
            )
        })
    });
    // Queued before the row says the undo is over, so whoever sees it finished
    // also finds its entry.
    record_job_finished(&state, &activity, &undo);
    persist_job(&state, &undo).await;

    match terminal_error {
        Some(error) => crate::chimes::emit_daemon_event(
            &state,
            DaemonEvent::OperationFailed {
                operation_id: undo.job_id,
                operation: UNDO_JOB_KIND.to_string(),
                account_id: None,
                error,
                retryable: false,
            },
        ),
        None => crate::chimes::emit_daemon_event(
            &state,
            DaemonEvent::OperationCompleted {
                operation_id: undo.job_id,
                operation: UNDO_JOB_KIND.to_string(),
                account_id: None,
                message: format!(
                    "Undo completed: {} restored, {} skipped",
                    tally.aggregate.succeeded, tally.aggregate.skipped
                ),
            },
        ),
    }
}

fn progress_of(aggregate: &MutationResultData, total: u32) -> JobProgressData {
    JobProgressData {
        total,
        completed: aggregate.succeeded + aggregate.skipped + aggregate.failed,
        succeeded: aggregate.succeeded,
        skipped: aggregate.skipped,
        failed: aggregate.failed,
    }
}

/// Put one chunk's messages back. A provider's request to wait is waited
/// out; what a retry could still fix stays under the same entry id so
/// undoing the job again retries just that.
async fn restore_entry(state: &AppState, entry: UndoEntry, tally: &mut Tally) {
    let mut pending = entry.snapshots.clone();
    let mut waits_in_a_row = 0u32;
    let mut last_error = None;
    loop {
        let RestoreOutcome {
            restored,
            irreversible,
            retryable,
            reindex,
            last_error: error,
            rate_limited_secs,
            stale_label_counts,
        } = restore_snapshots(state, entry.kind, &pending, true).await;
        tally.label_counts.note(stale_label_counts);
        // One search commit per pass, as a single undo does.
        if let Err(error) = super::mutations::reindex_messages_in_search(state, &reindex).await {
            tracing::warn!(%error, "undo job: search reindex failed; the messages are marked for the next reindex");
        }
        tally.aggregate.succeeded += restored;
        tally.aggregate.skipped += irreversible;
        if error.is_some() {
            last_error = error;
        }
        if restored > 0 {
            waits_in_a_row = 0;
        }
        pending = retryable;
        let Some(retry_after_secs) = rate_limited_secs else {
            break;
        };
        waits_in_a_row += 1;
        if waits_in_a_row > MAX_CONSECUTIVE_RATE_LIMIT_WAITS {
            break;
        }
        tokio::time::sleep(rate_limit_wait(retry_after_secs)).await;
    }

    if pending.is_empty() {
        if let Err(error) = state.store.delete_undo_entry(&entry.mutation_id).await {
            tracing::warn!(%error, "undo job: failed to delete a spent undo entry");
        }
        return;
    }
    tally.aggregate.failed += pending.len() as u32;
    let detail = last_error.unwrap_or_else(|| "the provider could not be reached".to_string());
    for snapshot in &pending {
        if tally.failures.len() < MAX_JOB_FAILURES {
            tally.failures.push(JobFailureData {
                message_id: snapshot.message_id.clone(),
                error: detail.clone(),
            });
        }
    }
    // Keep only what is left under the same id, with the same expiry.
    if let Err(error) = state
        .store
        .write_undo_entry(&UndoEntry {
            snapshots: pending,
            desk: None,
            ..entry
        })
        .await
    {
        tracing::warn!(%error, "undo job: failed to keep the entries a retry could still restore");
    }
}
