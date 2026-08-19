mod cpu;
mod service;

#[cfg(feature = "local")]
use anyhow::Context;
use anyhow::{anyhow, Result};
#[cfg(feature = "local")]
use calamine::{open_workbook_auto, Reader};
#[cfg(feature = "local")]
use cpu::CpuExecutor;
#[cfg(all(test, feature = "local"))]
use cpu::CpuObserver;
#[cfg(feature = "local")]
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
#[cfg(feature = "local")]
use hnsw_rs::prelude::{DistCosine, Hnsw};
use mxr_config::SemanticConfig;
#[cfg(feature = "local")]
use mxr_core::id::SemanticProfileId;
use mxr_core::id::{MessageId, SemanticChunkId};
#[cfg(feature = "local")]
use mxr_core::types::{
    AttachmentMeta, Envelope, MessageBody, SemanticChunkRecord, SemanticEmbeddingRecord,
    SemanticEmbeddingStatus, SemanticProfileStatus,
};
use mxr_core::types::{
    SearchMode, SemanticChunkSourceKind, SemanticProfile, SemanticProfileRecord,
    SemanticRuntimeMetrics, SemanticStatusSnapshot,
};
#[cfg(feature = "local")]
use mxr_reader::{clean, ReaderConfig};
use mxr_store::Store;
#[cfg(feature = "local")]
use sha2::{Digest, Sha256};
#[cfg(feature = "local")]
use std::collections::{HashMap, HashSet};
use std::path::Path;
#[cfg(feature = "local")]
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
#[cfg(feature = "local")]
use std::sync::Mutex;
#[cfg(feature = "local")]
use std::time::{Duration, Instant};

pub use service::SemanticServiceHandle;

#[cfg(feature = "local")]
const FASTEMBED_REVISION: &str = "fastembed-5.13.0";

/// Messages an index job processes per step. Bounds peak memory (envelopes,
/// bodies and chunks for one page) and how long the semantic worker can go
/// without draining status/search commands.
#[cfg(feature = "local")]
const INDEX_PAGE_MESSAGES: usize = 64;

/// Chunk texts handed to one embedding call. One message yields ~2-3 chunks,
/// and an ONNX run that narrow is dominated by per-run overhead, so calls are
/// filled with chunks spanning many messages.
#[cfg(feature = "local")]
const EMBED_TEXTS_PER_CALL: usize = 512;

/// Message ids read per keyset page when collecting the full id list.
#[cfg(feature = "local")]
const MESSAGE_ID_PAGE: u32 = 5_000;

/// Rows per ONNX session run inside one embedding call. Measured on a 2k
/// message corpus: 16 and 64 rows are both ~5-15% slower than 32.
#[cfg(feature = "local")]
const EMBED_MODEL_BATCH: usize = 32;

#[derive(Debug, Clone)]
pub struct SemanticHit {
    pub message_id: MessageId,
    pub score: f32,
    pub chunk_id: SemanticChunkId,
    pub source_kind: SemanticChunkSourceKind,
    pub snippet: String,
}

#[cfg(feature = "local")]
struct IndexedChunk {
    chunk_id: SemanticChunkId,
    message_id: MessageId,
    source_kind: SemanticChunkSourceKind,
    normalized: String,
}

#[cfg(feature = "local")]
struct SemanticIndex {
    hnsw: Hnsw<'static, f32, DistCosine>,
    chunks_by_id: HashMap<usize, IndexedChunk>,
}

#[cfg(feature = "local")]
struct MessageChunkBatch {
    message_id: MessageId,
    chunks: Vec<SemanticChunkRecord>,
    /// Whether the stored chunk rows had to be rewritten. A rewrite cascades
    /// away the message's embeddings, so it always forces a re-embed.
    chunks_changed: bool,
}

/// Where an index job takes chunk text from.
#[cfg(feature = "local")]
#[derive(Clone, Copy)]
enum ChunkSource {
    /// Re-extract from the message and rewrite chunks that drifted.
    Message,
    /// Reuse stored chunks; only extract for messages that have none. Chunks
    /// are profile-independent, so switching or backfilling a profile does not
    /// need to redo extraction.
    Stored,
}

/// A resumable full-profile indexing pass.
///
/// Stepping it one page at a time lets the semantic worker answer status and
/// search commands between pages instead of disappearing for the length of a
/// whole reindex.
pub(crate) struct SemanticIndexJob {
    #[cfg(feature = "local")]
    inner: IndexJobInner,
}

#[cfg(feature = "local")]
struct IndexBuild {
    profile: SemanticProfile,
    handle: tokio::task::JoinHandle<SemanticIndex>,
}

#[cfg(feature = "local")]
struct IndexJobInner {
    record: SemanticProfileRecord,
    message_ids: Vec<MessageId>,
    chunk_source: ChunkSource,
    cursor: usize,
    /// False when the stored embeddings cannot be trusted (model revision
    /// changed), which forces every message to be re-embedded.
    reuse_existing: bool,
}

#[cfg(feature = "local")]
struct ChunkPreparationInput {
    envelope: Envelope,
    body: Option<MessageBody>,
}

#[cfg(feature = "local")]
type TestEmbedder = Arc<dyn Fn(SemanticProfile, &[String]) -> Result<Vec<Vec<f32>>> + Send + Sync>;

pub struct SemanticEngine {
    store: Arc<Store>,
    #[cfg(feature = "local")]
    cache_dir: PathBuf,
    config: SemanticConfig,
    runtime_metrics: SemanticRuntimeMetrics,
    #[cfg(feature = "local")]
    cpu_executor: CpuExecutor,
    /// Shared so embedding runs can move off the async runtime onto a blocking
    /// thread; `TextEmbedding::embed` needs `&mut self`, hence the mutex.
    #[cfg(feature = "local")]
    models: HashMap<SemanticProfile, Arc<Mutex<TextEmbedding>>>,
    #[cfg(feature = "local")]
    indexes: HashMap<SemanticProfile, SemanticIndex>,
    /// Profiles whose ANN index no longer matches the stored embeddings.
    /// Rebuilding is deferred so a burst of ingests costs one rebuild, not one
    /// per message.
    #[cfg(feature = "local")]
    dirty_indexes: HashSet<SemanticProfile>,
    /// The ANN rebuild currently running on a blocking thread, if any.
    #[cfg(feature = "local")]
    index_build: Option<IndexBuild>,
    #[cfg(feature = "local")]
    test_embedder: Option<TestEmbedder>,
}

#[cfg(feature = "local")]
impl SemanticEngine {
    pub fn new(store: Arc<Store>, data_dir: &Path, config: SemanticConfig) -> Self {
        Self {
            store,
            cache_dir: data_dir.join("models"),
            config,
            runtime_metrics: SemanticRuntimeMetrics::default(),
            cpu_executor: CpuExecutor::new(),
            models: HashMap::new(),
            indexes: HashMap::new(),
            dirty_indexes: HashSet::new(),
            index_build: None,
            test_embedder: None,
        }
    }

    pub fn apply_config(&mut self, config: SemanticConfig) {
        self.config = config;
    }

    #[doc(hidden)]
    pub fn set_test_embedder(&mut self, embedder: TestEmbedder) {
        self.test_embedder = Some(embedder);
    }

    #[cfg(all(test, feature = "local"))]
    #[doc(hidden)]
    fn set_test_cpu_observer(&mut self, observer: CpuObserver) {
        self.cpu_executor.set_observer(observer);
    }

    pub async fn status_snapshot(&self) -> Result<SemanticStatusSnapshot> {
        Ok(SemanticStatusSnapshot {
            enabled: self.config.enabled,
            active_profile: self.config.active_profile,
            profiles: self.store.list_semantic_profiles().await?,
            runtime: self.runtime_metrics.clone(),
        })
    }

    pub async fn install_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let stored = self.store.get_semantic_profile(profile).await?;
        // The warmup embed exists to load the model and learn its dimensions.
        // Once the model is loaded in this process the stored dimensions are
        // authoritative, and repeating it would cost a full inference on every
        // ingest.
        let known_dimensions = stored.as_ref().and_then(|record| {
            (record.dimensions > 0
                && record.model_revision == FASTEMBED_REVISION
                && self.test_embedder.is_none()
                && self.models.contains_key(&profile))
            .then_some(record.dimensions)
        });
        let dimensions = match known_dimensions {
            Some(dimensions) => dimensions,
            None => {
                let warmup = vec![prefixed_document(profile, "warmup document")];
                let embeddings = self
                    .embed_texts(profile, warmup, Some(1), true, "embed warmup document")
                    .await?;
                embeddings
                    .first()
                    .map(|embedding| embedding.len() as u32)
                    .ok_or_else(|| anyhow!("embedding backend returned no vector"))?
            }
        };

