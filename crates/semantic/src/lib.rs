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
use mxr_core::id::MessageId;
#[cfg(feature = "local")]
use mxr_core::id::SemanticProfileId;
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
use std::collections::HashMap;
use std::path::Path;
#[cfg(feature = "local")]
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;

pub use service::SemanticServiceHandle;

#[cfg(feature = "local")]
const FASTEMBED_REVISION: &str = "fastembed-5.13.0";

#[derive(Debug, Clone)]
pub struct SemanticHit {
    pub message_id: MessageId,
    pub score: f32,
}

#[cfg(feature = "local")]
struct IndexedChunk {
    message_id: MessageId,
    source_kind: SemanticChunkSourceKind,
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
    extract_elapsed: std::time::Duration,
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
    #[cfg(feature = "local")]
    models: HashMap<SemanticProfile, TextEmbedding>,
    #[cfg(feature = "local")]
    indexes: HashMap<SemanticProfile, SemanticIndex>,
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
        let dimensions = {
            let warmup = vec![prefixed_document(profile, "warmup document")];
            let embeddings =
                self.embed_texts(profile, &warmup, Some(1), true, "embed warmup document")?;
            embeddings
                .first()
                .map(|embedding| embedding.len() as u32)
                .ok_or_else(|| anyhow!("embedding backend returned no vector"))?
        };

        let mut record = self
            .store
            .get_semantic_profile(profile)
            .await?
            .unwrap_or_else(|| default_profile_record(profile, dimensions));
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

    /// Sync-time semantic ingest path.
    ///
    /// This always prepares and persists chunks for the changed messages so later
    /// semantic enablement can reuse stored normalized text. Embedding generation
    /// and ANN refresh stay feature-gated behind `config.enabled`.
    pub async fn ingest_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        let ingest_started = std::time::Instant::now();
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
        self.reindex_batches_for_profile(&record, &batches, now)
            .await?;

