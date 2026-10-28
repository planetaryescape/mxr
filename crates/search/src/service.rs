use crate::{SearchIndex, SearchPage};
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, MessageBody, SortOrder};
use mxr_core::MxrError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tantivy::query::Query;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct SearchIndexEntry {
    pub envelope: Envelope,
    pub body: Option<MessageBody>,
    pub reply_later: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchUpdateBatch {
    pub entries: Vec<SearchIndexEntry>,
    pub removed_message_ids: Vec<MessageId>,
}

#[derive(Clone)]
pub struct SearchServiceHandle {
    tx: mpsc::Sender<SearchCommand>,
    rebuilds: Arc<AtomicU64>,
    /// Set while the index is known to hold only part of the mail: from the
    /// start of a rebuild until one finishes, and from startup when the index
    /// on disk is stale or short. A failed rebuild leaves it set.
    incomplete: Arc<AtomicBool>,
    clean_fallbacks: Arc<AtomicU64>,
}

/// Whether the lexical index can answer a search right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexAvailability {
    /// It can. `epoch` changes whenever a rebuild starts or ends, so a caller
    /// that read `Ready` before a query can check it still holds afterwards.
    Ready { epoch: u64 },
    /// The worker has stopped; only a daemon restart brings it back.
    Stopped,
    /// The index is being wiped and refilled from SQLite, so it holds part of
    /// the mail.
    Rebuilding,
    /// The last rebuild failed or was interrupted, so the index holds part of
    /// the mail. It stays here until a rebuild finishes.
    Incomplete,
}

/// Held while the index is wiped and refilled. Dropping it ends the rebuild.
/// Only [`RebuildGuard::finish`] makes the index complete again; a rebuild that
/// fails, or whose future is cancelled, drops the guard and leaves it
/// [`IndexAvailability::Incomplete`].
#[must_use = "the index counts as rebuilding only while this guard is alive"]
pub struct RebuildGuard {
    rebuilds: Arc<AtomicU64>,
    incomplete: Arc<AtomicBool>,
}

impl RebuildGuard {
    /// Call once the index holds every message. Ends the rebuild as complete.
    pub fn finish(self) {
        self.incomplete.store(false, Ordering::Release);
    }
}

// One word holds both the rebuilds in progress (low bits) and an epoch (high
// bits), so a reader sees both from a single load. Ending a rebuild adds
// `END_REBUILD`: epoch + 1 and rebuilds - 1 in one step.
const REBUILD_BITS: u32 = 16;
const REBUILD_MASK: u64 = (1 << REBUILD_BITS) - 1;
const START_REBUILD: u64 = (1 << REBUILD_BITS) + 1;
const END_REBUILD: u64 = (1 << REBUILD_BITS) - 1;

impl Drop for RebuildGuard {
    fn drop(&mut self) {
        self.rebuilds.fetch_add(END_REBUILD, Ordering::AcqRel);
    }
}

enum SearchCommand {
    ApplyBatch {
        batch: SearchUpdateBatch,
        resp: oneshot::Sender<Result<(), MxrError>>,
    },
    Search {
        query: String,
        account_id: Option<String>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
        resp: oneshot::Sender<Result<SearchPage, MxrError>>,
    },
    SearchAst {
        query: Box<dyn Query>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
        resp: oneshot::Sender<Result<SearchPage, MxrError>>,
    },
    Clear {
        resp: oneshot::Sender<Result<(), MxrError>>,
    },
    Commit {
        resp: oneshot::Sender<Result<(), MxrError>>,
    },
    NumDocs {
        resp: oneshot::Sender<Result<u64, MxrError>>,
    },
    Warm {
        resp: oneshot::Sender<Result<(), MxrError>>,
    },
    Shutdown {
        resp: oneshot::Sender<()>,
    },
}