        let mut record = stored.unwrap_or_else(|| default_profile_record(profile, dimensions));
        record.dimensions = dimensions;
        record.status = SemanticProfileStatus::Ready;
        if record.installed_at.is_none() {
            record.installed_at = Some(chrono::Utc::now());
        }
        record.last_error = None;
        self.store.upsert_semantic_profile(&record).await?;
        Ok(record)
    }

    pub async fn use_profile(&mut self, profile: SemanticProfile) -> Result<SemanticProfileRecord> {
        let mut record = self.activate_profile(profile).await?;
        record.activated_at = Some(chrono::Utc::now());
        self.store.upsert_semantic_profile(&record).await?;
        Ok(record)
    }

    pub async fn reindex_active(&mut self) -> Result<SemanticProfileRecord> {
        self.reindex_all_for_profile(self.config.active_profile)
            .await
    }

    pub async fn backfill_active(&mut self) -> Result<SemanticProfileRecord> {
        self.backfill_missing_for_profile(self.config.active_profile)
            .await
    }

    pub async fn backfill_active_limited(&mut self, limit: u32) -> Result<SemanticProfileRecord> {
        self.backfill_missing_for_profile_limited(self.config.active_profile, limit)
            .await
    }

    /// Sync-time semantic ingest path.
    ///
    /// This always prepares and persists chunks for the changed messages so later
    /// semantic enablement can reuse stored normalized text. Embedding generation
    /// and ANN refresh stay feature-gated behind `config.enabled`.
    pub async fn ingest_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        let ingest_started = Instant::now();
        if message_ids.is_empty() {
            self.runtime_metrics.last_ingest_ms = Some(ingest_started.elapsed().as_millis() as u64);
            return Ok(());
        }

        let now = chrono::Utc::now();
        let batches = self.prepare_message_chunks(message_ids, now).await?;
        if !self.config.enabled {
            self.runtime_metrics.last_ingest_ms = Some(ingest_started.elapsed().as_millis() as u64);
            return Ok(());
        }

        let profile = self.config.active_profile;
        let record = self.install_profile(profile).await?;
        let stale = self
            .batches_needing_embeddings(&record, &batches, true)
            .await?;
        if !stale.is_empty() {
            self.embed_and_store(&record, &stale, now).await?;
            // Deferred: rebuilding per ingest makes a backlog drain quadratic.
            self.dirty_indexes.insert(profile);
        }

        let mut ready_record = record;
        ready_record.status = SemanticProfileStatus::Ready;
        ready_record.last_indexed_at = Some(chrono::Utc::now());
        ready_record.last_error = None;
        self.store.upsert_semantic_profile(&ready_record).await?;
        self.runtime_metrics.last_ingest_ms = Some(ingest_started.elapsed().as_millis() as u64);
        Ok(())
    }

    /// Starts a deferred ANN rebuild if one is pending and none is running.
    ///
    /// Returns as soon as the build is spawned; the worker loop learns it
    /// finished through [`Self::index_build_ready`].
    pub(crate) async fn poll_index_builds(&mut self) -> Result<()> {
        self.start_next_index_build().await?;
        Ok(())
    }

    pub async fn reindex_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        self.ingest_messages(message_ids).await
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: usize,
        allowed_source_kinds: &[SemanticChunkSourceKind],
    ) -> Result<Vec<SemanticHit>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let profile = self.config.active_profile;
        self.install_profile(profile).await?;
        if !self.indexes.contains_key(&profile) {
            // Nothing to answer from yet, so this first query has to wait for
            // the build. Once an index exists a stale one keeps serving while
            // the rebuild runs in the background.
            self.dirty_indexes.insert(profile);
            self.settle_index_builds().await?;
        }

        let query_texts = vec![prefixed_query(profile, query)];
        let query_embedding = self
            .embed_texts(
                profile,
                query_texts,
                Some(1),
                self.config.auto_download_models,
                "embed query",
            )
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("embedding backend returned no query vector"))?;

        let Some(index) = self.indexes.get(&profile) else {
            return Ok(Vec::new());
        };
        if index.chunks_by_id.is_empty() {
            return Ok(Vec::new());
        }

        let candidate_limit = limit.max(1);
        let ef = candidate_limit.max(64);
        let neighbours = index.hnsw.search(&query_embedding, candidate_limit, ef);
        Ok(best_hits_for_neighbours(
            index,
            neighbours
                .into_iter()
                .map(|neighbour| (neighbour.d_id, 1.0 - neighbour.distance)),
            allowed_source_kinds,
            limit,
        ))
    }

    /// Starts a reindex of every stored message for the active profile.
    ///
    /// The caller drives it with [`Self::index_job_step`] so long passes stay
    /// interruptible; [`Self::reindex_active`] is the run-to-completion form.
    pub(crate) async fn begin_reindex(&mut self) -> Result<SemanticIndexJob> {
        let profile = self.config.active_profile;
        let message_ids = self.all_message_ids().await?;
        self.begin_index_job(profile, message_ids, ChunkSource::Message)
            .await
    }

    /// Records a failed pass so status stops reporting `indexing` forever.
    pub(crate) async fn fail_index_job(&mut self, job: SemanticIndexJob, error: &anyhow::Error) {
        let mut record = job.inner.record;
        record.status = SemanticProfileStatus::Error;
        // `{:#}` keeps the anyhow cause chain; `last_error` is what the user
        // sees in `mxr semantic status`.
        record.last_error = Some(format!("{error:#}"));
        if let Err(store_error) = self.store.upsert_semantic_profile(&record).await {
            tracing::warn!("failed to record semantic index failure: {store_error}");
        }
    }

    async fn begin_index_job(
        &mut self,
        profile: SemanticProfile,
        message_ids: Vec<MessageId>,
        chunk_source: ChunkSource,
    ) -> Result<SemanticIndexJob> {
        let stored = self.store.get_semantic_profile(profile).await?;
        // A model revision change silently invalidates every stored vector for
        // the profile, so nothing may be treated as already current.
        let reuse_existing =
            stored.is_none_or(|stored| stored.model_revision == FASTEMBED_REVISION);

        let mut record = self.install_profile(profile).await?;
        record.model_revision = FASTEMBED_REVISION.to_string();
        record.status = SemanticProfileStatus::Indexing;
        record.progress_completed = 0;
        record.progress_total = message_ids.len() as u32;
        record.last_error = None;
        self.store.upsert_semantic_profile(&record).await?;

        Ok(SemanticIndexJob {
            inner: IndexJobInner {
                record,
                message_ids,
                chunk_source,
                cursor: 0,
                reuse_existing,
            },
        })
    }

    /// Processes one page. Returns `true` while work remains.
    pub(crate) async fn index_job_step(&mut self, job: &mut SemanticIndexJob) -> Result<bool> {
        let job = &mut job.inner;
        let end = (job.cursor + INDEX_PAGE_MESSAGES).min(job.message_ids.len());
        let page = job.message_ids[job.cursor..end].to_vec();
        let now = chrono::Utc::now();

        let batches = match job.chunk_source {
            ChunkSource::Message => self.prepare_message_chunks(&page, now).await?,
            ChunkSource::Stored => self.load_or_prepare_chunks(&page, now).await?,
        };
        let stale = self
            .batches_needing_embeddings(&job.record, &batches, job.reuse_existing)
            .await?;
        self.embed_and_store(&job.record, &stale, now).await?;

        job.cursor = end;
        job.record.progress_completed = job.cursor as u32;
        // One row upsert per page is what makes `GetSemanticStatus` show live
        // progress instead of 0/N until the pass ends.
        self.store.upsert_semantic_profile(&job.record).await?;
        Ok(job.cursor < job.message_ids.len())
    }

    pub(crate) async fn finish_index_job(
        &mut self,
        job: SemanticIndexJob,
    ) -> Result<SemanticProfileRecord> {
        let mut job = job.inner;
        let profile = job.record.profile;
        job.record.status = SemanticProfileStatus::Ready;
        job.record.last_indexed_at = Some(chrono::Utc::now());
        job.record.last_error = None;
        if job.record.activated_at.is_none() && self.config.active_profile == profile {
            job.record.activated_at = Some(chrono::Utc::now());
        }
        self.store.upsert_semantic_profile(&job.record).await?;
        self.dirty_indexes.insert(profile);
        Ok(job.record)
    }

    async fn run_index_job(&mut self, mut job: SemanticIndexJob) -> Result<SemanticProfileRecord> {
        while self.index_job_step(&mut job).await? {}
        let record = self.finish_index_job(job).await?;
        self.settle_index_builds().await?;
        Ok(record)
    }

    async fn reindex_all_for_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let message_ids = self.all_message_ids().await?;
        let job = self
            .begin_index_job(profile, message_ids, ChunkSource::Message)
            .await?;
        self.run_index_job(job).await
    }

    async fn activate_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let message_ids = self.all_message_ids().await?;
        let job = self
            .begin_index_job(profile, message_ids, ChunkSource::Stored)
            .await?;
        self.run_index_job(job).await
    }

    async fn backfill_missing_for_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        self.backfill_missing_for_profile_limited(profile, 10_000)
            .await
    }

    async fn backfill_missing_for_profile_limited(
        &mut self,
        profile: SemanticProfile,
        limit: u32,
    ) -> Result<SemanticProfileRecord> {
        let profile_id = self.install_profile(profile).await?.id;
        let limit = limit.max(1);
        let missing_chunks = self
            .store
            .list_message_ids_missing_semantic_chunks(limit)
            .await?;
        let mut targets = self
            .store
            .list_message_ids_missing_semantic_embeddings(&profile_id, limit)
            .await?;
        for message_id in missing_chunks {
            if !targets.contains(&message_id) {
                targets.push(message_id);
            }
        }

        let job = self
            .begin_index_job(profile, targets, ChunkSource::Stored)
            .await?;
        let record = self.run_index_job(job).await?;
        self.store
            .insert_event(
                "info",
                "semantic",
                "Semantic backfill completed",
                None,
                Some(&format!(
                    "profile={} messages_backfilled={}",
                    profile.as_str(),
                    record.progress_completed
                )),
            )
            .await?;
        Ok(record)
    }

    /// Starts the ANN rebuild for one dirty profile, if nothing is building.
    ///
    /// Returns `true` when a build was started. Building 100k+ vectors takes
    /// tens of seconds, so it runs on a blocking thread and the current index
    /// keeps serving searches until the new one is swapped in.
    async fn start_next_index_build(&mut self) -> Result<bool> {
        if self.index_build.is_some() {
            return Ok(false);
        }
        let Some(profile) = self.dirty_indexes.iter().copied().next() else {
            return Ok(false);
        };
        self.dirty_indexes.remove(&profile);

        let record = self
            .store
            .get_semantic_profile(profile)
            .await?
            .ok_or_else(|| anyhow!("semantic profile {} not installed", profile.as_str()))?;
        let rows = self.store.list_semantic_embeddings(&record.id).await?;
        let handle = tokio::task::spawn_blocking(move || build_semantic_index(rows));
        self.index_build = Some(IndexBuild { profile, handle });
        Ok(true)
    }

    /// Resolves once the in-flight ANN build finishes and has been swapped in.
    ///
    /// Never resolves when nothing is building, so it can sit in a `select!`
    /// arm; polling it without completing it leaves the build untouched.
    pub(crate) async fn index_build_ready(&mut self) {
        let Some(build) = self.index_build.as_mut() else {
            return std::future::pending::<()>().await;
        };
        let profile = build.profile;
        let result = (&mut build.handle).await;
        self.index_build = None;
        match result {
            Ok(index) => {
                self.indexes.insert(profile, index);
            }
            Err(error) => {
                tracing::error!(
                    profile = profile.as_str(),
                    "semantic index build task failed: {error}"
                );
                // Leave it dirty so the next idle pass retries.
                self.dirty_indexes.insert(profile);
            }
        }
    }

    /// Builds every dirty index and waits for it. Used by the run-to-completion
    /// paths (`use_profile`, `backfill`, `reindex_active`), which must leave a
    /// searchable index behind; the worker loop uses the polled form instead.
    async fn settle_index_builds(&mut self) -> Result<()> {
        while self.start_next_index_build().await? || self.index_build.is_some() {
            self.index_build_ready().await;
        }
        Ok(())
    }

    /// Every stored message id, paged on the primary key.
    ///
    /// This used to hydrate envelopes per account with a hard 10k limit, so a
    /// reindex of a larger mailbox silently covered only the first 10k
    /// messages per account and reported a truncated `progress_total`.
    async fn all_message_ids(&self) -> Result<Vec<MessageId>> {
        let mut message_ids: Vec<MessageId> = Vec::new();
        loop {
            let page = self
                .store
                .list_message_ids_after(message_ids.last(), MESSAGE_ID_PAGE)
                .await?;
            let exhausted = page.len() < MESSAGE_ID_PAGE as usize;
            message_ids.extend(page);
            if exhausted {
                return Ok(message_ids);
            }
        }
    }

    /// Extracts chunks from the messages themselves, rewriting stored chunks
    /// only where the extracted text drifted. Skipping the rewrite matters:
    /// replacing chunk rows cascades the message's embeddings away.
    async fn prepare_message_chunks(
        &mut self,
        message_ids: &[MessageId],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MessageChunkBatch>> {
        let extract_started = Instant::now();
        let mut inputs = Vec::with_capacity(message_ids.len());
        for message_id in message_ids {
            if let Some(input) = self.load_chunk_preparation_input(message_id).await? {
                inputs.push(input);
            }
        }

        let mut batches = self
            .cpu_executor
            .map(inputs, move |input| {
                Ok(build_message_chunk_batch(input, now))
            })
            .await?;
        for batch in &mut batches {
            let stored = self
                .store
                .list_semantic_chunk_fingerprints(&batch.message_id)
                .await?;
            batch.chunks_changed = !chunks_match(&stored, &batch.chunks);
            if batch.chunks_changed {
                self.store
                    .replace_semantic_chunks(&batch.message_id, &batch.chunks)
                    .await?;
            }
        }
        self.runtime_metrics.last_extract_ms = Some(extract_started.elapsed().as_millis() as u64);
        Ok(batches)
    }

    /// Reuses stored chunks and only extracts for messages that have none.
    /// Chunks are profile-independent, so switching or backfilling a profile
    /// never needs extraction redone.
    async fn load_or_prepare_chunks(
        &mut self,
        message_ids: &[MessageId],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MessageChunkBatch>> {
        let mut batches = Vec::with_capacity(message_ids.len());
        let mut without_chunks = Vec::new();
        for message_id in message_ids {
            let chunks = self.store.list_semantic_chunks(message_id).await?;
            if chunks.is_empty() {
                without_chunks.push(message_id.clone());
            } else {
                batches.push(MessageChunkBatch {
                    message_id: message_id.clone(),
                    chunks,
                    chunks_changed: false,
                });
            }
        }
        batches.extend(self.prepare_message_chunks(&without_chunks, now).await?);
        Ok(batches)
    }

    /// Filters out messages whose stored embeddings are already current for
    /// the profile, so a repeated reindex or a re-enqueued ingest costs a
    /// lookup instead of an inference.
    async fn batches_needing_embeddings<'a>(
        &self,
        record: &SemanticProfileRecord,
        batches: &'a [MessageChunkBatch],
        reuse_existing: bool,
    ) -> Result<Vec<&'a MessageChunkBatch>> {
        let mut stale = Vec::new();
        for batch in batches {
            if batch.chunks.is_empty() {
                continue;
            }
            if !reuse_existing || batch.chunks_changed {
                stale.push(batch);
                continue;
            }
            let missing = self
                .store
                .count_semantic_chunks_missing_embeddings(
                    &batch.message_id,
                    &record.id,
                    record.dimensions,
                )
                .await?;
            if missing > 0 {
                stale.push(batch);
            }
        }
        Ok(stale)
    }

    /// Embeds and persists chunks for many messages, filling each embedding
    /// call with chunks spanning as many messages as fit.
    async fn embed_and_store(
        &mut self,
        record: &SemanticProfileRecord,
        batches: &[&MessageChunkBatch],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let mut start = 0;
        while start < batches.len() {
            // Take whole messages until the next one would overflow the call,
            // so a message's chunks never straddle two embedding calls.
            let mut end = start;
            let mut texts = 0;
            while end < batches.len()
                && (end == start || texts + batches[end].chunks.len() <= EMBED_TEXTS_PER_CALL)
            {
                texts += batches[end].chunks.len();
                end += 1;
            }
            self.embed_group(record, &batches[start..end], now).await?;
            start = end;
        }
        Ok(())
    }

    async fn embed_group(
        &mut self,
        record: &SemanticProfileRecord,
        group: &[&MessageChunkBatch],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let texts = group
            .iter()
            .flat_map(|batch| {
                batch
                    .chunks
                    .iter()
                    .map(|chunk| prefixed_document(record.profile, &chunk.normalized))
            })
            .collect::<Vec<_>>();
        if texts.is_empty() {
            return Ok(());
        }

        let expected = texts.len();
        // The tokenizer pads every row of an ONNX batch to the longest row in
        // it, so mixing short header chunks with full-window body chunks pays
        // for padding. Grouping similar lengths together keeps the batches
        // tight; the original order is restored below.
        let mut indexed = texts.into_iter().enumerate().collect::<Vec<_>>();
        indexed.sort_unstable_by_key(|(_, text)| std::cmp::Reverse(text.len()));
        let (order, sorted): (Vec<usize>, Vec<String>) = indexed.into_iter().unzip();

        let embeddings = self
            .embed_texts(
                record.profile,
                sorted,
                Some(EMBED_MODEL_BATCH),
                self.config.auto_download_models,
                "embed message chunks",
            )
            .await?;
        if embeddings.len() != expected {
            return Err(anyhow!(
                "embedding backend returned {} vectors for {expected} chunks",
                embeddings.len()
            ));
        }
        let mut restored = vec![Vec::new(); expected];
        for (slot, embedding) in order.into_iter().zip(embeddings) {
            restored[slot] = embedding;
        }

        let mut embeddings = restored.into_iter();
        let mut prep_elapsed = Duration::ZERO;
        for batch in group {
            let prep_started = Instant::now();
            let records = batch
                .chunks
                .iter()
                .zip(embeddings.by_ref())
                .map(|(chunk, embedding)| SemanticEmbeddingRecord {
                    chunk_id: chunk.id.clone(),
                    profile_id: record.id.clone(),
                    dimensions: embedding.len() as u32,
                    vector: f32s_to_blob(&embedding),
                    status: SemanticEmbeddingStatus::Ready,
                    created_at: now,
                    updated_at: now,
                })
                .collect::<Vec<_>>();
            prep_elapsed += prep_started.elapsed();
            self.store
                .replace_semantic_embeddings(&batch.message_id, &record.id, &records)
                .await?;
        }
        self.runtime_metrics.last_embedding_prep_ms = Some(prep_elapsed.as_millis() as u64);
        Ok(())
    }

    async fn load_chunk_preparation_input(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<ChunkPreparationInput>> {
        let Some(envelope) = self.store.get_envelope(message_id).await? else {
            return Ok(None);
        };
        let body = self.store.get_body(message_id).await?;
        Ok(Some(ChunkPreparationInput { envelope, body }))
    }

    async fn embed_texts(
        &mut self,
        profile: SemanticProfile,
        texts: Vec<String>,
        batch_size: Option<usize>,
        allow_download: bool,
        context_label: &'static str,
    ) -> Result<Vec<Vec<f32>>> {
        if let Some(embedder) = &self.test_embedder {
            return embedder(profile, &texts);
        }

        let model = self.ensure_model(profile, allow_download)?;
        // ONNX inference is CPU-bound for the length of a whole batch; running
        // it inline would park an async runtime worker for seconds at a time.
        tokio::task::spawn_blocking(move || {
            let mut model = model
                .lock()
                .map_err(|_| anyhow!("semantic embedding model lock poisoned"))?;
            model.embed(texts, batch_size).context(context_label)
        })
        .await?
    }

    fn ensure_model(
        &mut self,
        profile: SemanticProfile,
        allow_download: bool,
    ) -> Result<Arc<Mutex<TextEmbedding>>> {
        if let Some(model) = self.models.get(&profile) {
            return Ok(model.clone());
        }
        if !allow_download {
            return Err(anyhow!(
                "semantic profile {} is not installed locally",
                profile.as_str()
            ));
        }

        std::fs::create_dir_all(&self.cache_dir)?;
        let model = TextEmbedding::try_new(
            TextInitOptions::new(embedding_model(profile))
                .with_cache_dir(self.cache_dir.clone())
                .with_show_download_progress(false),
        )
        .with_context(|| format!("load semantic profile {}", profile.as_str()))?;
        let model = Arc::new(Mutex::new(model));
        self.models.insert(profile, model.clone());
        Ok(model)
    }
}

