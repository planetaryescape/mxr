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
    cached_gist, load_and_write_gist, newest_message_id, thread_envelopes, GistCall, GistPolicy,
    GistSetup,
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
/// Most backoffs remembered; the soonest to expire go first.
const FAILED_CAP: usize = 1024;
/// A conversation whose gist goes stale mid-call (someone keeps writing)
/// is tried again at the back of the queue this many times, then waits
/// `STALE_BACKOFF`, so it can't hold the writer.
const STALE_RETRIES: u32 = 2;
const STALE_BACKOFF: Duration = Duration::from_secs(60);

/// What happened to one conversation a request asked to queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Enqueued {
    Queued,
    InFlight,
    /// Past the cap: older requests are ahead of it.
    Dropped,
}

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
    /// Conversations backing off, until when: the model failed on this
    /// content (`Some(hash)`), or its gists kept going stale (`None`, any
    /// content).
    failed: HashMap<ThreadId, (Option<String>, Instant)>,
    /// Times each conversation's gist went stale mid-call in a row.
    stale_attempts: HashMap<ThreadId, u32>,
}

impl GistQueue {
    /// Put `ids` at the front of the queue, first id first, skipping any
    /// already being written; drop the oldest requests past the cap. Says
    /// what happened to each id, and how many writers to start so that up
    /// to `writers` run.
    fn enqueue(&self, ids: &[ThreadId], writers: usize) -> (Vec<Enqueued>, usize) {
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
        let outcomes = ids
            .iter()
            .map(|id| {
                if state.in_flight.contains(id) {
                    Enqueued::InFlight
                } else if state.pending.contains(id) {
                    Enqueued::Queued
                } else {
                    Enqueued::Dropped
                }
            })
            .collect();
        let start = writers
            .saturating_sub(state.writers)
            .min(state.pending.len());
        state.writers += start;
        (outcomes, start)
    }

    /// Its gist went stale while the model was writing: back of the queue
    /// for another go, or, after `STALE_RETRIES`, a backoff. Never the
    /// front, so a busy conversation can't starve the rest.
    fn went_stale(&self, id: &ThreadId, now: Instant) {
        let mut state = self.inner.lock();
        state.in_flight.remove(id);
        if state.stale_attempts.len() >= FAILED_CAP {
            state.stale_attempts.clear();
        }
        let attempts = state.stale_attempts.entry(id.clone()).or_default();
        *attempts += 1;
        if *attempts <= STALE_RETRIES {
            if !state.pending.contains(id) && state.pending.len() < PENDING_CAP {
                state.pending.push_back(id.clone());
            }
        } else {
            state.stale_attempts.remove(id);
            back_off(&mut state, id, None, now + STALE_BACKOFF);
        }
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
        self.finish_at(id, failed_at, Instant::now());
    }

    fn finish_at(&self, id: &ThreadId, failed_at: Option<String>, now: Instant) {
        let mut state = self.inner.lock();
        state.in_flight.remove(id);
        state.stale_attempts.remove(id);
        match failed_at {
            Some(hash) => back_off(&mut state, id, Some(hash), now + FAILED_BACKOFF),
            None => {
                state.failed.remove(id);
            }
        }
    }

    /// Nothing more can be written (no model, privacy block, shutdown).
    fn clear_pending(&self) {
        self.inner.lock().pending.clear();
    }

    /// Forget backoffs that have run out; once per request.
    fn prune_failed(&self) {
        let now = Instant::now();
        self.inner
            .lock()
            .failed
            .retain(|_, (_, until)| *until > now);
    }

    /// Backing off: the model failed on this content a moment ago, or the
    /// conversation's gists kept going stale.
    fn recently_failed(&self, id: &ThreadId, content_hash: &str) -> bool {
        let now = Instant::now();
        self.inner
            .lock()
            .failed
            .get(id)
            .is_some_and(|(hash, until)| {
                *until > now && hash.as_deref().is_none_or(|hash| hash == content_hash)
            })
    }

