//! Background jobs a previous daemon died with.
//!
//! A job runs in a task of the daemon process, so a restart ends it. Its row
//! stays `running` forever unless someone marks it. At startup this marks
//! every such job failed, with the counts the job had persisted after its
//! last finished chunk, and starts the hour an `--all` job's undo stays open.
//! Re-running the same search finishes what is left. Undo entries the dead
//! daemon was restoring are released, and entries no job can anchor any more
//! are closed.

use super::mutations::{persist_job, SELECTION_UNDO_WINDOW_SECS};
use super::selection_jobs::{record_job_finished, JobActivity};
use crate::state::AppState;
use mxr_protocol::{ClientKind, JobData, JobStatusData};

/// Marks every queued or running mutation job failed ("interrupted"). Returns
/// how many it marked.
pub(crate) async fn mark_interrupted_mutation_jobs(state: &AppState) -> usize {
    let rows = match state.store.list_unfinished_mutation_jobs().await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(%error, "could not look for mutation jobs the last daemon left running");
            return 0;
        }
    };
    let mut marked = 0;
    for json in rows {
        let Ok(mut job) = serde_json::from_str::<JobData>(&json) else {
            tracing::warn!("a stored mutation job could not be read; leaving it as it is");
            continue;
        };
        let finished_at = chrono::Utc::now().timestamp_millis();
        job.status = JobStatusData::Failed;
        job.finished_at = Some(finished_at);
        job.paused_until = None;
        job.error = Some(format!(
            "interrupted by a daemon restart after {} of {} messages (counts are as of the last finished batch of 100); search again and re-run to finish",
            job.progress.completed, job.progress.total
        ));
        if job.selection.is_some() && !job.undo_ids.is_empty() {
            let expires_at = finished_at / 1000 + SELECTION_UNDO_WINDOW_SECS;
            match state
                .store
                .set_undo_entries_expiry(&job.undo_ids, expires_at)
                .await
            {
                Ok(_) => job.undo_expires_at = Some(expires_at * 1000),
                Err(error) => {
                    tracing::warn!(%error, job_id = %job.job_id, "could not anchor an interrupted job's undo window");
                }
            }
        }
        persist_job(state, &job).await;
        record_job_finished(
            state,
            &JobActivity {
                source: ClientKind::Daemon,
                action: action_for_kind(&job.kind),
                account_id: None,
                query: None,
                undo_of: job.undo_of.clone(),
            },
            &job,
        );
        marked += 1;
    }
    if marked > 0 {
        tracing::warn!(
            jobs = marked,
            "marked mutation jobs interrupted by a restart as failed"
        );
    }
    marked
}

/// Undo entries a previous daemon left in a state only it could end. Run
/// after [`mark_interrupted_mutation_jobs`], which anchors the entries of the
/// jobs it marks.
///
/// Every claim is released: a claim belongs to an undo running in a daemon
/// process, and at startup every holder is a process that died (its undo job
/// was just marked interrupted). Every entry still open-ended is closed: a
/// running selection job writes its entries with no deadline and anchors
/// them when it ends, so one still open now belongs to no job that could
/// anchor it (its row was pruned, or the workspace came from a backup), and
/// would otherwise stay claimable forever. Returns (released, closed).
pub(crate) async fn recover_undo_entries_of_dead_daemon(state: &AppState) -> (u64, u64) {
    let released = match state.store.release_all_undo_claims().await {
        Ok(released) => released,
        Err(error) => {
            tracing::warn!(%error, "could not release undo claims a previous daemon held");
            0
        }
    };
    let closed = match state
        .store
        .close_open_undo_entries(chrono::Utc::now().timestamp())
        .await
    {
        Ok(closed) => closed,
        Err(error) => {
            tracing::warn!(%error, "could not close undo entries no job can anchor");
            0
        }
    };
    if released > 0 || closed > 0 {
        tracing::warn!(
            released,
            closed,
            "recovered undo entries a previous daemon left open"
        );
    }
    (released, closed)
}

fn action_for_kind(kind: &str) -> &'static str {
    match kind {
        "mutation.modify_labels" => "mail.label",
        _ => "mail.archive",
    }
}