#[cfg(not(feature = "local"))]
impl SemanticEngine {
    pub fn new(store: Arc<Store>, data_dir: &Path, config: SemanticConfig) -> Self {
        let _ = data_dir;
        Self {
            store,
            config,
            runtime_metrics: SemanticRuntimeMetrics::default(),
        }
    }

    pub fn apply_config(&mut self, config: SemanticConfig) {
        self.config = config;
    }

    pub async fn status_snapshot(&self) -> Result<SemanticStatusSnapshot> {
        Ok(SemanticStatusSnapshot {
            enabled: false,
            active_profile: self.config.active_profile,
            profiles: self.store.list_semantic_profiles().await?,
            runtime: self.runtime_metrics.clone(),
        })
    }

    pub async fn install_profile(
        &mut self,
        _profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub async fn use_profile(
        &mut self,
        _profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub async fn reindex_active(&mut self) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub async fn backfill_active(&mut self) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub async fn backfill_active_limited(&mut self, _limit: u32) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub async fn ingest_messages(&mut self, _message_ids: &[MessageId]) -> Result<()> {
        Ok(())
    }

    pub async fn reindex_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        self.ingest_messages(message_ids).await
    }

    pub(crate) async fn poll_index_builds(&mut self) -> Result<()> {
        Ok(())
    }

    pub(crate) async fn index_build_ready(&mut self) {
        std::future::pending::<()>().await
    }

    pub(crate) async fn begin_reindex(&mut self) -> Result<SemanticIndexJob> {
        Err(semantic_unavailable_error())
    }

    pub(crate) async fn index_job_step(&mut self, _job: &mut SemanticIndexJob) -> Result<bool> {
        Err(semantic_unavailable_error())
    }

    pub(crate) async fn finish_index_job(
        &mut self,
        _job: SemanticIndexJob,
    ) -> Result<SemanticProfileRecord> {
        Err(semantic_unavailable_error())
    }

    pub(crate) async fn fail_index_job(&mut self, _job: SemanticIndexJob, _error: &anyhow::Error) {}

    pub async fn search(
        &mut self,
        _query: &str,
        _limit: usize,
        _allowed_source_kinds: &[SemanticChunkSourceKind],
    ) -> Result<Vec<SemanticHit>> {
        Ok(Vec::new())
    }
}

#[cfg(feature = "local")]
pub fn should_use_semantic(mode: SearchMode) -> bool {
    matches!(mode, SearchMode::Hybrid | SearchMode::Semantic)
}

#[cfg(not(feature = "local"))]
pub fn should_use_semantic(_mode: SearchMode) -> bool {
    false
}

#[cfg(feature = "local")]
fn default_profile_record(profile: SemanticProfile, dimensions: u32) -> SemanticProfileRecord {
    SemanticProfileRecord {
        id: semantic_profile_id(profile),
        profile,
        backend: "fastembed".to_string(),
        model_revision: FASTEMBED_REVISION.to_string(),
        dimensions,
        status: SemanticProfileStatus::Pending,
        installed_at: None,
        activated_at: None,
        last_indexed_at: None,
        progress_completed: 0,
        progress_total: 0,
        last_error: None,
    }
}

#[cfg(not(feature = "local"))]
fn semantic_unavailable_error() -> anyhow::Error {
    anyhow!("semantic search unavailable in this binary")
}

#[cfg(all(test, not(feature = "local")))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn default_build_reports_semantic_disabled() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let data_dir = tempfile::tempdir().unwrap();
        let engine = SemanticEngine::new(store, data_dir.path(), SemanticConfig::default());

