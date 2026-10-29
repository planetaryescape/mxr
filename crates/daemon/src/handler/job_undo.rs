//! Reversing a whole bulk job (`mxr undo <job_id>`).
//!
//! The job's chunks each wrote an undo entry holding every message's state
//! from before the job, and the job anchored all their expiries to one
//! hour after it ended. Undoing the job is its own background job: it walks
//! those entries, puts each message back as it was, and reports progress and
//! the outcome the same way any job does. A message whose snapshot has
//! expired, is missing, or whose mail is gone is skipped and counted, never
//! dropped silently.
//!
//! Each entry is claimed in the store before it is restored, the same claim a
//! chunk's `UndoMutation` takes, so the two never restore one message twice:
//! an entry another undo holds is skipped and counted. The undo job releases
//! whatever it still holds when it ends, and a daemon start releases every
//! claim (`job_recovery`).

use super::job_records::{bounded_text, job_detail, MAX_JOB_TEXT_BYTES};
pub(super) use super::mutations::UndoClock;
use super::mutations::{
    persist_job, persist_job_checked, rate_limit_wait, real_clock, restore_snapshots,
    LabelCountSchedule, RestoreOutcome, UndoWindow, MAX_CONSECUTIVE_RATE_LIMIT_WAITS,
    MAX_JOB_FAILURES,
};
use super::selection_jobs::{record_job_accepted, record_job_finished, JobActivity};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::AccountId;
use mxr_protocol::{
    AccountMutationResultData, ClientKind, DaemonEvent, JobData, JobFailureData, JobProgressData,
    JobStatusData, MutationResultData, ResponseData,
};
use mxr_store::UndoEntry;
use std::collections::HashMap;
use std::sync::Arc;

const UNDO_JOB_KIND: &str = "mutation.undo";

pub(super) async fn start_undo_job(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
) -> HandlerResult {
    start_undo_job_with(
        state,
        source,
        job_id,
        chrono::Utc::now().timestamp_millis(),
        real_clock(),
    )
    .await
}

/// [`start_undo_job`] as of `now_ms` (unix milliseconds), so a test can stand
/// either side of the one-hour boundary.
#[cfg(test)]
pub(super) async fn start_undo_job_at(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
    now_ms: i64,
) -> HandlerResult {
    start_undo_job_with(state, source, job_id, now_ms, real_clock()).await
}

/// [`start_undo_job_at`] with the clock the running undo reads, too.
pub(super) async fn start_undo_job_with(
    state: &Arc<AppState>,
    source: ClientKind,
    job_id: &str,
    now_ms: i64,
    clock: UndoClock,
) -> HandlerResult {
    // Every job id is a UUID; anything longer would only be echoed back in
    // the errors below.
    if job_id.len() > 64 {
        return Err("job_id is not a job id".into());
    }
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

    // Messages the job changed, or may have: every message it saved a
    // snapshot of is counted as succeeded or failed (see `run_mutation_job`).
    let total = job.progress.succeeded + job.progress.failed;
    let original_accounts = job
        .result
        .as_ref()
        .map(|result| result.accounts.clone())
        .unwrap_or_default();
    let accounts: Vec<AccountMutationResultData> = original_accounts
        .iter()
        .map(|account| AccountMutationResultData {
            account_id: account.account_id.clone(),
            account_name: account.account_name.clone(),
            succeeded: 0,
            skipped: 0,
            failed: 0,
            error: None,
        })
        .collect();
    let expected: HashMap<AccountId, u32> = original_accounts
        .iter()
        .map(|account| {
            (
                account.account_id.clone(),
                account.succeeded + account.failed,
            )
        })
        .collect();
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
        action: None,
        undo_id_count: 0,
        failure_count: 0,
    };

    // Concurrent requests to undo one job are handed one undo job. This is
    // not what keeps a message from being restored twice (the store claim on
    // each entry does that, for chunk undos too); it keeps two requests from
    // splitting the work into two undo jobs that each report the other's
    // half as skipped. In-process state is enough for that: a restart ends
    // the undo job and releases its claims.
    {
        let mut claims = state.undo_claims.lock().await;
        if let Some(running) = claims.get(job_id) {
            let running = job_detail(load_job(state, running).await?);
            return Ok(ResponseData::JobStarted { job: running });
        }
        persist_job_checked(state, &undo).await.map_err(|error| {
            HandlerError::from(format!("could not record the undo job: {error}"))
        })?;
        claims.insert(job_id.to_string(), undo.job_id.clone());
    }

    let activity = JobActivity {
        source,
        action: "mail.archive",
        account_id: None,
        query: None,
        undo_of: Some(job_id.to_string()),
    };
    record_job_accepted(state, &activity, &undo);

    let started = job_detail(undo.clone());
    let state = state.clone();
    let entry_ids = job.undo_ids;
    tokio::spawn(async move {
        let original = job_id_of(&undo);
        run_undo_job(state.clone(), undo, entry_ids, activity, expected, clock).await;
        state.undo_claims.lock().await.remove(&original);
    });
    Ok(ResponseData::JobStarted { job: started })
}

fn job_id_of(undo: &JobData) -> String {
    undo.undo_of.clone().unwrap_or_default()
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
    /// Messages the original job changed, by account.
    expected: HashMap<AccountId, u32>,
    clock: UndoClock,
}

impl Tally {
    fn now_secs(&self) -> i64 {
        (self.clock)() / 1000
    }