impl SearchServiceHandle {
    pub fn start(index: SearchIndex) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<SearchCommand>(32);
        let clean_fallbacks = Arc::clone(&index.clean_fallbacks);
        let handle = tokio::spawn(async move {
            let mut index = index;
            while let Some(command) = rx.recv().await {
                match command {
                    SearchCommand::ApplyBatch { batch, resp } => {
                        // Indexing and committing are CPU-bound and, on an
                        // initial sync, run for minutes. Doing that inline
                        // parks a tokio worker thread for the whole run, so
                        // hand it to the blocking pool and take the index
                        // back afterwards.
                        let handle = tokio::task::spawn_blocking(move || {
                            let mut owned = index;
                            let result = apply_batch(&mut owned, batch);
                            (owned, result)
                        });
                        match handle.await {
                            Ok((returned, result)) => {
                                index = returned;
                                let _ = resp.send(result);
                            }
                            Err(error) => {
                                // The index went down with the panicking
                                // task; there is nothing left to serve. Only
                                // this one caller gets the error back, so log
                                // it — every later request just sees the
                                // channel closed.
                                tracing::error!(
                                    %error,
                                    "search index worker died; search is unavailable until the daemon restarts"
                                );
                                let _ = resp.send(Err(MxrError::Search(format!(
                                    "search index task failed: {error}"
                                ))));
                                break;
                            }
                        }
                    }
                    SearchCommand::Search {
                        query,
                        account_id,
                        limit,
                        offset,
                        sort,
                        resp,
                    } => {
                        let _ = resp.send(index.search_in_account(
                            &query,
                            account_id.as_deref(),
                            limit,
                            offset,
                            sort,
                        ));
                    }
                    SearchCommand::SearchAst {
                        query,
                        limit,
                        offset,
                        sort,
                        resp,
                    } => {
                        let _ = resp.send(index.search_ast(query, limit, offset, sort));
                    }
                    SearchCommand::Clear { resp } => {
                        let _ = resp.send(index.clear());
                    }
                    SearchCommand::Commit { resp } => {
                        let _ = resp.send(index.commit());
                    }
                    SearchCommand::NumDocs { resp } => {
                        let _ = resp.send(Ok(index.num_docs()));
                    }
                    SearchCommand::Warm { resp } => {
                        let _ = resp.send(index.warm());
                    }
                    SearchCommand::Shutdown { resp } => {
                        let _ = resp.send(());
                        break;
                    }
                }
            }
        });
        (
            Self {
                tx,
                rebuilds: Arc::new(AtomicU64::new(0)),
                incomplete: Arc::new(AtomicBool::new(false)),
                clean_fallbacks,
            },
            handle,
        )
    }

    /// Marks the index as rebuilding until the guard is dropped. Searches see
    /// [`IndexAvailability::Rebuilding`] in the meantime and can answer from
    /// SQLite instead of from a half-filled index.
    pub fn begin_rebuild(&self) -> RebuildGuard {
        // Set before the rebuild count, so a reader that sees the count also
        // sees the index as incomplete.
        self.incomplete.store(true, Ordering::Release);
        self.rebuilds.fetch_add(START_REBUILD, Ordering::AcqRel);
        RebuildGuard {
            rebuilds: Arc::clone(&self.rebuilds),
            incomplete: Arc::clone(&self.incomplete),
        }
    }

    /// Marks the index incomplete without a rebuild running, for an index found
    /// stale at startup. A rebuild that finishes clears it.
    pub fn mark_incomplete(&self) {
        self.incomplete.store(true, Ordering::Release);
    }

    /// Clears an incomplete mark when no rebuild is needed after all, such as
    /// when sync has filled the gap a short index had.
    pub fn clear_incomplete(&self) {
        self.incomplete.store(false, Ordering::Release);
    }

    /// Bodies indexed from their fallback text since the last call, because
    /// cleaning panicked on them. Counts only; no message content.
    pub fn take_clean_fallbacks(&self) -> u64 {
        self.clean_fallbacks.swap(0, Ordering::AcqRel)
    }

    pub fn availability(&self) -> IndexAvailability {
        // A stopped worker outranks a rebuild: waiting does not fix it.
        if self.worker_stopped() {
            return IndexAvailability::Stopped;
        }
        let state = self.rebuilds.load(Ordering::Acquire);
        if state & REBUILD_MASK != 0 {
            IndexAvailability::Rebuilding
        } else if self.incomplete.load(Ordering::Acquire) {
            IndexAvailability::Incomplete
        } else {
            IndexAvailability::Ready {
                epoch: state >> REBUILD_BITS,
            }
        }
    }

    /// True while the index is ready and no rebuild has started or ended since
    /// `availability` returned `Ready { epoch }`. A query that finished while
    /// this is false may have read a half-filled index and must be discarded.
    pub fn still_ready(&self, epoch: u64) -> bool {
        self.availability() == IndexAvailability::Ready { epoch }
    }

    pub async fn apply_batch(&self, batch: SearchUpdateBatch) -> Result<(), MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::ApplyBatch {
                batch,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Search {
                query: query.to_string(),
                account_id: None,
                limit,
                offset,
                sort,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn search_in_account(
        &self,
        query: &str,
        account_id: Option<&str>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Search {
                query: query.to_string(),
                account_id: account_id.map(ToString::to_string),
                limit,
                offset,
                sort,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn search_ast(
        &self,
        query: Box<dyn Query>,
        limit: usize,
        offset: usize,
        sort: SortOrder,
    ) -> Result<SearchPage, MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::SearchAst {
                query,
                limit,
                offset,
                sort,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn clear(&self) -> Result<(), MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Clear { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn commit(&self) -> Result<(), MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Commit { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn num_docs(&self) -> Result<u64, MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::NumDocs { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn warm(&self) -> Result<(), MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Warm { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    /// True once the worker task has stopped and search can no longer serve
    /// anything: the index moved into a `spawn_blocking` job that panicked, or
    /// the daemon asked it to shut down. Every later request fails with a
    /// closed channel, so callers that only ever hit the happy path never learn
    /// about it — health reporting reads this instead.
    ///
    /// There is no auto-restart: the index is owned by the dead task, and
    /// re-opening it safely means re-taking the tantivy writer lock and
    /// replaying whatever the panicking batch was doing. Restarting the daemon
    /// is the remedy today.
    pub fn worker_stopped(&self) -> bool {
        self.tx.is_closed()
    }

    pub async fn request_shutdown(&self) -> Result<(), MxrError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SearchCommand::Shutdown { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?;
        Ok(())
    }
}

/// Documents indexed between commits. Bounds the writer's in-memory
/// segment on a large batch; a full backfill batch would otherwise be
/// held until the single commit at the end.
const COMMIT_CHUNK: usize = 5_000;

fn apply_batch(index: &mut SearchIndex, batch: SearchUpdateBatch) -> Result<(), MxrError> {
    for message_id in batch.removed_message_ids {
        index.remove_document(&message_id);
    }

    let mut since_commit = 0;
    let mut indexed_any = false;
    for entry in batch.entries {
        if let Some(body) = entry.body.as_ref() {
            index.index_body_with_reply_later(&entry.envelope, body, entry.reply_later)?;
        } else {
            index.index_envelope_with_reply_later(&entry.envelope, entry.reply_later)?;
        }
        indexed_any = true;
        since_commit += 1;
        if since_commit == COMMIT_CHUNK {
            index.commit()?;
            since_commit = 0;
        }
    }

    // A chunk commit may have already flushed everything. Still commit when
    // the batch only removed documents — those deletes are uncommitted.
    if since_commit > 0 || !indexed_any {
        index.commit()?;
    }
    Ok(())
}

fn closed_error(error: mpsc::error::SendError<SearchCommand>) -> MxrError {
    MxrError::Search(format!("search service unavailable: {error}"))
}

fn worker_stopped() -> MxrError {
    MxrError::Search("search service worker stopped".to_string())
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::SearchIndex;

    fn ready_epoch(handle: &SearchServiceHandle) -> Option<u64> {
        match handle.availability() {
            IndexAvailability::Ready { epoch } => Some(epoch),
            _ => None,
        }
    }

    #[tokio::test]
    async fn a_live_worker_is_not_reported_as_stopped() {
        let (handle, _worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());

        assert!(!handle.worker_stopped());
        assert!(handle.num_docs().await.is_ok());
    }

    #[tokio::test]
    async fn a_worker_that_died_with_its_index_is_reported_as_stopped() {
        let (handle, worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());

        // Stands in for the index moving into a `spawn_blocking` job that
        // panicked: the task ends holding the index, and every later request
        // finds a closed channel.
        worker.abort();
        let _ = worker.await;

        assert!(handle.worker_stopped());
        assert!(handle.num_docs().await.is_err());
    }

    #[tokio::test]
    async fn a_rebuild_makes_the_index_unavailable_until_the_guard_drops() {
        let (handle, _worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());
        let epoch = ready_epoch(&handle).unwrap();

        let guard = handle.begin_rebuild();
        assert_eq!(handle.availability(), IndexAvailability::Rebuilding);
        assert!(!handle.still_ready(epoch));

        guard.finish();
        let after = ready_epoch(&handle).unwrap();
        // A query that straddled the rebuild must not pass for a clean one,
        // even though the index looks ready again.
        assert_ne!(epoch, after);
        assert!(!handle.still_ready(epoch));
        assert!(handle.still_ready(after));
    }

    #[tokio::test]
    async fn overlapping_rebuilds_keep_the_index_unavailable_until_the_last_ends() {
        let (handle, _worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());

        let first = handle.begin_rebuild();
        let second = handle.begin_rebuild();
        drop(first);
        assert_eq!(handle.availability(), IndexAvailability::Rebuilding);
        second.finish();
        assert!(matches!(
            handle.availability(),
            IndexAvailability::Ready { .. }
        ));
    }

    #[tokio::test]
    async fn a_rebuild_that_is_dropped_unfinished_stays_incomplete_until_one_finishes() {
        let (handle, _worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());

        drop(handle.begin_rebuild());
        assert_eq!(handle.availability(), IndexAvailability::Incomplete);
        assert!(handle.num_docs().await.is_ok());

        handle.begin_rebuild().finish();
        assert!(matches!(
            handle.availability(),
            IndexAvailability::Ready { .. }
        ));
    }

    #[tokio::test]
    async fn an_index_marked_incomplete_at_startup_is_not_ready_until_rebuilt() {
        let (handle, _worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());

        handle.mark_incomplete();
        assert_eq!(handle.availability(), IndexAvailability::Incomplete);

        handle.begin_rebuild().finish();
        assert!(matches!(
            handle.availability(),
            IndexAvailability::Ready { .. }
        ));
    }

    #[tokio::test]
    async fn a_stopped_worker_outranks_a_rebuild() {
        let (handle, worker) = SearchServiceHandle::start(SearchIndex::in_memory().unwrap());
        let _guard = handle.begin_rebuild();
        worker.abort();
        let _ = worker.await;

        assert_eq!(handle.availability(), IndexAvailability::Stopped);
    }
}