        let snapshot = engine.status_snapshot().await.unwrap();

        assert!(!snapshot.enabled);
        assert!(snapshot.profiles.is_empty());
        assert!(!should_use_semantic(SearchMode::Hybrid));
        assert!(!should_use_semantic(SearchMode::Semantic));
    }

    #[tokio::test]
    async fn default_build_keeps_ingest_and_search_as_noops() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let data_dir = tempfile::tempdir().unwrap();
        let mut engine = SemanticEngine::new(store, data_dir.path(), SemanticConfig::default());

        let message_id = MessageId::new();
        engine
            .ingest_messages(std::slice::from_ref(&message_id))
            .await
            .unwrap();
        let hits = engine
            .search("anything", 10, &[SemanticChunkSourceKind::Body])
            .await
            .unwrap();

        assert!(hits.is_empty());
    }
}

#[cfg(feature = "local")]
fn semantic_profile_id(profile: SemanticProfile) -> SemanticProfileId {
    SemanticProfileId::from_provider_id("semantic_profile", profile.as_str())
}

#[cfg(feature = "local")]
fn semantic_chunk_id(
    message_id: &str,
    source_kind: &SemanticChunkSourceKind,
    ordinal: u32,
) -> mxr_core::SemanticChunkId {
    mxr_core::SemanticChunkId::from_provider_id(
        "semantic_chunk",
        &format!("{message_id}:{source_kind:?}:{ordinal}"),
    )
}

