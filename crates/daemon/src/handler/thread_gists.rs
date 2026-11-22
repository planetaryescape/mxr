use super::diagnostics_impl::emit_operation_event;
use super::gist_kind::{thread_kinds, GistStyle};
use super::mail_kind::SenderKind;
use super::thread_gist::{
    cached_gist, load_and_write_gist, newest_message_id, thread_envelopes, GistCall, GistPolicy,
    GistSetup,
};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use mxr_core::id::ThreadId;
use mxr_core::types::Envelope;
use mxr_protocol::{
    DaemonEvent, GistModelData, ResponseData, ThreadGistBatchData, ThreadGistData,
    ThreadGistSkipData, ThreadGistSkipReasonData, ThreadGistStatusData, THREAD_GISTS_MAX_BATCH,
};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PENDING_CAP: usize = 64;
const FAILED_BACKOFF: Duration = Duration::from_secs(10 * 60);
const MAX_WRITERS: usize = 8;
const FAILED_CAP: usize = 1024;
const STALE_RETRIES: u32 = 2;
const STALE_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Enqueued {
    Queued,
    InFlight,
    Dropped,
}

#[derive(Default)]
pub(crate) struct GistQueue {
    inner: Mutex<QueueState>,
}

#[derive(Default)]
struct QueueState {
    pending: VecDeque<ThreadId>,
    in_flight: HashSet<ThreadId>,
    writers: usize,
    failed: HashMap<ThreadId, (Option<String>, Instant)>,
    stale_attempts: HashMap<ThreadId, u32>,
}

impl GistQueue {
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

    fn clear_pending(&self) {
        self.inner.lock().pending.clear();
    }

    fn prune_failed(&self) {
        let now = Instant::now();
        self.inner
            .lock()
            .failed
            .retain(|_, (_, until)| *until > now);
    }

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
    pub(crate) fn snapshot(&self) -> (Vec<ThreadId>, Vec<ThreadId>, usize) {
        let state = self.inner.lock();
        (
            state.pending.iter().cloned().collect(),
            state.in_flight.iter().cloned().collect(),
            state.writers,
        )
    }
}

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
    automated: bool,
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
    if model == GistModelData::Disabled {
        return Ok(ResponseData::ThreadGists { batch });
    }

    state.gist_queue.prune_failed();
    let mut seen = HashSet::new();
    let mut found: Vec<Candidate> = Vec::new();
    for thread_id in thread_ids {
        if !seen.insert(thread_id) {
            continue;
        }
        let envelopes = thread_envelopes(state, thread_id).await?;
        if envelopes.is_empty() {
            batch
                .skipped
                .push(skip(thread_id, ThreadGistSkipReasonData::NotFound));
        } else {
            found.push(Candidate {
                thread_id: thread_id.clone(),
                envelopes,
            });
        }
    }
    let kinds = thread_kinds(
        state,
        &found
            .iter()
            .map(|candidate| (&candidate.thread_id, candidate.envelopes.as_slice()))
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut missing: Vec<(Candidate, SenderKind)> = Vec::new();
    for candidate in found {
        let Some(kind) = kinds.get(&candidate.thread_id).copied() else {
            batch.skipped.push(skip(
                &candidate.thread_id,
                ThreadGistSkipReasonData::NotFound,
            ));
            continue;
        };
        let Some(setup) = GistSetup::new(
            &policy,
            &candidate.thread_id,
            &candidate.envelopes,
            GistStyle::of(kind),
        ) else {
            continue;
        };
        if let Some(gist) = cached_gist(state, &candidate.thread_id, &setup).await? {
            batch.gists.push(gist);
        } else if !generate || model != GistModelData::Available {
            batch.skipped.push(skip(
                &candidate.thread_id,
                ThreadGistSkipReasonData::NotGenerated,
            ));
        } else if state
            .gist_queue
            .recently_failed(&candidate.thread_id, &setup.content_hash)
        {
            batch.skipped.push(skip(
                &candidate.thread_id,
                ThreadGistSkipReasonData::RecentlyFailed,
            ));
        } else {
            missing.push((candidate, kind));
        }
    }

    if !missing.is_empty() {
        let mut wanted = Vec::new();
        for (candidate, kind) in missing {
            if kind == SenderKind::Person || (automated && kind == SenderKind::Automated) {
                wanted.push(candidate.thread_id);
            } else {
                batch.skipped.push(skip(
                    &candidate.thread_id,
                    ThreadGistSkipReasonData::NotPeople,
                ));
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
