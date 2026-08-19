use crate::{SearchIndex, SearchPage};
use mxr_core::id::MessageId;
use mxr_core::types::{Envelope, MessageBody, SortOrder};
use mxr_core::MxrError;
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
        (Self { tx }, handle)
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