        let mut ready_record = record;
        ready_record.status = SemanticProfileStatus::Ready;
        ready_record.last_indexed_at = Some(chrono::Utc::now());
        ready_record.last_error = None;
        self.store.upsert_semantic_profile(&ready_record).await?;
        self.rebuild_index(profile).await?;
        self.runtime_metrics.last_ingest_ms = Some(ingest_started.elapsed().as_millis() as u64);
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
            self.rebuild_index(profile).await?;
        }

        let query_text = prefixed_query(profile, query);
        let query_texts = vec![query_text];
        let query_embedding = self
            .embed_texts(
                profile,
                &query_texts,
                Some(1),
                self.config.auto_download_models,
                "embed query",
            )?
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

    async fn reindex_all_for_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let mut record = self.install_profile(profile).await?;
        record.status = SemanticProfileStatus::Indexing;
        record.progress_completed = 0;
        record.progress_total = 0;
        record.last_error = None;
        self.store.upsert_semantic_profile(&record).await?;

        let message_ids = self.all_message_ids().await?;
        record.progress_total = message_ids.len() as u32;
        self.store.upsert_semantic_profile(&record).await?;
        let now = chrono::Utc::now();
        let batches = self.prepare_message_chunks(&message_ids, now).await?;
        for batch in &batches {
            let embeddings = self
                .build_embedding_records(&record, &batch.chunks, now)
                .await?;
            self.store
                .replace_semantic_embeddings(&batch.message_id, &record.id, &embeddings)
                .await?;
            record.progress_completed += 1;
        }

        record.status = SemanticProfileStatus::Ready;
        record.last_indexed_at = Some(chrono::Utc::now());
        record.last_error = None;
        if record.activated_at.is_none() && self.config.active_profile == profile {
            record.activated_at = Some(chrono::Utc::now());
        }
        self.store.upsert_semantic_profile(&record).await?;
        self.rebuild_index(profile).await?;
        Ok(record)
    }

    async fn activate_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let mut record = self.install_profile(profile).await?;
        record.status = SemanticProfileStatus::Indexing;
        record.progress_completed = 0;
        record.progress_total = 0;
        record.last_error = None;
        self.store.upsert_semantic_profile(&record).await?;

        let message_ids = self.all_message_ids().await?;
        record.progress_total = message_ids.len() as u32;
        self.store.upsert_semantic_profile(&record).await?;

        let now = chrono::Utc::now();
        self.backfill_missing_chunks(&message_ids, now).await?;

        for message_id in &message_ids {
            let chunks = self.store.list_semantic_chunks(message_id).await?;
            let embeddings = self.build_embedding_records(&record, &chunks, now).await?;
            self.store
                .replace_semantic_embeddings(message_id, &record.id, &embeddings)
                .await?;
            record.progress_completed += 1;
        }

        record.status = SemanticProfileStatus::Ready;
        record.last_indexed_at = Some(chrono::Utc::now());
        record.last_error = None;
        self.store.upsert_semantic_profile(&record).await?;
        self.rebuild_index(profile).await?;
        Ok(record)
    }

    async fn rebuild_index(&mut self, profile: SemanticProfile) -> Result<()> {
        let record = self
            .store
            .get_semantic_profile(profile)
            .await?
            .ok_or_else(|| anyhow!("semantic profile {} not installed", profile.as_str()))?;
        let rows = self.store.list_semantic_embeddings(&record.id).await?;
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
                    message_id: chunk.message_id,
                    source_kind: chunk.source_kind,
                },
            );
        }
        hnsw.set_searching_mode(true);

        self.indexes
            .insert(profile, SemanticIndex { hnsw, chunks_by_id });
        Ok(())
    }

    async fn all_message_ids(&self) -> Result<Vec<MessageId>> {
        let accounts = self.store.list_accounts().await?;
        let mut message_ids = Vec::new();
        for account in accounts {
            message_ids.extend(
                self.store
                    .list_envelopes_by_account(&account.id, 10_000, 0)
                    .await?
                    .into_iter()
                    .map(|envelope| envelope.id),
            );
        }
        Ok(message_ids)
    }

    async fn backfill_missing_chunks(
        &mut self,
        message_ids: &[MessageId],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        for message_id in message_ids {
            if self
                .store
                .list_semantic_chunks(message_id)
                .await?
                .is_empty()
            {
                let _ = self.prepare_message_chunk(message_id, now).await?;
            }
        }
        Ok(())
    }

    async fn prepare_message_chunks(
        &mut self,
        message_ids: &[MessageId],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MessageChunkBatch>> {
        let extract_started = std::time::Instant::now();
        let mut inputs = Vec::with_capacity(message_ids.len());
        for message_id in message_ids {
            if let Some(input) = self.load_chunk_preparation_input(message_id).await? {
                inputs.push(input);
            }
        }

        let batches = self
            .cpu_executor
            .map(inputs, move |input| {
                Ok(build_message_chunk_batch(input, now))
            })
            .await?;
        for batch in &batches {
            tracing::trace!(
                message_id = %batch.message_id,
                elapsed_ms = batch.extract_elapsed.as_secs_f64() * 1000.0,
                "semantic chunk extraction complete"
            );
            self.store
                .replace_semantic_chunks(&batch.message_id, &batch.chunks)
                .await?;
        }
        self.runtime_metrics.last_extract_ms = Some(extract_started.elapsed().as_millis() as u64);
        Ok(batches)
    }

    async fn prepare_message_chunk(
        &mut self,
        message_id: &MessageId,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<MessageChunkBatch>> {
        let extract_started = std::time::Instant::now();
        let Some(input) = self.load_chunk_preparation_input(message_id).await? else {
            return Ok(None);
        };
        let mut batches = self
            .cpu_executor
            .map(vec![input], move |input| {
                Ok(build_message_chunk_batch(input, now))
            })
            .await?;
        let Some(batch) = batches.pop() else {
            return Ok(None);
        };
        tracing::trace!(
            message_id = %batch.message_id,
            elapsed_ms = batch.extract_elapsed.as_secs_f64() * 1000.0,
            "semantic chunk extraction complete"
        );
        self.store
            .replace_semantic_chunks(&batch.message_id, &batch.chunks)
            .await?;
        self.runtime_metrics.last_extract_ms = Some(extract_started.elapsed().as_millis() as u64);
        Ok(Some(batch))
    }

    async fn reindex_batches_for_profile(
        &mut self,
        profile: &SemanticProfileRecord,
        batches: &[MessageChunkBatch],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        for batch in batches {
            let embeddings = self
                .build_embedding_records(profile, &batch.chunks, now)
                .await?;
            self.store
                .replace_semantic_embeddings(&batch.message_id, &profile.id, &embeddings)
                .await?;
        }
        Ok(())
    }

    async fn build_embedding_records(
        &mut self,
        profile: &SemanticProfileRecord,
        chunks: &[SemanticChunkRecord],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<SemanticEmbeddingRecord>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let texts = chunks
            .iter()
            .map(|chunk| prefixed_document(profile.profile, &chunk.normalized))
            .collect::<Vec<_>>();
        let embeddings = self.embed_texts(
            profile.profile,
            &texts,
            Some(32),
            self.config.auto_download_models,
            "embed message chunks",
        )?;
        let profile_id = profile.id.clone();
        let chunk_embeddings = chunks
            .iter()
            .cloned()
            .zip(embeddings.into_iter())
            .collect::<Vec<_>>();
        let embed_prep_started = std::time::Instant::now();
        let embedding_records = self
            .cpu_executor
            .map(chunk_embeddings, move |(chunk, embedding)| {
                Ok(SemanticEmbeddingRecord {
                    chunk_id: chunk.id,
                    profile_id: profile_id.clone(),
                    dimensions: embedding.len() as u32,
                    vector: f32s_to_blob(&embedding),
                    status: SemanticEmbeddingStatus::Ready,
                    created_at: now,
                    updated_at: now,
                })
            })
            .await?;
        self.runtime_metrics.last_embedding_prep_ms =
            Some(embed_prep_started.elapsed().as_millis() as u64);
        Ok(embedding_records)
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

    fn embed_texts(
        &mut self,
        profile: SemanticProfile,
        texts: &[String],
        batch_size: Option<usize>,
        allow_download: bool,
        context_label: &'static str,
    ) -> Result<Vec<Vec<f32>>> {
        if let Some(embedder) = &self.test_embedder {
            return embedder(profile, texts);
        }

        self.ensure_model(profile, allow_download)?
            .embed(texts, batch_size)
            .context(context_label)
    }

    fn ensure_model(
        &mut self,
        profile: SemanticProfile,
        allow_download: bool,
    ) -> Result<&mut TextEmbedding> {
        if !self.models.contains_key(&profile) {
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
            self.models.insert(profile, model);
        }

        self.models
            .get_mut(&profile)
            .ok_or_else(|| anyhow!("semantic profile {} not loaded", profile.as_str()))
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

    pub async fn ingest_messages(&mut self, _message_ids: &[MessageId]) -> Result<()> {
        Ok(())
    }

    pub async fn reindex_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        self.ingest_messages(message_ids).await
    }

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
    let started_at = std::time::Instant::now();
    let message_id = input.envelope.id.clone();
    let chunks = build_chunk_records(&input.envelope, input.body.as_ref(), now);
    MessageChunkBatch {
        message_id,
        chunks,
        extract_elapsed: started_at.elapsed(),
    }
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

    for sheet_name in workbook.sheet_names().to_owned() {
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
    format!("{digest:x}")
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
    let mut best_by_message: HashMap<MessageId, f32> = HashMap::new();

    for (point_id, similarity) in neighbour_scores {
        let Some(chunk) = index.chunks_by_id.get(&point_id) else {
            continue;
        };
        if !allowed_source_kinds.contains(&chunk.source_kind) {
            continue;
        }
        best_by_message
            .entry(chunk.message_id.clone())
            .and_modify(|score| {
                if similarity > *score {
                    *score = similarity;
                }
            })
            .or_insert(similarity);
    }

    let mut hits = best_by_message
        .into_iter()
        .map(|(message_id, score)| SemanticHit { message_id, score })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| right.score.total_cmp(&left.score));
    if hits.len() > limit {
        hits.truncate(limit);
    }
    hits
}

#[cfg(all(test, feature = "local"))]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use mxr_core::id::{AccountId, AttachmentId, MessageId, ThreadId};
    use mxr_core::types::{Address, BackendRef, MessageFlags, MessageMetadata, ProviderKind};
    use mxr_core::Account;
    use mxr_store::Store;
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
            label_provider_ids: Vec::new(),
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
        let mut engine =
            SemanticEngine::new(store.clone(), data_dir.path(), SemanticConfig::default());
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
                        message_id: message_a.clone(),
                        source_kind: SemanticChunkSourceKind::Header,
                    },
                ),
                (
                    1,
                    IndexedChunk {
                        message_id: message_a.clone(),
                        source_kind: SemanticChunkSourceKind::Body,
                    },
                ),
                (
                    2,
                    IndexedChunk {
                        message_id: message_b.clone(),
                        source_kind: SemanticChunkSourceKind::Body,
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
    }
}
