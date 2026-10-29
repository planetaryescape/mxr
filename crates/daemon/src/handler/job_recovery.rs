//! Background jobs a previous daemon died with.
//!
//! A job runs in a task of the daemon process, so a restart ends it. Its row
//! stays `running` forever unless someone marks it. At startup this marks
//! every such job failed, with the counts the job had persisted after its
//! last finished chunk, and starts the hour an `--all` job's undo stays open.
//! Re-running the same search finishes what is left. Undo entries the dead
//! daemon was restoring are released.

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

/// Releases every undo entry claim. A claim belongs to an undo running in a
/// daemon process; at startup every holder is a process that died, and its
/// undo job was just marked interrupted, so the entries go back to whoever
/// undoes them next.
pub(crate) async fn release_undo_claims_of_dead_daemon(state: &AppState) -> u64 {
    match state.store.release_all_undo_claims().await {
        Ok(released) => {
            if released > 0 {
                tracing::warn!(
                    entries = released,
                    "released undo entries a previous daemon was restoring"
                );
            }
            released
        }
        Err(error) => {
            tracing::warn!(%error, "could not release undo claims a previous daemon held");
            0
        }
    }
}

fn action_for_kind(kind: &str) -> &'static str {
    match kind {
        "mutation.modify_labels" => "mail.label",
        _ => "mail.archive",
    }
}
