use crate::{SemanticEngine, SemanticHit, SemanticIndexJob};
use anyhow::{anyhow, Result};
use mxr_config::SemanticConfig;
use mxr_core::id::MessageId;
use mxr_core::types::{
    SemanticChunkSourceKind, SemanticProfile, SemanticProfileRecord, SemanticRuntimeMetrics,
    SemanticStatusSnapshot,
};
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, Semaphore};
use tokio::task::JoinHandle;

#[cfg(feature = "local")]
type TestEmbedder =
    std::sync::Arc<dyn Fn(SemanticProfile, &[String]) -> Result<Vec<Vec<f32>>> + Send + Sync>;

#[derive(Clone)]
pub struct SemanticServiceHandle {
    tx: mpsc::Sender<SemanticCommand>,
    runtime_metrics: Arc<Mutex<SemanticRuntimeMetrics>>,
}

enum SemanticCommand {
    ApplyConfig {
        config: SemanticConfig,
        resp: oneshot::Sender<Result<()>>,
    },
    StatusSnapshot {
        resp: oneshot::Sender<Result<SemanticStatusSnapshot>>,
    },
    UseProfile {
        profile: SemanticProfile,
        resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    },
    InstallProfile {
        profile: SemanticProfile,
        resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    },
    ReindexActive {
        force: bool,
        resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    },
    BackfillActive {
        resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    },
    BackfillActiveLimited {
        limit: u32,
        resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    },
    IngestMessages {
        message_ids: Vec<MessageId>,
        resp: oneshot::Sender<Result<()>>,
    },
    EnqueueIngest {
        message_ids: Vec<MessageId>,
        resp: oneshot::Sender<Result<()>>,
    },
    Search {
        query: String,
        limit: usize,
        allowed_source_kinds: Vec<SemanticChunkSourceKind>,
        resp: oneshot::Sender<Result<Vec<SemanticHit>>>,
    },
    Shutdown {
        resp: oneshot::Sender<()>,
    },
    #[cfg(feature = "local")]
    SetTestEmbedder {
        embedder: TestEmbedder,
        resp: oneshot::Sender<Result<()>>,
    },
}

struct PendingIngest {
    message_id: MessageId,
    enqueued_at: Instant,
}

/// The index pass the worker is stepping through, plus everyone waiting on
/// it. Only one runs at a time: they all rewrite the same profile row, so a
/// second pass would corrupt the first one's status and progress.
struct ActivePass {
    request: PassRequest,
    job: SemanticIndexJob,
    waiters: Vec<oneshot::Sender<Result<SemanticProfileRecord>>>,
}

/// What was asked for, so a duplicate request can join instead of being
/// refused.
#[derive(Clone, Copy, Eq, PartialEq)]
enum PassRequest {
    Reindex {
        force: bool,
    },
    UseProfile(SemanticProfile),
    /// The limit is part of the identity: a caller asking for 200 messages
    /// must not be handed the result of a 10,000-message pass it did not ask
    /// for, so only an identical request joins.
    Backfill {
        limit: u32,
    },
}

impl PassRequest {
    fn describe(self) -> &'static str {
        match self {
            Self::Reindex { .. } => "reindex",
            Self::UseProfile(_) => "profile activation",
            Self::Backfill { .. } => "backfill",
        }
    }
}

fn pass_busy_error(active: PassRequest, requested: PassRequest) -> anyhow::Error {
    anyhow!(
        "semantic {} already running; retry the {} when it finishes",
        active.describe(),
        requested.describe()
    )
}

/// Messages pulled off the ingest queue per iteration. Batching them lets the
/// engine fill embedding calls with chunks from many messages while keeping
/// each iteration short enough to stay responsive to commands.
const INGEST_BATCH_MESSAGES: usize = 64;

/// Messages an unbounded `BackfillActive` covers.
const DEFAULT_BACKFILL_LIMIT: u32 = 10_000;

/// Fans one pass outcome out to every caller waiting on it.
///
/// `anyhow::Error` does not clone, so only one waiter can be handed the
/// original. That goes to the first - the caller whose request started the
/// pass; anyone who joined it later gets the rendered message.
fn respond_all(
    waiters: Vec<oneshot::Sender<Result<SemanticProfileRecord>>>,
    result: Result<SemanticProfileRecord>,
) {
    let mut waiters = waiters.into_iter();
    let Some(starter) = waiters.next() else {
        return;
    };
    for waiter in waiters {
        let _ = waiter.send(match &result {
            Ok(record) => Ok(record.clone()),
            Err(error) => Err(anyhow!("{error:#}")),
        });
    }
    let _ = starter.send(result);
}

