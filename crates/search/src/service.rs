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
                        let result = apply_batch(&mut index, batch);
                        let _ = resp.send(result);
                    }
                    SearchCommand::Search {
                        query,
                        limit,
                        offset,
                        sort,
                        resp,
                    } => {
                        let _ = resp.send(index.search(&query, limit, offset, sort));
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

fn apply_batch(index: &mut SearchIndex, batch: SearchUpdateBatch) -> Result<(), MxrError> {
    for message_id in batch.removed_message_ids {
        index.remove_document(&message_id);
    }

    for entry in batch.entries {
        if let Some(body) = entry.body.as_ref() {
            index.index_body(&entry.envelope, body)?;
        } else {
            index.index_envelope(&entry.envelope)?;
        }
    }

    index.commit()
}

fn closed_error(error: mpsc::error::SendError<SearchCommand>) -> MxrError {
    MxrError::Search(format!("search service unavailable: {error}"))
}

fn worker_stopped() -> MxrError {
    MxrError::Search("search service worker stopped".to_string())
}