    /// The account's row in the result, added if the original job never
    /// named the account.
    fn row(&mut self, account: &AccountId) -> &mut AccountMutationResultData {
        let accounts = &mut self.aggregate.accounts;
        let index = accounts
            .iter()
            .position(|row| &row.account_id == account)
            .unwrap_or_else(|| {
                accounts.push(AccountMutationResultData {
                    account_id: account.clone(),
                    account_name: account.to_string(),
                    succeeded: 0,
                    skipped: 0,
                    failed: 0,
                    error: None,
                });
                accounts.len() - 1
            });
        &mut accounts[index]
    }

    /// Count messages of one account, in the totals and in its row.
    fn restored(&mut self, account: &AccountId, count: u32) {
        self.aggregate.succeeded += count;
        self.row(account).succeeded += count;
    }

    fn skipped(&mut self, account: &AccountId, count: u32) {
        self.aggregate.skipped += count;
        self.row(account).skipped += count;
    }

    fn failed(&mut self, account: &AccountId, count: u32) {
        self.aggregate.failed += count;
        self.row(account).failed += count;
    }
}

async fn run_undo_job(
    state: Arc<AppState>,
    mut undo: JobData,
    entry_ids: Vec<String>,
    activity: JobActivity,
    expected: HashMap<AccountId, u32>,
    clock: UndoClock,
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
        expected,
        clock,
    };
    let mut terminal_error: Option<String> = None;

    let claimant = undo.job_id.clone();
    // Newest chunk first, the way a person would take it back.
    for entry_id in entry_ids.iter().rev() {
        let now = tally.now_secs();
        match state.store.claim_undo_entry(entry_id, &claimant, now).await {
            Ok(Some(entry)) => restore_entry(&state, entry, &claimant, &mut tally).await,
            // Not restored here, and counted below as skipped from what the
            // job changed and what came back: gone (already undone or
            // pruned), held by another undo, or past its window.
            Ok(None) => {
                if let Ok(mxr_store::UndoEntryState::Expired) =
                    state.store.undo_entry_state(entry_id, now).await
                {
                    if let Err(error) = state.store.delete_undo_entry(entry_id).await {
                        tracing::warn!(%error, "undo job: failed to delete an expired undo entry");
                    }
                }
            }
            Err(error) => {
                terminal_error = Some(format!("could not read an undo entry: {error}"));
                break;
            }
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
    // Nothing this undo still holds may stay out of reach. Each entry was
    // consumed or given back as it finished, so this only matters for one
    // it never finished.
    if let Err(error) = state.store.release_undo_claims_of(&claimant).await {
        tracing::warn!(%error, "undo job: failed to release its claims; a daemon restart releases them");
    }

    // Messages the job changed whose snapshot is no longer there, by the
    // account the job changed them in.
    let expected = std::mem::take(&mut tally.expected);
    for (account, changed) in expected {
        let row = tally.row(&account);
        let accounted = row.succeeded + row.skipped + row.failed;
        tally.skipped(&account, changed.saturating_sub(accounted));
    }
    let accounted = tally.aggregate.succeeded + tally.aggregate.skipped + tally.aggregate.failed;
    tally.aggregate.skipped += undo.progress.total.saturating_sub(accounted);

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
            "nothing was restored: {} message(s) skipped because their snapshot expired, is missing, was already undone, or another undo is restoring it",
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
                "{} message(s) were not restored: their snapshot expired or is missing, the mail is gone, or another undo is restoring it",
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
                error: bounded_text(&error, MAX_JOB_TEXT_BYTES),
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

/// Put back one chunk's messages, from an entry this undo holds. A provider's
/// request to wait is waited out; the window is checked before every
/// message, so a message reached after it closed is skipped, not restored
/// late. The entry is then consumed, or, when a retry could still fix some
/// messages, given back holding just those so undoing the job again retries
/// them. A message the window cut off is not kept.
async fn restore_entry(state: &AppState, entry: UndoEntry, claimant: &str, tally: &mut Tally) {
    let window = UndoWindow::new(entry.expires_at, tally.clock.clone());
    let mut pending = entry.snapshots.clone();
    let mut waits_in_a_row = 0u32;
    let mut last_error = None;
    loop {
        let RestoreOutcome {
            restored,
            irreversible: _,
            retryable,
            expired: _,
            expired_by_account,
            reindex,
            last_error: error,
            rate_limited_secs,
            stale_label_counts,
            restored_by_account,
            irreversible_by_account,
        } = restore_snapshots(state, entry.kind, &pending, true, &window).await;
        tally.label_counts.note(stale_label_counts);
        // One search commit per pass, as a single undo does.
        if let Err(error) = super::mutations::reindex_messages_in_search(state, &reindex).await {
            tracing::warn!(%error, "undo job: search reindex failed; the messages are marked for the next reindex");
        }
        for (account, count) in restored_by_account {
            tally.restored(&account, count);
        }
        for (account, count) in irreversible_by_account
            .into_iter()
            .chain(expired_by_account)
        {
            tally.skipped(&account, count);
        }
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
        if let Err(error) = state
            .store
            .consume_undo_entry(&entry.mutation_id, claimant)
            .await
        {
            tracing::warn!(%error, "undo job: failed to delete a spent undo entry");
        }
        return;
    }
    for snapshot in &pending {
        tally.failed(&snapshot.account_id, 1);
    }
    let detail = last_error.unwrap_or_else(|| "the provider could not be reached".to_string());
    let detail = bounded_text(&detail, MAX_JOB_TEXT_BYTES);
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
        .release_undo_entry_keeping(&entry.mutation_id, claimant, &pending)
        .await
    {
        tracing::warn!(%error, "undo job: failed to keep the entries a retry could still restore");
    }
}