#[cfg(feature = "local")]
fn build_chunk_records(
    envelope: &Envelope,
    body: Option<&MessageBody>,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<SemanticChunkRecord> {
    let chunks = build_chunks(envelope, body);
    let mut chunk_records = Vec::with_capacity(chunks.len());

    for (index, (source_kind, normalized)) in chunks.into_iter().enumerate() {
        let chunk_id = semantic_chunk_id(&envelope.id.as_str(), &source_kind, index as u32);
        let chunk_record = SemanticChunkRecord {
            id: chunk_id,
            message_id: envelope.id.clone(),
            source_kind,
            ordinal: index as u32,
            normalized: normalized.clone(),
            content_hash: content_hash(&normalized),
            created_at: now,
            updated_at: now,
        };
        chunk_records.push(chunk_record);
    }

    chunk_records
}

#[cfg(feature = "local")]
fn build_message_chunk_batch(
    input: ChunkPreparationInput,
    now: chrono::DateTime<chrono::Utc>,
) -> MessageChunkBatch {
    let started_at = Instant::now();
    let message_id = input.envelope.id.clone();
    let chunks = build_chunk_records(&input.envelope, input.body.as_ref(), now);
    tracing::trace!(
        %message_id,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "semantic chunk extraction complete"
    );
    MessageChunkBatch {
        message_id,
        chunks,
        chunks_changed: false,
    }
}

#[cfg(feature = "local")]
fn build_semantic_index(
    rows: Vec<(SemanticChunkRecord, SemanticEmbeddingRecord)>,
) -> SemanticIndex {
    let max_elements = rows.len().max(1);
    let mut hnsw = Hnsw::<f32, DistCosine>::new(16, max_elements, 16, 200, DistCosine {});
    let mut chunks_by_id = HashMap::with_capacity(rows.len());

    for (point_id, (chunk, embedding)) in rows.into_iter().enumerate() {
        let vector = blob_to_f32s(&embedding.vector);
        if vector.is_empty() {
            continue;
        }
        hnsw.insert((&vector, point_id));
        chunks_by_id.insert(
            point_id,
            IndexedChunk {
                chunk_id: chunk.id,
                message_id: chunk.message_id,
                source_kind: chunk.source_kind,
                normalized: chunk.normalized,
            },
        );
    }
    hnsw.set_searching_mode(true);
    SemanticIndex { hnsw, chunks_by_id }
}

#[cfg(feature = "local")]
fn chunks_match(stored: &[(SemanticChunkId, String)], built: &[SemanticChunkRecord]) -> bool {
    stored.len() == built.len()
        && stored
            .iter()
            .zip(built)
            .all(|((id, hash), chunk)| *id == chunk.id && *hash == chunk.content_hash)
}

#[cfg(feature = "local")]
fn build_chunks(
    envelope: &Envelope,
    body: Option<&MessageBody>,
) -> Vec<(SemanticChunkSourceKind, String)> {
    let mut chunks = Vec::new();

    let header = normalize_text(&format!(
        "subject {} from {} {} to {} snippet {}",
        envelope.subject,
        envelope.from.name.as_deref().unwrap_or(""),
        envelope.from.email,
        envelope
            .to
            .iter()
            .map(|addr| addr.email.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        envelope.snippet
    ));
    if !header.is_empty() {
        chunks.push((SemanticChunkSourceKind::Header, header));
    }

    if let Some(body) = body {
        let reader_output = clean(
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            &ReaderConfig::default(),
        );
        for chunk in chunk_text(&reader_output.content, 120, 30) {
            chunks.push((SemanticChunkSourceKind::Body, chunk));
        }

        for attachment in &body.attachments {
            let summary =
                normalize_text(&format!("{} {}", attachment.filename, attachment.mime_type));
            if !summary.is_empty() {
                chunks.push((SemanticChunkSourceKind::AttachmentSummary, summary));
            }

            if let Some(text) = read_attachment_text(attachment) {
                for chunk in chunk_text(&text, 120, 30) {
                    chunks.push((SemanticChunkSourceKind::AttachmentText, chunk));
                }
            }
        }
    }

    chunks
}

#[cfg(feature = "local")]
fn read_attachment_text(attachment: &AttachmentMeta) -> Option<String> {
    let path = attachment.local_path.as_ref()?;
    match attachment_kind(attachment, path) {
        AttachmentKind::Text => read_text_attachment(path, false),
        AttachmentKind::Html => read_text_attachment(path, true),
        AttachmentKind::Pdf => read_pdf_attachment(path),
        AttachmentKind::OfficeDocument => read_office_attachment(path),
        AttachmentKind::Spreadsheet => read_spreadsheet_attachment(attachment, path),
        // Active semantic indexing is real-text only. We keep filename/mime
        // summaries for recall, but never OCR image attachments.
        AttachmentKind::Image => None,
        AttachmentKind::Unknown => None,
    }
}

#[cfg(feature = "local")]
fn read_text_attachment(path: &StdPath, is_html: bool) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    if is_html {
        return normalized_nonempty(&clean(None, Some(&content), &ReaderConfig::default()).content);
    }
    normalized_nonempty(&content)
}

#[cfg(feature = "local")]
fn read_office_attachment(path: &StdPath) -> Option<String> {
    let markdown = undoc::to_markdown(path).ok()?;
    normalized_nonempty(&markdown)
}

#[cfg(feature = "local")]
fn read_spreadsheet_attachment(attachment: &AttachmentMeta, path: &StdPath) -> Option<String> {
    let extension = attachment_extension(attachment, path);
    let mime = attachment.mime_type.to_ascii_lowercase();
    let undoc_text = should_try_undoc_spreadsheet(&mime, extension.as_deref())
        .then(|| read_office_attachment(path))
        .flatten();
    let table_text = read_spreadsheet_tables(path);
    combine_extracted_texts([undoc_text, table_text])
}

#[cfg(feature = "local")]
fn read_spreadsheet_tables(path: &StdPath) -> Option<String> {
    let mut workbook = open_workbook_auto(path).ok()?;
    let mut sections = Vec::new();

    for sheet_name in workbook.sheet_names().clone() {
        let Ok(range) = workbook.worksheet_range(&sheet_name) else {
            continue;
        };

        let mut rows = Vec::new();
        for row in range.rows() {
            let cells = row
                .iter()
                .map(ToString::to_string)
                .map(|cell| normalize_text(&cell))
                .filter(|cell| !cell.is_empty())
                .collect::<Vec<_>>();
            if !cells.is_empty() {
                rows.push(cells.join(" | "));
            }
        }

        if !rows.is_empty() {
            sections.push(format!("sheet {sheet_name}\n{}", rows.join("\n")));
        }
    }

    normalized_nonempty(&sections.join("\n\n"))
}

#[cfg(feature = "local")]
fn should_try_undoc_spreadsheet(mime: &str, extension: Option<&str>) -> bool {
    mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        || matches!(extension, Some("xlsx"))
}

#[cfg(feature = "local")]
fn combine_extracted_texts<I>(parts: I) -> Option<String>
where
    I: IntoIterator<Item = Option<String>>,
{
    let mut combined = Vec::new();
    for part in parts.into_iter().flatten() {
        if combined.iter().any(|existing: &String| {
            existing == &part || existing.contains(&part) || part.contains(existing)
        }) {
            continue;
        }
        combined.push(part);
    }

    if combined.is_empty() {
        None
    } else {
        Some(combined.join("\n\n"))
    }
}

#[cfg(feature = "local")]
fn attachment_extension(attachment: &AttachmentMeta, path: &StdPath) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .or_else(|| attachment.filename.rsplit('.').next())
        .map(|ext| ext.trim().to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}

#[cfg(feature = "local")]
fn normalized_nonempty(text: &str) -> Option<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[cfg(feature = "local")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentKind {
    Text,
    Html,
    Pdf,
    OfficeDocument,
    Spreadsheet,
    Image,
    Unknown,
}

#[cfg(feature = "local")]
fn attachment_kind(attachment: &AttachmentMeta, path: &StdPath) -> AttachmentKind {
    let mime = attachment.mime_type.to_ascii_lowercase();
    let extension = attachment_extension(attachment, path);
    let extension = extension.as_deref();

    if mime == "text/html" || matches!(extension, Some("html" | "htm")) {
        return AttachmentKind::Html;
    }

    if mime.starts_with("text/")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/xml"
                | "application/x-yaml"
                | "application/yaml"
                | "application/markdown"
        )
        || matches!(
            extension,
            Some("txt" | "md" | "markdown" | "json" | "xml" | "yaml" | "yml" | "csv" | "tsv")
        )
    {
        return AttachmentKind::Text;
    }

    if mime == "application/pdf" || matches!(extension, Some("pdf")) {
        return AttachmentKind::Pdf;
    }

    if matches!(
        mime.as_str(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
    ) || matches!(extension, Some("docx" | "pptx"))
    {
        return AttachmentKind::OfficeDocument;
    }

    if matches!(
        mime.as_str(),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-excel"
            | "application/vnd.ms-excel.sheet.binary.macroenabled.12"
            | "application/vnd.ms-excel.sheet.macroenabled.12"
            | "application/vnd.oasis.opendocument.spreadsheet"
    ) || matches!(extension, Some("xlsx" | "xlsm" | "xlsb" | "xls" | "ods"))
    {
        return AttachmentKind::Spreadsheet;
    }

    if mime.starts_with("image/")
        || matches!(
            extension,
            Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff")
        )
    {
        return AttachmentKind::Image;
    }

    AttachmentKind::Unknown
}

#[cfg(feature = "local")]
fn read_pdf_attachment(path: &StdPath) -> Option<String> {
    // No OCR fallback here. PDFs contribute semantic text only when a real text
    // extraction path succeeds.
    unpdf::to_markdown(path)
        .ok()
        .and_then(|markdown| normalized_nonempty(&markdown))
}

#[cfg(feature = "local")]
fn embedding_model(profile: SemanticProfile) -> EmbeddingModel {
    match profile {
        SemanticProfile::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
        SemanticProfile::MultilingualE5Small => EmbeddingModel::MultilingualE5Small,
        SemanticProfile::BgeM3 => EmbeddingModel::BGEM3,
    }
}

#[cfg(feature = "local")]
fn prefixed_query(profile: SemanticProfile, text: &str) -> String {
    let normalized = normalize_text(text);
    match profile {
        SemanticProfile::MultilingualE5Small => format!("query: {normalized}"),
        _ => normalized,
    }
}

#[cfg(feature = "local")]
fn prefixed_document(profile: SemanticProfile, text: &str) -> String {
    let normalized = normalize_text(text);
    match profile {
        SemanticProfile::MultilingualE5Small => format!("passage: {normalized}"),
        _ => normalized,
    }
}

#[cfg(feature = "local")]
fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(feature = "local")]
fn chunk_text(text: &str, window_words: usize, overlap_words: usize) -> Vec<String> {
    let normalized = normalize_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    if words.len() <= window_words {
        return vec![normalized];
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    let step = window_words.saturating_sub(overlap_words).max(1);
    while start < words.len() {
        let end = (start + window_words).min(words.len());
        let chunk = words[start..end].join(" ");
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

#[cfg(feature = "local")]
fn content_hash(normalized: &str) -> String {
    let digest = Sha256::digest(normalized.as_bytes());
    base16ct::lower::encode_string(&digest)
}

#[cfg(feature = "local")]
fn f32s_to_blob(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[cfg(feature = "local")]
fn blob_to_f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

#[cfg(feature = "local")]
fn best_hits_for_neighbours<I>(
    index: &SemanticIndex,
    neighbour_scores: I,
    allowed_source_kinds: &[SemanticChunkSourceKind],
    limit: usize,
) -> Vec<SemanticHit>
where
    I: IntoIterator<Item = (usize, f32)>,
{
    let mut best_by_message: HashMap<MessageId, SemanticHit> = HashMap::new();

    for (point_id, similarity) in neighbour_scores {
        let Some(chunk) = index.chunks_by_id.get(&point_id) else {
            continue;
        };
        if !allowed_source_kinds.contains(&chunk.source_kind) {
            continue;
        }
        let hit = SemanticHit {
            message_id: chunk.message_id.clone(),
            score: similarity,
            chunk_id: chunk.chunk_id.clone(),
            source_kind: chunk.source_kind,
            snippet: semantic_hit_snippet(&chunk.normalized),
        };
        best_by_message
            .entry(chunk.message_id.clone())
            .and_modify(|existing| {
                if similarity > existing.score {
                    *existing = hit.clone();
                }
            })
            .or_insert(hit);
    }

    let mut hits = best_by_message.into_values().collect::<Vec<_>>();
    hits.sort_by(|left, right| right.score.total_cmp(&left.score));
    if hits.len() > limit {
        hits.truncate(limit);
    }
    hits
}

#[cfg(feature = "local")]
fn semantic_hit_snippet(normalized: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut snippet = String::new();
    for ch in normalized.chars().take(MAX_CHARS) {
        snippet.push(ch);
    }
    snippet
}

#[cfg(all(test, feature = "local"))]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use mxr_core::id::{AccountId, AttachmentId, MessageId, ThreadId};
    use mxr_core::types::{Address, BackendRef, MessageFlags, MessageMetadata, ProviderKind};
    use mxr_core::Account;
    use mxr_store::Store;
    use std::collections::BTreeSet;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn test_account() -> Account {
        Account {
            id: AccountId::new(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        }
    }

    fn test_envelope(account_id: &AccountId) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: "fake-1".to_string(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<test@example.com>".to_string()),
            in_reply_to: None,
            references: Vec::new(),
            from: Address {
                name: Some("Alice".to_string()),
                email: "alice@example.com".to_string(),
            },
            to: vec![Address {
                name: Some("Bob".to_string()),
                email: "bob@example.com".to_string(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "House of cards".to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: "body mention".to_string(),
            has_attachments: false,
            size_bytes: 128,
            unsubscribe: mxr_core::types::UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: Vec::new(),
            keywords: BTreeSet::new(),
        }
    }

    fn test_body(message_id: &MessageId, attachments: Vec<AttachmentMeta>) -> MessageBody {
        test_body_with_text(
            message_id,
            "The body mentions house of cards for semantic chunk prep.",
            attachments,
        )
    }

    fn test_body_with_text(
        message_id: &MessageId,
        text_plain: &str,
        attachments: Vec<AttachmentMeta>,
    ) -> MessageBody {
        MessageBody {
            message_id: message_id.clone(),
            text_plain: Some(text_plain.into()),
            text_html: None,
            attachments,
            fetched_at: chrono::Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    fn attachment(path: &StdPath, filename: &str, mime_type: &str) -> AttachmentMeta {
        AttachmentMeta {
            id: AttachmentId::new(),
            message_id: MessageId::new(),
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            disposition: mxr_core::types::AttachmentDisposition::Attachment,
            content_id: None,
            content_location: None,
            size_bytes: std::fs::metadata(path).unwrap().len(),
            local_path: Some(path.to_path_buf()),
            provider_id: "att-1".to_string(),
        }
    }

    fn write_zip(path: &StdPath, files: &[(&str, String)]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, contents) in files {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_docx(path: &StdPath, text: &str) {
        write_zip(
            path,
            &[
                (
                    "[Content_Types].xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#
                        .to_string(),
                ),
                (
                    "_rels/.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "word/document.xml",
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>{text}</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#
                    ),
                ),
            ],
        );
    }

    fn write_pptx(path: &StdPath, text: &str) {
        write_zip(
            path,
            &[
                (
                    "[Content_Types].xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>
</Types>"#
                        .to_string(),
                ),
                (
                    "_rels/.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "ppt/presentation.xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:sldIdLst>
    <p:sldId id="256" r:id="rId1"/>
  </p:sldIdLst>
</p:presentation>"#
                        .to_string(),
                ),
                (
                    "ppt/_rels/presentation.xml.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "ppt/slides/slide1.xml",
                    format!(
                        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr>
        <p:cNvPr id="1" name=""/>
        <p:cNvGrpSpPr/>
        <p:nvPr/>
      </p:nvGrpSpPr>
      <p:grpSpPr/>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="2" name="Title 1"/>
          <p:cNvSpPr/>
          <p:nvPr/>
        </p:nvSpPr>
        <p:txBody>
          <a:bodyPr/>
          <a:lstStyle/>
          <a:p><a:r><a:t>{text}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#
                    ),
                ),
            ],
        );
    }

    fn write_xlsx(path: &StdPath) {
        write_zip(
            path,
            &[
                (
                    "[Content_Types].xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>
</Types>"#
                        .to_string(),
                ),
                (
                    "_rels/.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "xl/workbook.xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Summary" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#
                        .to_string(),
                ),
                (
                    "xl/_rels/workbook.xml.rels",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>
</Relationships>"#
                        .to_string(),
                ),
                (
                    "xl/sharedStrings.xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="4" uniqueCount="4">
  <si><t>Name</t></si>
  <si><t>Value</t></si>
  <si><t>Alice</t></si>
  <si><t>42</t></si>
</sst>"#
                        .to_string(),
                ),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="s"><v>0</v></c>
      <c r="B1" t="s"><v>1</v></c>
    </row>
    <row r="2">
      <c r="A2" t="s"><v>2</v></c>
      <c r="B2" t="s"><v>3</v></c>
    </row>
  </sheetData>
</worksheet>"#
                        .to_string(),
                ),
            ],
        );
    }

    fn test_embedder(_profile: SemanticProfile, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let contains = |needle: &str| text.contains(needle) as u8 as f32;
                vec![
                    contains("deployment"),
                    contains("roadmap"),
                    contains("launch"),
                    contains("notes"),
                    1.0,
                ]
            })
            .collect())
    }

    #[test]
    fn attachment_kind_uses_extension_when_mime_is_generic() {
        let dir = tempdir().unwrap();
        let docx_path = dir.path().join("roadmap.docx");
        write_docx(&docx_path, "Quarterly roadmap");
        let attachment = attachment(&docx_path, "roadmap.docx", "application/octet-stream");

        assert_eq!(
            attachment_kind(&attachment, docx_path.as_path()),
            AttachmentKind::OfficeDocument
        );
    }

    #[test]
    fn read_attachment_text_extracts_docx_with_undoc() {
        let dir = tempdir().unwrap();
        let docx_path = dir.path().join("roadmap.docx");
        write_docx(&docx_path, "Quarterly roadmap for launch");
        let attachment = attachment(
            &docx_path,
            "roadmap.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("quarterly roadmap"));
        assert!(extracted.contains("launch"));
    }

    #[test]
    fn read_attachment_text_extracts_pptx_with_undoc() {
        let dir = tempdir().unwrap();
        let pptx_path = dir.path().join("deck.pptx");
        write_pptx(&pptx_path, "Launch metrics");
        let attachment = attachment(
            &pptx_path,
            "deck.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("launch metrics"));
    }

    #[test]
    fn read_attachment_text_extracts_xlsx_with_table_fallback() {
        let dir = tempdir().unwrap();
        let xlsx_path = dir.path().join("table.xlsx");
        write_xlsx(&xlsx_path);
        let attachment = attachment(
            &xlsx_path,
            "table.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        );

        let extracted = read_attachment_text(&attachment).unwrap();
        assert!(extracted.contains("sheet summary"));
        assert!(extracted.contains("name | value"));
        assert!(extracted.contains("alice | 42"));
    }

    #[test]
    fn read_attachment_text_skips_images_without_ocr() {
        let dir = tempdir().unwrap();
        let image_path = dir.path().join("photo.png");
        std::fs::write(&image_path, b"not-a-real-png").unwrap();
        let attachment = attachment(&image_path, "photo.png", "image/png");

        assert_eq!(read_attachment_text(&attachment), None);
    }

    #[test]
    fn read_attachment_text_skips_non_extractable_pdfs_without_ocr() {
        let dir = tempdir().unwrap();
        let pdf_path = dir.path().join("scan.pdf");
        std::fs::write(&pdf_path, b"not-a-real-pdf").unwrap();
        let attachment = attachment(&pdf_path, "scan.pdf", "application/pdf");

        assert_eq!(read_attachment_text(&attachment), None);
    }

    #[tokio::test]
    async fn ingest_messages_persists_chunks_when_semantic_is_disabled() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envelope = test_envelope(&account.id);
        let body = test_body(&envelope.id, Vec::new());
        store.upsert_envelope(&envelope).await.unwrap();
        store.insert_body(&body).await.unwrap();

        let data_dir = tempdir().unwrap();
        let mut engine = SemanticEngine::new(
            store.clone(),
            data_dir.path(),
            SemanticConfig {
                enabled: false,
                ..SemanticConfig::default()
            },
        );
        engine
            .ingest_messages(std::slice::from_ref(&envelope.id))
            .await
            .unwrap();

        let counts = store.collect_record_counts().await.unwrap();
        assert!(counts.semantic_chunks > 0);
        assert_eq!(counts.semantic_embeddings, 0);
        assert!(store.list_semantic_profiles().await.unwrap().is_empty());

        let chunks = store.list_semantic_chunks(&envelope.id).await.unwrap();
        assert!(!chunks.is_empty());
        assert!(chunks
            .iter()
            .any(|chunk| chunk.source_kind == SemanticChunkSourceKind::Header));
        assert!(chunks
            .iter()
            .any(|chunk| chunk.source_kind == SemanticChunkSourceKind::Body));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ingest_messages_uses_cpu_executor_for_multiple_messages() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let message_ids = (0..4)
            .map(|index| {
                let envelope = Envelope {
                    provider_id: format!("fake-{index}"),
                    subject: format!("Message {index}"),
                    snippet: format!("Snippet {index}"),
                    ..test_envelope(&account.id)
                };
                let body =
                    test_body_with_text(&envelope.id, &format!("Body text {index}"), Vec::new());
                (envelope, body)
            })
            .collect::<Vec<_>>();

        for (envelope, body) in &message_ids {
            store.upsert_envelope(envelope).await.unwrap();
            store.insert_body(body).await.unwrap();
        }

        let data_dir = tempdir().unwrap();
        let mut engine =
            SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
        let observer = CpuObserver::new(std::time::Duration::from_millis(20));
        engine.set_test_cpu_observer(observer.clone());

        let ids = message_ids
            .iter()
            .map(|(envelope, _)| envelope.id.clone())
            .collect::<Vec<_>>();
        engine.ingest_messages(&ids).await.unwrap();

        assert!(
            observer.max_concurrency() > 1,
            "expected semantic cpu executor overlap, observed {}",
            observer.max_concurrency()
        );
    }

    #[tokio::test]
    async fn status_snapshot_reports_runtime_metrics_after_ingest() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envelope = test_envelope(&account.id);
        let body = test_body(&envelope.id, Vec::new());
        store.upsert_envelope(&envelope).await.unwrap();
        store.insert_body(&body).await.unwrap();

        let data_dir = tempdir().unwrap();
        let mut engine =
            SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
        engine
            .ingest_messages(std::slice::from_ref(&envelope.id))
            .await
            .unwrap();

        let snapshot = engine.status_snapshot().await.unwrap();
        assert!(snapshot.runtime.last_extract_ms.is_some());
        assert!(snapshot.runtime.last_ingest_ms.is_some());
    }

    #[tokio::test]
    async fn use_profile_reuses_stored_chunks_and_backfills_missing_messages() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let existing = test_envelope(&account.id);
        let existing_body = test_body_with_text(&existing.id, "Deployment checklist", Vec::new());
        store.upsert_envelope(&existing).await.unwrap();
        store.insert_body(&existing_body).await.unwrap();

        let missing = Envelope {
            provider_id: "fake-2".into(),
            subject: "Roadmap notes".into(),
            snippet: "Launch plan".into(),
            ..test_envelope(&account.id)
        };
        let missing_body = test_body_with_text(&missing.id, "Launch notes", Vec::new());
        store.upsert_envelope(&missing).await.unwrap();
        store.insert_body(&missing_body).await.unwrap();

        let data_dir = tempdir().unwrap();
        let mut engine =
            SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
        engine
            .ingest_messages(std::slice::from_ref(&existing.id))
            .await
            .unwrap();

        let before_existing = store.list_semantic_chunks(&existing.id).await.unwrap();
        assert!(!before_existing.is_empty());
        assert!(store
            .list_semantic_chunks(&missing.id)
            .await
            .unwrap()
            .is_empty());

        let config = SemanticConfig {
            enabled: true,
            ..SemanticConfig::default()
        };
        engine.apply_config(config);
        engine.set_test_embedder(Arc::new(test_embedder));

        let profile = engine
            .use_profile(SemanticProfile::BgeSmallEnV15)
            .await
            .unwrap();

        assert_eq!(profile.status, SemanticProfileStatus::Ready);
        assert!(profile.activated_at.is_some());

        let after_existing = store.list_semantic_chunks(&existing.id).await.unwrap();
        assert_eq!(after_existing.len(), before_existing.len());
        for (before, after) in before_existing.iter().zip(after_existing.iter()) {
            assert_eq!(before.id, after.id);
            assert_eq!(before.content_hash, after.content_hash);
            assert_eq!(before.created_at, after.created_at);
            assert_eq!(before.updated_at, after.updated_at);
        }

        let missing_chunks = store.list_semantic_chunks(&missing.id).await.unwrap();
        assert!(!missing_chunks.is_empty());

        let embeddings = store.list_semantic_embeddings(&profile.id).await.unwrap();
        assert_eq!(
            embeddings.len(),
            after_existing.len().saturating_add(missing_chunks.len())
        );
    }

    #[tokio::test]
    async fn ingest_messages_generates_embeddings_and_refreshes_search_when_semantic_is_enabled() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envelope = test_envelope(&account.id);
        let body = test_body(&envelope.id, Vec::new());
        store.upsert_envelope(&envelope).await.unwrap();
        store.insert_body(&body).await.unwrap();

        let data_dir = tempdir().unwrap();
        let mut engine = SemanticEngine::new(
            store.clone(),
            data_dir.path(),
            SemanticConfig {
                enabled: true,
                ..SemanticConfig::default()
            },
        );
        engine.set_test_embedder(Arc::new(test_embedder));
        engine
            .ingest_messages(std::slice::from_ref(&envelope.id))
            .await
            .unwrap();

        let profile = store
            .get_semantic_profile(SemanticProfile::BgeSmallEnV15)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(profile.status, SemanticProfileStatus::Ready);
        assert!(profile.last_indexed_at.is_some());

        let chunks = store.list_semantic_chunks(&envelope.id).await.unwrap();
        assert!(!chunks.is_empty());
        let embeddings = store.list_semantic_embeddings(&profile.id).await.unwrap();
        assert_eq!(embeddings.len(), chunks.len());

        let hits = engine
            .search(
                "house of cards",
                10,
                &[
                    SemanticChunkSourceKind::Header,
                    SemanticChunkSourceKind::Body,
                    SemanticChunkSourceKind::AttachmentSummary,
                    SemanticChunkSourceKind::AttachmentText,
                ],
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_id, envelope.id);
    }

    #[test]
    fn build_chunks_keeps_attachment_summary_but_skips_attachment_text_for_non_ocr_inputs() {
        let dir = tempdir().unwrap();
        let image_path = dir.path().join("photo.png");
        let pdf_path = dir.path().join("scan.pdf");
        std::fs::write(&image_path, b"not-a-real-png").unwrap();
        std::fs::write(&pdf_path, b"not-a-real-pdf").unwrap();

        let account = test_account();
        let envelope = test_envelope(&account.id);
        let body = test_body(
            &envelope.id,
            vec![
                attachment(&image_path, "photo.png", "image/png"),
                attachment(&pdf_path, "scan.pdf", "application/pdf"),
            ],
        );

        let chunks = build_chunks(&envelope, Some(&body));

        assert!(chunks.iter().any(|(kind, text)| *kind
            == SemanticChunkSourceKind::AttachmentSummary
            && text.contains("photo.png")));
        assert!(chunks.iter().any(|(kind, text)| *kind
            == SemanticChunkSourceKind::AttachmentSummary
            && text.contains("scan.pdf")));
        assert!(!chunks.iter().any(|(kind, text)| *kind
            == SemanticChunkSourceKind::AttachmentText
            && (text.contains("photo") || text.contains("scan"))));
    }

    #[test]
    fn best_hits_for_neighbours_filters_source_kinds_before_collapsing() {
        let message_a = MessageId::new();
        let message_b = MessageId::new();
        let index = SemanticIndex {
            hnsw: Hnsw::<f32, DistCosine>::new(16, 1, 16, 200, DistCosine {}),
            chunks_by_id: HashMap::from([
                (
                    0,
                    IndexedChunk {
                        chunk_id: SemanticChunkId::new(),
                        message_id: message_a.clone(),
                        source_kind: SemanticChunkSourceKind::Header,
                        normalized: "header chunk".to_string(),
                    },
                ),
                (
                    1,
                    IndexedChunk {
                        chunk_id: SemanticChunkId::new(),
                        message_id: message_a.clone(),
                        source_kind: SemanticChunkSourceKind::Body,
                        normalized: "body chunk for message a".to_string(),
                    },
                ),
                (
                    2,
                    IndexedChunk {
                        chunk_id: SemanticChunkId::new(),
                        message_id: message_b.clone(),
                        source_kind: SemanticChunkSourceKind::Body,
                        normalized: "body chunk for message b".to_string(),
                    },
                ),
            ]),
        };

        let hits = best_hits_for_neighbours(
            &index,
            [(0, 0.95), (1, 0.40), (2, 0.90)],
            &[SemanticChunkSourceKind::Body],
            10,
        );

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].message_id, message_b);
        assert_eq!(hits[1].message_id, message_a);
        assert!(hits[0].score > hits[1].score);
        assert_eq!(hits[0].source_kind, SemanticChunkSourceKind::Body);
        assert!(hits[0].snippet.contains("message b"));
    }

    fn fingerprint_vector(text: &str) -> Vec<f32> {
        let checksum = text.bytes().map(u32::from).sum::<u32>();
        vec![text.len() as f32, checksum as f32, 1.0]
    }

    /// Test embedder that records the size of every call so tests can assert
    /// how chunks were batched, and returns a vector derived from the text so
    /// they can assert each chunk got its own vector.
    fn recording_embedder(calls: Arc<std::sync::Mutex<Vec<usize>>>) -> TestEmbedder {
        Arc::new(move |_profile: SemanticProfile, texts: &[String]| {
            if let Ok(mut calls) = calls.lock() {
                calls.push(texts.len());
            }
            Ok(texts.iter().map(|text| fingerprint_vector(text)).collect())
        })
    }

    async fn seed_messages(store: &Store, account_id: &AccountId, count: usize) -> Vec<MessageId> {
        let mut ids = Vec::with_capacity(count);
        for index in 0..count {
            let envelope = Envelope {
                provider_id: format!("fake-{index}"),
                subject: format!("Subject {index}"),
                snippet: format!("Snippet {index}"),
                ..test_envelope(account_id)
            };
            let body = test_body_with_text(
                &envelope.id,
                &format!("Body text number {index}"),
                Vec::new(),
            );
            store.upsert_envelope(&envelope).await.unwrap();
            store.insert_body(&body).await.unwrap();
            ids.push(envelope.id);
        }
        ids
    }

    /// Envelope-only seed: one header chunk per message and no body, which
    /// keeps a 10k-message fixture cheap enough for a normal test run.
    async fn seed_envelopes_only(store: &Store, account_id: &AccountId, count: usize) {
        for index in 0..count {
            let envelope = Envelope {
                provider_id: format!("bulk-{index}"),
                subject: format!("Bulk {index}"),
                snippet: format!("Snippet {index}"),
                ..test_envelope(account_id)
            };
            store.upsert_envelope(&envelope).await.unwrap();
        }
    }

    async fn engine_with_recording_embedder(
        store: Arc<Store>,
        data_dir: &StdPath,
    ) -> (SemanticEngine, Arc<std::sync::Mutex<Vec<usize>>>) {
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut engine = SemanticEngine::new(store, data_dir, SemanticConfig::default());
        engine.set_test_embedder(recording_embedder(calls.clone()));
        (engine, calls)
    }

    fn embed_call_sizes(calls: &Arc<std::sync::Mutex<Vec<usize>>>) -> Vec<usize> {
        calls.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn reindex_fills_embed_calls_with_chunks_from_many_messages() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        seed_messages(&store, &account.id, 40).await;

        let data_dir = tempdir().unwrap();
        let (mut engine, calls) =
            engine_with_recording_embedder(store.clone(), data_dir.path()).await;
        let record = engine.reindex_active().await.unwrap();

        let sizes = embed_call_sizes(&calls);
        let largest = sizes.iter().copied().max().unwrap_or_default();
        assert!(
            largest > 8,
            "expected embed calls spanning many messages, saw {sizes:?}"
        );
        assert!(
            sizes.iter().all(|size| *size <= EMBED_TEXTS_PER_CALL),
            "embed calls must respect the per-call cap, saw {sizes:?}"
        );

        let stored = store.list_semantic_embeddings(&record.id).await.unwrap();
        assert_eq!(stored.len(), 80);
        for (chunk, embedding) in stored {
            let expected =
                fingerprint_vector(&prefixed_document(record.profile, &chunk.normalized));
            assert_eq!(
                blob_to_f32s(&embedding.vector),
                expected,
                "chunk {} got another chunk's vector",
                chunk.id.as_str()
            );
        }
    }

    #[tokio::test]
    async fn reindex_persists_progress_between_pages() {
        let message_count = INDEX_PAGE_MESSAGES + 6;
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        seed_messages(&store, &account.id, message_count).await;

        let data_dir = tempdir().unwrap();
        let (mut engine, _calls) =
            engine_with_recording_embedder(store.clone(), data_dir.path()).await;

        let mut job = engine.begin_reindex().await.unwrap();
        assert!(engine.index_job_step(&mut job).await.unwrap());

        let mid = store
            .get_semantic_profile(SemanticProfile::BgeSmallEnV15)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mid.status, SemanticProfileStatus::Indexing);
        assert_eq!(mid.progress_total, message_count as u32);
        assert_eq!(mid.progress_completed, INDEX_PAGE_MESSAGES as u32);

        assert!(!engine.index_job_step(&mut job).await.unwrap());
        let record = engine.finish_index_job(job).await.unwrap();
        assert_eq!(record.status, SemanticProfileStatus::Ready);
        assert_eq!(record.progress_completed, message_count as u32);
    }

    #[tokio::test]
    async fn reindex_reuses_current_embeddings_and_re_embeds_changed_messages() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let ids = seed_messages(&store, &account.id, 12).await;

        let data_dir = tempdir().unwrap();
        let (mut engine, calls) =
            engine_with_recording_embedder(store.clone(), data_dir.path()).await;
        let record = engine.reindex_active().await.unwrap();
        let first_pass = store.list_semantic_embeddings(&record.id).await.unwrap();
        calls.lock().unwrap().clear();

        engine.reindex_active().await.unwrap();
        assert!(
            embed_call_sizes(&calls).iter().all(|size| *size <= 1),
            "an unchanged reindex must not re-embed message chunks, saw {:?}",
            embed_call_sizes(&calls)
        );
        let second_pass = store.list_semantic_embeddings(&record.id).await.unwrap();
        assert_eq!(second_pass.len(), first_pass.len());

        let changed = ids[3].clone();
        let body = test_body_with_text(&changed, "Completely different body text", Vec::new());
        store.insert_body(&body).await.unwrap();
        calls.lock().unwrap().clear();

        engine.reindex_active().await.unwrap();
        let embedded_texts = embed_call_sizes(&calls).iter().sum::<usize>();
        assert!(
            (1..=3).contains(&embedded_texts),
            "only the changed message should be re-embedded, saw {embedded_texts} texts"
        );
        let chunks = store.list_semantic_chunks(&changed).await.unwrap();
        assert!(chunks
            .iter()
            .any(|chunk| chunk.normalized.contains("completely different")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn status_stays_responsive_while_a_reindex_runs() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        seed_messages(&store, &account.id, INDEX_PAGE_MESSAGES * 3).await;

        let data_dir = tempdir().unwrap();
        let engine = SemanticEngine::new(store, data_dir.path(), SemanticConfig::default());
        let (handle, worker) =
            SemanticServiceHandle::start(engine, Arc::new(tokio::sync::Semaphore::new(4)));
        handle
            .set_test_embedder(|_profile, texts: &[String]| {
                // Slow enough that the pass spans several status polls.
                std::thread::sleep(std::time::Duration::from_millis(20));
                Ok(texts.iter().map(|text| fingerprint_vector(text)).collect())
            })
            .await
            .unwrap();

        let reindex = tokio::spawn({
            let handle = handle.clone();
            async move { handle.reindex_active().await }
        });

        let mut saw_live_progress = false;
        while !reindex.is_finished() {
            let snapshot =
                tokio::time::timeout(std::time::Duration::from_secs(10), handle.status_snapshot())
                    .await
                    .expect("status query blocked behind the reindex")
                    .unwrap();
            saw_live_progress |= snapshot.profiles.iter().any(|profile| {
                profile.status == SemanticProfileStatus::Indexing && profile.progress_completed > 0
            });
            tokio::task::yield_now().await;
        }

        let record = reindex.await.unwrap().unwrap();
        assert_eq!(record.status, SemanticProfileStatus::Ready);
        assert!(
            saw_live_progress,
            "status never reported progress while the reindex was running"
        );

        handle.request_shutdown().await.unwrap();
        worker.await.unwrap();
    }

    /// The ANN rebuild runs on a blocking thread, so a stale index has to keep
    /// answering searches until the new one is swapped in — otherwise a 100k
    /// vector rebuild would hide search for its whole duration.
    #[tokio::test]
    async fn a_pending_index_rebuild_keeps_serving_the_previous_index() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let first = test_envelope(&account.id);
        let first_body = test_body_with_text(&first.id, "Deployment checklist", Vec::new());
        store.upsert_envelope(&first).await.unwrap();
        store.insert_body(&first_body).await.unwrap();

        let second = Envelope {
            provider_id: "fake-2".into(),
            subject: "Roadmap notes".into(),
            snippet: "Launch plan".into(),
            ..test_envelope(&account.id)
        };
        let second_body = test_body_with_text(&second.id, "Roadmap notes launch", Vec::new());
        store.upsert_envelope(&second).await.unwrap();
        store.insert_body(&second_body).await.unwrap();

        let data_dir = tempdir().unwrap();
        let mut engine =
            SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
        engine.set_test_embedder(Arc::new(test_embedder));

        let kinds = [
            SemanticChunkSourceKind::Header,
            SemanticChunkSourceKind::Body,
        ];
        engine
            .ingest_messages(std::slice::from_ref(&first.id))
            .await
            .unwrap();
        let hits = engine
            .search("deployment checklist", 5, &kinds)
            .await
            .unwrap();
        assert!(hits.iter().any(|hit| hit.message_id == first.id));

        engine
            .ingest_messages(std::slice::from_ref(&second.id))
            .await
            .unwrap();
        let stale = engine
            .search("roadmap notes launch", 5, &kinds)
            .await
            .unwrap();
        assert!(
            stale.iter().all(|hit| hit.message_id != second.id),
            "search should answer from the previous index until the rebuild lands"
        );

        assert!(engine.start_next_index_build().await.unwrap());
        engine.index_build_ready().await;
        let refreshed = engine
            .search("roadmap notes launch", 5, &kinds)
            .await
            .unwrap();
        assert!(refreshed.iter().any(|hit| hit.message_id == second.id));
    }

    /// `all_message_ids` used to hydrate envelopes per account with a hard
    /// 10k limit, so a mailbox past that size was silently only partly
    /// indexed and reported a truncated `progress_total`.
    #[tokio::test]
    async fn reindex_covers_every_message_past_the_old_ten_thousand_cap() {
        let message_count = 10_001;
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        seed_envelopes_only(&store, &account.id, message_count).await;

        let data_dir = tempdir().unwrap();
        let (mut engine, _calls) =
            engine_with_recording_embedder(store.clone(), data_dir.path()).await;
        let record = engine.reindex_active().await.unwrap();

        assert_eq!(record.progress_total, message_count as u32);
        assert_eq!(record.progress_completed, message_count as u32);
        let counts = store.collect_record_counts().await.unwrap();
        assert_eq!(counts.semantic_chunks, message_count as u32);
        assert_eq!(counts.semantic_embeddings, message_count as u32);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn queued_ingest_batches_across_messages_and_skips_already_indexed_ones() {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let ids = seed_messages(&store, &account.id, 40).await;

        let data_dir = tempdir().unwrap();
        let engine = SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
        let (handle, worker) =
            SemanticServiceHandle::start(engine, Arc::new(tokio::sync::Semaphore::new(4)));
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorder = calls.clone();
        handle
            .set_test_embedder(move |_profile, texts: &[String]| {
                if let Ok(mut calls) = recorder.lock() {
                    calls.push(texts.len());
                }
                Ok(texts.iter().map(|text| fingerprint_vector(text)).collect())
            })
            .await
            .unwrap();

        handle.enqueue_ingest_messages(&ids).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while store
                .collect_record_counts()
                .await
                .unwrap()
                .semantic_embeddings
                < 80
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("queued ingest did not finish");

        let sizes = embed_call_sizes(&calls);
        assert!(
            sizes.iter().copied().max().unwrap_or_default() > 8,
            "queued ingest should batch chunks across messages, saw {sizes:?}"
        );

        calls.lock().unwrap().clear();
        handle.ingest_messages(&ids).await.unwrap();
        assert!(
            embed_call_sizes(&calls).iter().all(|size| *size <= 1),
            "re-ingesting unchanged messages must not re-embed, saw {:?}",
            embed_call_sizes(&calls)
        );

        handle.request_shutdown().await.unwrap();
        worker.await.unwrap();
    }
}
