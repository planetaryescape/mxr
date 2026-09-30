//! `GetThreadGists`: gists for list rows, many conversations at once.
//!
//! The request itself never calls a model. It returns the cached gists that
//! still match each conversation, and with `generate` it queues the people's
//! conversations that have none. A small pool of background writers (one by
//! default: a local model answers one request at a time well) takes them in
//! order, writes each through the same path as the reader's gist (same
//! prompt, privacy policy, quote check and cache), and announces each one
//! with `DaemonEvent::ThreadGistReady`.
//!
//! The queue holds conversation ids only. The newest request goes to the
//! front, since it is what the user is looking at now; asking again for a
//! queued or in-flight conversation never queues it twice; and the queue is
//! capped, so requests for rows scrolled past long ago fall off the back.

use super::diagnostics_impl::emit_operation_event;
use super::mail_kind::{self, SenderKind};
use super::places::AccountKinds;
use super::thread_gist::{
    cached_gist, load_and_write_gist, thread_envelopes, GistCall, GistPolicy, GistSetup,
};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::Envelope;
use mxr_protocol::{
    DaemonEvent, GistModelData, ResponseData, ThreadGistBatchData, ThreadGistData,
    ThreadGistSkipData, ThreadGistSkipReasonData, ThreadGistStatusData, THREAD_GISTS_MAX_BATCH,
};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Most conversations waiting for a writer. At one gist every few seconds,
/// more than this is minutes of work for rows the user has left behind.
const PENDING_CAP: usize = 64;
/// A conversation the model just failed on waits this long before it is
/// queued again (unless a new message changes it).
const FAILED_BACKOFF: Duration = Duration::from_secs(10 * 60);
/// Upper bound on `llm.gist_concurrency`.
const MAX_WRITERS: usize = 8;

/// Conversations waiting for a list gist, and the writers draining them.
#[derive(Default)]
pub(crate) struct GistQueue {
    inner: Mutex<QueueState>,
}

#[derive(Default)]
struct QueueState {
    pending: VecDeque<ThreadId>,
    in_flight: HashSet<ThreadId>,
    /// Writers running now; each exits when the queue is empty.
    writers: usize,
    /// Conversations the model failed on, with the content they failed at
    /// and when.
    failed: HashMap<ThreadId, (String, Instant)>,
}

impl GistQueue {
    /// Put `ids` at the front of the queue, first id first, skipping any
    /// already being written; drop the oldest requests past the cap.
    /// Returns how many writers to start so that up to `writers` run.
    fn enqueue(&self, ids: &[ThreadId], writers: usize) -> usize {
        let mut state = self.inner.lock();
        let fresh: Vec<ThreadId> = ids
            .iter()
            .filter(|id| !state.in_flight.contains(*id))
            .cloned()
            .collect();
        state.pending.retain(|id| !fresh.contains(id));
        for id in fresh.into_iter().rev() {
            state.pending.push_front(id);
        }
        state.pending.truncate(PENDING_CAP);
        let start = writers
            .saturating_sub(state.writers)
            .min(state.pending.len());
        state.writers += start;
        start
    }

    /// The next conversation to write, now marked in flight. `None` retires
    /// the calling writer.
    fn next(&self) -> Option<ThreadId> {
        let mut state = self.inner.lock();
        match state.pending.pop_front() {
            Some(id) => {
                state.in_flight.insert(id.clone());
                Some(id)
            }
            None => {
                state.writers = state.writers.saturating_sub(1);
                None
            }
        }
    }

    /// A writer is done with `id`; `failed_at` is the content hash the
    /// model failed on, if it did.
    fn finish(&self, id: &ThreadId, failed_at: Option<String>) {
        let mut state = self.inner.lock();
        state.in_flight.remove(id);
        match failed_at {
            Some(hash) => {
                state.failed.insert(id.clone(), (hash, Instant::now()));
            }
            None => {
                state.failed.remove(id);
            }
        }
    }