impl SemanticServiceHandle {
    /// `background_db` gates this worker's DB-touching work below the
    /// reader-pool size so background ingest can never starve
    /// interactive/status queries of connections. A permit is held across
    /// each unit of work (embedding compute + writes) the worker runs: one
    /// ingest batch, or one page of a reindex.
    pub fn start(engine: SemanticEngine, background_db: Arc<Semaphore>) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<SemanticCommand>(32);
        let runtime_metrics = Arc::new(Mutex::new(SemanticRuntimeMetrics::default()));
        let worker_metrics = runtime_metrics.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine;
            if let Err(error) = engine.reclaim_interrupted_index_passes().await {
                tracing::warn!("could not reclaim interrupted semantic passes: {error}");
            }
            let mut pending = VecDeque::<PendingIngest>::new();
            let mut pending_ids = HashSet::<MessageId>::new();
            let mut pass: Option<ActivePass> = None;

            loop {
                while let Ok(command) = rx.try_recv() {
                    if handle_command(
                        &mut engine,
                        &mut pending,
                        &mut pending_ids,
                        &mut pass,
                        &worker_metrics,
                        command,
                    )
                    .await
                    .is_err()
                    {
                        abandon_pass(&mut engine, pass.take()).await;
                        return;
                    }
                }

                // One page of an in-flight reindex, then back to the command
                // drain: a full pass would otherwise hide status and search
                // for its whole duration. Ingest waits until it finishes so
                // the two paths never fight over the profile record.
                let step = if let Some(active) = pass.as_mut() {
                    let _bg_permit = background_db.acquire().await;
                    Some(engine.index_job_step(&mut active.job).await)
                } else {
                    None
                };
                if let Some(step) = step {
                    match step {
                        Ok(true) => {}
                        Ok(false) => {
                            if let Some(active) = pass.take() {
                                let result = engine.finish_index_job(active.job).await;
                                respond_all(active.waiters, result);
                            }
                        }
                        Err(error) => {
                            tracing::error!("semantic index pass failed: {error}");
                            if let Some(active) = pass.take() {
                                engine.fail_index_job(active.job, &error).await;
                                respond_all(active.waiters, Err(error));
                            }
                        }
                    }
                    continue;
                }

                if !pending.is_empty() {
                    let batch = pending
                        .drain(..INGEST_BATCH_MESSAGES.min(pending.len()))
                        .collect::<Vec<_>>();
                    let queue_wait = batch
                        .iter()
                        .map(|item| item.enqueued_at.elapsed())
                        .max()
                        .unwrap_or_default();
                    let message_ids = batch
                        .into_iter()
                        .map(|item| {
                            pending_ids.remove(&item.message_id);
                            item.message_id
                        })
                        .collect::<Vec<_>>();
                    if let Ok(mut metrics) = worker_metrics.lock() {
                        metrics.queue_depth = pending.len() as u32;
                        metrics.in_flight = message_ids.len() as u32;
                        metrics.last_queue_wait_ms = Some(queue_wait.as_millis() as u64);
                    }
                    let started_at = Instant::now();
                    // Reserve background-DB headroom: hold a permit across
                    // the ingest unit so concurrent background work can't
                    // exhaust the reader pool and starve interactive queries.
                    let _bg_permit = background_db.acquire().await;
                    let result = engine.ingest_messages(&message_ids).await;
                    if let Ok(mut metrics) = worker_metrics.lock() {
                        metrics.queue_depth = pending.len() as u32;
                        metrics.in_flight = 0;
                    }
                    match result {
                        Ok(()) => {
                            tracing::trace!(
                                messages = message_ids.len(),
                                queue_wait_ms = queue_wait.as_secs_f64() * 1000.0,
                                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                                "semantic background ingest processed"
                            );
                        }
                        Err(error) => {
                            tracing::error!(
                                messages = message_ids.len(),
                                queue_wait_ms = queue_wait.as_secs_f64() * 1000.0,
                                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                                "semantic background ingest failed: {error}"
                            );
                        }
                    }
                    continue;
                }

                // Backlog drained: this is when the deferred ANN rebuild
                // starts. It runs on a blocking thread, so the loop keeps
                // answering status and search while a large index is built.
                if let Err(error) = engine.poll_index_builds().await {
                    tracing::warn!("semantic index rebuild failed to start: {error}");
                }

                // A build waiting out its retry backoff needs the loop to
                // wake itself; nothing else will.
                let retry_at = engine.next_index_build_retry();
                let command = tokio::select! {
                    biased;
                    command = rx.recv() => command,
                    // Swapping a finished index in is the loop's own work, so
                    // it has to be woken for it even with no command pending.
                    () = engine.index_build_ready() => continue,
                    () = sleep_until_opt(retry_at) => continue,
                };
                let Some(command) = command else {
                    break;
                };
                if handle_command(
                    &mut engine,
                    &mut pending,
                    &mut pending_ids,
                    &mut pass,
                    &worker_metrics,
                    command,
                )
                .await
                .is_err()
                {
                    break;
                }
            }
            abandon_pass(&mut engine, pass.take()).await;
        });
        (
            Self {
                tx,
                runtime_metrics,
            },
            handle,
        )
    }

    pub async fn apply_config(&self, config: SemanticConfig) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::ApplyConfig {
                config,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn status_snapshot(&self) -> Result<SemanticStatusSnapshot> {
        let runtime_metrics = self
            .runtime_metrics
            .lock()
            .map_err(|_| anyhow!("semantic runtime metrics lock poisoned"))?
            .clone();
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::StatusSnapshot { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        let mut snapshot = resp_rx.await.map_err(|_| worker_stopped())??;
        snapshot.runtime.queue_depth = runtime_metrics.queue_depth;
        snapshot.runtime.in_flight = runtime_metrics.in_flight;
        if runtime_metrics.last_queue_wait_ms.is_some() {
            snapshot.runtime.last_queue_wait_ms = runtime_metrics.last_queue_wait_ms;
        }
        Ok(snapshot)
    }

    pub async fn use_profile(&self, profile: SemanticProfile) -> Result<SemanticProfileRecord> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::UseProfile {
                profile,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn install_profile(&self, profile: SemanticProfile) -> Result<SemanticProfileRecord> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::InstallProfile {
                profile,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn reindex_active(&self) -> Result<SemanticProfileRecord> {
        self.reindex(false).await
    }

    /// Reindex that re-embeds every message even when its stored vectors look
    /// current: the recovery path for a corrupt or mixed-model index.
    pub async fn reindex(&self, force: bool) -> Result<SemanticProfileRecord> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::ReindexActive {
                force,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn backfill_active(&self) -> Result<SemanticProfileRecord> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::BackfillActive { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn backfill_active_limited(&self, limit: u32) -> Result<SemanticProfileRecord> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::BackfillActiveLimited {
                limit,
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn ingest_messages(&self, message_ids: &[MessageId]) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::IngestMessages {
                message_ids: message_ids.to_vec(),
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn enqueue_ingest_messages(&self, message_ids: &[MessageId]) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::EnqueueIngest {
                message_ids: message_ids.to_vec(),
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
        allowed_source_kinds: &[SemanticChunkSourceKind],
    ) -> Result<Vec<SemanticHit>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::Search {
                query: query.to_string(),
                limit,
                allowed_source_kinds: allowed_source_kinds.to_vec(),
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }

    pub async fn request_shutdown(&self) -> Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::Shutdown { resp: resp_tx })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?;
        Ok(())
    }

    #[cfg(feature = "local")]
    #[doc(hidden)]
    pub async fn set_test_embedder<F>(&self, embedder: F) -> Result<()>
    where
        F: Fn(SemanticProfile, &[String]) -> Result<Vec<Vec<f32>>> + Send + Sync + 'static,
    {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::SetTestEmbedder {
                embedder: std::sync::Arc::new(embedder),
                resp: resp_tx,
            })
            .await
            .map_err(closed_error)?;
        resp_rx.await.map_err(|_| worker_stopped())?
    }
}

async fn handle_command(
    engine: &mut SemanticEngine,
    pending: &mut VecDeque<PendingIngest>,
    pending_ids: &mut HashSet<MessageId>,
    pass: &mut Option<ActivePass>,
    runtime_metrics: &Arc<Mutex<SemanticRuntimeMetrics>>,
    command: SemanticCommand,
) -> Result<()> {
    match command {
        SemanticCommand::ApplyConfig { config, resp } => {
            engine.apply_config(config);
            let _ = resp.send(Ok(()));
        }
        SemanticCommand::StatusSnapshot { resp } => {
            let _ = resp.send(engine.status_snapshot().await);
        }
        SemanticCommand::InstallProfile { profile, resp } => {
            let _ = resp.send(engine.install_profile(profile).await);
        }
        SemanticCommand::ReindexActive { force, resp } => {
            let request = PassRequest::Reindex { force };
            if let Some(resp) = claim_pass(pass, request, resp) {
                let begun = engine.begin_reindex(force).await;
                install_pass(pass, request, resp, begun);
            }
        }
        SemanticCommand::UseProfile { profile, resp } => {
            let request = PassRequest::UseProfile(profile);
            if let Some(resp) = claim_pass(pass, request, resp) {
                let begun = engine.begin_use_profile(profile).await;
                install_pass(pass, request, resp, begun);
            }
        }
        SemanticCommand::BackfillActive { resp } => {
            let request = PassRequest::Backfill {
                limit: DEFAULT_BACKFILL_LIMIT,
            };
            if let Some(resp) = claim_pass(pass, request, resp) {
                let begun = engine.begin_backfill(DEFAULT_BACKFILL_LIMIT).await;
                install_pass(pass, request, resp, begun);
            }
        }
        SemanticCommand::BackfillActiveLimited { limit, resp } => {
            let request = PassRequest::Backfill { limit };
            if let Some(resp) = claim_pass(pass, request, resp) {
                let begun = engine.begin_backfill(limit).await;
                install_pass(pass, request, resp, begun);
            }
        }
        SemanticCommand::IngestMessages { message_ids, resp } => {
            // A synchronous ingest would write its own Ready status over the
            // running pass's Indexing/progress, so refuse instead of lying.
            // Callers that can wait should enqueue: the queue drains once the
            // reindex finishes.
            let result = if let Some(active) = pass.as_ref() {
                Err(anyhow!(
                    "semantic {} in progress; enqueue the ingest or retry when it finishes",
                    active.request.describe()
                ))
            } else {
                engine.ingest_messages(&message_ids).await
            };
            let _ = resp.send(result);
        }
        SemanticCommand::EnqueueIngest { message_ids, resp } => {
            let enqueued_at = Instant::now();
            for message_id in message_ids {
                if pending_ids.insert(message_id.clone()) {
                    pending.push_back(PendingIngest {
                        message_id,
                        enqueued_at,
                    });
                }
            }
            if let Ok(mut metrics) = runtime_metrics.lock() {
                metrics.queue_depth = pending.len() as u32;
            }
            tracing::trace!(queued = pending.len(), "semantic ingest enqueued");
            let _ = resp.send(Ok(()));
        }
        SemanticCommand::Search {
            query,
            limit,
            allowed_source_kinds,
            resp,
        } => {
            let _ = resp.send(engine.search(&query, limit, &allowed_source_kinds).await);
        }
        SemanticCommand::Shutdown { resp } => {
            let _ = resp.send(());
            return Err(anyhow!("semantic shutdown requested"));
        }
        #[cfg(feature = "local")]
        SemanticCommand::SetTestEmbedder { embedder, resp } => {
            engine.set_test_embedder(embedder);
            let _ = resp.send(Ok(()));
        }
    }
    Ok(())
}

/// Decides what to do with a pass request while another may be running.
///
/// An identical request joins the live pass; a different one is refused,
/// because index passes all rewrite the same profile row and overlapping them
/// corrupts its status and progress. Returns the responder only when the
/// caller should begin a new pass.
fn claim_pass(
    pass: &mut Option<ActivePass>,
    request: PassRequest,
    resp: oneshot::Sender<Result<SemanticProfileRecord>>,
) -> Option<oneshot::Sender<Result<SemanticProfileRecord>>> {
    match pass.as_mut() {
        Some(active) if active.request == request => {
            active.waiters.push(resp);
            None
        }
        Some(active) => {
            let _ = resp.send(Err(pass_busy_error(active.request, request)));
            None
        }
        None => Some(resp),
    }
}

fn install_pass(
    pass: &mut Option<ActivePass>,
    request: PassRequest,
    resp: oneshot::Sender<Result<SemanticProfileRecord>>,
    begun: Result<SemanticIndexJob>,
) {
    match begun {
        Ok(job) => {
            *pass = Some(ActivePass {
                request,
                job,
                waiters: vec![resp],
            });
        }
        Err(error) => {
            let _ = resp.send(Err(error));
        }
    }
}

/// Waits until `deadline`, or forever when there is nothing to wait for.
async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
        None => std::future::pending::<()>().await,
    }
}

/// Marks a pass the worker will never finish as failed, so its profile row
/// does not sit on `Indexing` until the next daemon start notices it.
async fn abandon_pass(engine: &mut SemanticEngine, pass: Option<ActivePass>) {
    let Some(active) = pass else {
        return;
    };
    let error = anyhow!("semantic worker stopped before the index pass finished");
    engine.fail_index_job(active.job, &error).await;
    respond_all(active.waiters, Err(error));
}

fn closed_error<T>(_error: mpsc::error::SendError<T>) -> anyhow::Error {
    anyhow!("semantic service unavailable")
}

fn worker_stopped() -> anyhow::Error {
    anyhow!("semantic service worker stopped")
}