    #[cfg(test)]
    fn failed_len(&self) -> usize {
        self.inner.lock().failed.len()
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

/// Remember a backoff, bounded: expired entries go on every insert, then
/// the soonest to expire while the map is full.
fn back_off(state: &mut QueueState, id: &ThreadId, hash: Option<String>, until: Instant) {
    let now = Instant::now();
    state.failed.retain(|_, (_, at)| *at > now);
    while state.failed.len() >= FAILED_CAP && !state.failed.contains_key(id) {
        let Some(soonest) = state
            .failed
            .iter()
            .min_by_key(|(_, (_, at))| *at)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        state.failed.remove(&soonest);
    }
    state.failed.insert(id.clone(), (hash, until));
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
        in_flight: Vec::new(),
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
        let mut wanted = Vec::new();
        for candidate in missing {
            match people.get(&candidate.thread_id) {
                Some(true) => wanted.push(candidate.thread_id),
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
        let (outcomes, start) = state.gist_queue.enqueue(&wanted, writers);
        for (thread_id, outcome) in wanted.into_iter().zip(outcomes) {
            match outcome {
                Enqueued::Queued => batch.queued.push(thread_id),
                Enqueued::InFlight => batch.in_flight.push(thread_id),
                Enqueued::Dropped => batch
                    .skipped
                    .push(skip(&thread_id, ThreadGistSkipReasonData::QueueFull)),
            }
        }
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
            .map(|envelope| envelope.from.email.clone())
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
                        // Announce only a gist for the conversation as it
                        // is now: a message that landed during the call
                        // makes it stale, and the conversation goes back
                        // to the front for another go.
                        let now = newest_message_id(&state, &thread_id).await.ok().flatten();
                        if now.is_some() && now == gist.newest_message_id {
                            state.gist_queue.finish(&thread_id, None);
                            announce(&state, gist);
                        } else {
                            tracing::info!(%thread_id, "list gist went stale mid-call");
                            state.gist_queue.went_stale(&thread_id, Instant::now());
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_map_is_bounded_and_forgets_expired_failures() {
        let queue = GistQueue::default();
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(5));
        let ids: Vec<ThreadId> = (0..FAILED_CAP + 6).map(|_| ThreadId::new()).collect();
        for (offset, id) in ids.iter().enumerate() {
            let at = start + Duration::from_millis(offset as u64);
            queue.finish_at(id, Some("hash".into()), at);
        }
        assert_eq!(queue.failed_len(), FAILED_CAP);
        assert!(
            !queue.recently_failed(&ids[0], "hash"),
            "the oldest went first"
        );
        assert!(queue.recently_failed(ids.last().unwrap(), "hash"));

        // Once they run out, the next insert clears everything expired.
        queue
            .inner
            .lock()
            .failed
            .values_mut()
            .for_each(|(_, until)| *until = start);
        queue.finish_at(&ThreadId::new(), Some("hash".into()), Instant::now());
        assert_eq!(queue.failed_len(), 1);
    }

    #[test]
    fn enqueue_says_what_happened_to_each_conversation() {
        let queue = GistQueue::default();
        let ids: Vec<ThreadId> = (0..PENDING_CAP + 2).map(|_| ThreadId::new()).collect();
        let (outcomes, start) = queue.enqueue(&ids, 1);
        assert_eq!(start, 1);
        assert_eq!(
            outcomes[..PENDING_CAP],
            vec![Enqueued::Queued; PENDING_CAP][..]
        );
        assert_eq!(
            outcomes[PENDING_CAP..],
            [Enqueued::Dropped, Enqueued::Dropped]
        );
        let taken = queue.next().unwrap();
        assert_eq!(taken, ids[0]);
        let (again, start) = queue.enqueue(std::slice::from_ref(&taken), 1);
        assert_eq!((again, start), (vec![Enqueued::InFlight], 0));
    }
}