    /// Nothing more can be written (no model, privacy block, shutdown).
    fn clear_pending(&self) {
        self.inner.lock().pending.clear();
    }

    /// Forget failures older than the backoff; once per request.
    fn prune_failed(&self) {
        self.inner
            .lock()
            .failed
            .retain(|_, (_, at)| at.elapsed() < FAILED_BACKOFF);
    }

    /// The model failed on this conversation, as it is now, a moment ago.
    fn recently_failed(&self, id: &ThreadId, content_hash: &str) -> bool {
        self.inner
            .lock()
            .failed
            .get(id)
            .is_some_and(|(hash, _)| hash == content_hash)
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> (Vec<ThreadId>, Vec<ThreadId>, usize) {
        let state = self.inner.lock();
        (
            state.pending.iter().cloned().collect(),
            state.in_flight.iter().cloned().collect(),
            state.writers,
        )
    }
}

pub(super) async fn get_thread_gists(
    state: &Arc<AppState>,
    thread_ids: &[ThreadId],
    generate: bool,
) -> HandlerResult {
    if thread_ids.len() > THREAD_GISTS_MAX_BATCH {
        return Err(HandlerError::from(format!(
            "at most {THREAD_GISTS_MAX_BATCH} conversations per request, got {}",
            thread_ids.len()
        )));
    }
    let policy = GistPolicy::pin(state);
    let model = if !policy.llm.is_configured() {
        GistModelData::Disabled
    } else if policy.llm.blocked_reason().is_some() {
        GistModelData::Blocked
    } else {
        GistModelData::Available
    };
    let mut batch = ThreadGistBatchData {
        model,
        gists: Vec::new(),
        queued: Vec::new(),
        skipped: Vec::new(),
    };
    // No model: nothing is cached for it and nothing can be written.
    if model == GistModelData::Disabled {
        return Ok(ResponseData::ThreadGists { batch });
    }

    state.gist_queue.prune_failed();
    let mut seen = HashSet::new();
    let mut missing: Vec<Candidate> = Vec::new();
    for thread_id in thread_ids {
        if !seen.insert(thread_id) {
            continue;
        }
        let envelopes = thread_envelopes(state, thread_id).await?;
        let Some(setup) = GistSetup::new(&policy, thread_id, &envelopes) else {
            batch
                .skipped
                .push(skip(thread_id, ThreadGistSkipReasonData::NotFound));
            continue;
        };
        if let Some(gist) = cached_gist(state, thread_id, &setup).await? {
            batch.gists.push(gist);
        } else if !generate || model != GistModelData::Available {
            batch
                .skipped
                .push(skip(thread_id, ThreadGistSkipReasonData::NotGenerated));
        } else if state
            .gist_queue
            .recently_failed(thread_id, &setup.content_hash)
        {
            batch
                .skipped
                .push(skip(thread_id, ThreadGistSkipReasonData::RecentlyFailed));
        } else {
            missing.push(Candidate {
                thread_id: thread_id.clone(),
                envelopes,
            });
        }
    }

    if !missing.is_empty() {
        let people = people_threads(state, &missing).await?;
        for candidate in missing {
            match people.get(&candidate.thread_id) {
                Some(true) => batch.queued.push(candidate.thread_id),
                Some(false) => batch.skipped.push(skip(
                    &candidate.thread_id,
                    ThreadGistSkipReasonData::NotPeople,
                )),
                None => batch.skipped.push(skip(
                    &candidate.thread_id,
                    ThreadGistSkipReasonData::NotFound,
                )),
            }
        }
        let writers = state
            .config_snapshot()
            .llm
            .gist_concurrency
            .clamp(1, MAX_WRITERS);
        let start = state.gist_queue.enqueue(&batch.queued, writers);
        for _ in 0..start {
            let state = state.clone();
            tokio::spawn(async move { run_writer(state).await });
        }
    }
    Ok(ResponseData::ThreadGists { batch })
}

struct Candidate {
    thread_id: ThreadId,
    envelopes: Vec<Envelope>,
}

fn skip(thread_id: &ThreadId, reason: ThreadGistSkipReasonData) -> ThreadGistSkipData {
    ThreadGistSkipData {
        thread_id: thread_id.clone(),
        reason,
    }
}

/// Whether each conversation is with a person, by the shared kind rules
/// (`mail_kind`) applied to its newest message from someone else. A
/// conversation only you have written in is with the people you wrote to.
/// Absent from the map: the message is gone (trashed since).
async fn people_threads(
    state: &AppState,
    candidates: &[Candidate],
) -> Result<HashMap<ThreadId, bool>, HandlerError> {
    let mut by_account: HashMap<&AccountId, Vec<&Candidate>> = HashMap::new();
    for candidate in candidates {
        if let Some(newest) = candidate.envelopes.last() {
            by_account
                .entry(&newest.account_id)
                .or_default()
                .push(candidate);
        }
    }
    let mut out = HashMap::new();
    for (account_id, candidates) in by_account {
        let senders: Vec<String> = candidates
            .iter()
            .flat_map(|candidate| &candidate.envelopes)
            .map(|envelope| envelope.from.email.to_ascii_lowercase())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let kinds = AccountKinds::load(state, account_id, &senders).await?;
        for candidate in candidates {
            let theirs = candidate
                .envelopes
                .iter()
                .rev()
                .find(|envelope| !kinds.is_self(&envelope.from.email));
            let Some(theirs) = theirs else {
                out.insert(candidate.thread_id.clone(), true);
                continue;
            };
            if let Some(message) = state.store.place_message(&theirs.id).await? {
                let kind = mail_kind::classify(&kinds.signals(&message)).kind;
                out.insert(candidate.thread_id.clone(), kind == SenderKind::Person);
            }
        }
    }
    Ok(out)
}

/// Drain the queue one conversation at a time, then exit.
async fn run_writer(state: Arc<AppState>) {
    let shutdown = state.shutdown_receiver();
    while let Some(thread_id) = state.gist_queue.next() {
        if *shutdown.borrow() {
            state.gist_queue.finish(&thread_id, None);
            state.gist_queue.clear_pending();
            continue;
        }
        let started = Instant::now();
        let outcome = write_one(&state, &thread_id).await;
        let elapsed_ms = started.elapsed().as_millis();
        match outcome {
            Ok((gist, content_hash)) => {
                tracing::info!(
                    %thread_id,
                    elapsed_ms,
                    status = ?gist.status,
                    from_cache = gist.from_cache,
                    "list gist written"
                );
                match gist.status {
                    ThreadGistStatusData::Ready => {
                        state.gist_queue.finish(&thread_id, None);
                        announce(&state, gist);
                    }
                    ThreadGistStatusData::Failed => {
                        state.gist_queue.finish(&thread_id, Some(content_hash));
                    }
                    // The model went away or privacy now blocks it: the
                    // rest of the queue would fail the same way.
                    ThreadGistStatusData::Disabled | ThreadGistStatusData::Blocked => {
                        state.gist_queue.finish(&thread_id, None);
                        state.gist_queue.clear_pending();
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%thread_id, elapsed_ms, %error, "list gist failed");
                state.gist_queue.finish(&thread_id, None);
            }
        }
    }
}

/// One conversation's gist through the reader's path, with the background
/// budget, plus the content hash it was written for.
async fn write_one(
    state: &AppState,
    thread_id: &ThreadId,
) -> Result<(ThreadGistData, String), HandlerError> {
    load_and_write_gist(state, thread_id, false, GistCall::Background).await
}

fn announce(state: &AppState, gist: ThreadGistData) {
    emit_operation_event(state, DaemonEvent::ThreadGistReady { gist });
}
