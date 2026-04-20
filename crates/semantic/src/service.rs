use crate::{SemanticEngine, SemanticHit};
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
use tokio::sync::{mpsc, oneshot};
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

impl SemanticServiceHandle {
    pub fn start(engine: SemanticEngine) -> (Self, JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<SemanticCommand>(32);
        let runtime_metrics = Arc::new(Mutex::new(SemanticRuntimeMetrics::default()));
        let worker_metrics = runtime_metrics.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine;
            let mut pending = VecDeque::<PendingIngest>::new();
            let mut pending_ids = HashSet::<MessageId>::new();

            loop {
                while let Ok(command) = rx.try_recv() {
                    if handle_command(
                        &mut engine,
                        &mut pending,
                        &mut pending_ids,
                        &worker_metrics,
                        command,
                    )
                    .await
                    .is_err()
                    {
                        return;
                    }
                }

                if let Some(next) = pending.pop_front() {
                    pending_ids.remove(&next.message_id);
                    let queue_wait = next.enqueued_at.elapsed();
                    if let Ok(mut metrics) = worker_metrics.lock() {
                        metrics.queue_depth = pending.len() as u32;
                        metrics.in_flight = 1;
                        metrics.last_queue_wait_ms = Some(queue_wait.as_millis() as u64);
                    }
                    let started_at = Instant::now();
                    let result = engine
                        .ingest_messages(std::slice::from_ref(&next.message_id))
                        .await;
                    if let Ok(mut metrics) = worker_metrics.lock() {
                        metrics.queue_depth = pending.len() as u32;
                        metrics.in_flight = 0;
                    }
                    match result {
                        Ok(()) => {
                            tracing::trace!(
                                message_id = %next.message_id,
                                queue_wait_ms = queue_wait.as_secs_f64() * 1000.0,
                                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                                "semantic background ingest processed"
                            );
                        }
                        Err(error) => {
                            tracing::error!(
                                message_id = %next.message_id,
                                queue_wait_ms = queue_wait.as_secs_f64() * 1000.0,
                                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                                "semantic background ingest failed: {error}"
                            );
                        }
                    }
                    continue;
                }

                let Some(command) = rx.recv().await else {
                    break;
                };
                if handle_command(
                    &mut engine,
                    &mut pending,
                    &mut pending_ids,
                    &worker_metrics,
                    command,
                )
                .await
                .is_err()
                {
                    break;
                }
            }
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
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(SemanticCommand::ReindexActive { resp: resp_tx })
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
        SemanticCommand::UseProfile { profile, resp } => {
            let _ = resp.send(engine.use_profile(profile).await);
        }
        SemanticCommand::InstallProfile { profile, resp } => {
            let _ = resp.send(engine.install_profile(profile).await);
        }
        SemanticCommand::ReindexActive { resp } => {
            let _ = resp.send(engine.reindex_active().await);
        }
        SemanticCommand::IngestMessages { message_ids, resp } => {
            let _ = resp.send(engine.ingest_messages(&message_ids).await);
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

fn closed_error<T>(_error: mpsc::error::SendError<T>) -> anyhow::Error {
    anyhow!("semantic service unavailable")
}

fn worker_stopped() -> anyhow::Error {
    anyhow!("semantic service worker stopped")
}
