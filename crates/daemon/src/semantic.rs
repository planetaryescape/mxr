use anyhow::{anyhow, Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use hnsw_rs::prelude::{DistCosine, Hnsw};
use mxr_config::SemanticConfig;
use mxr_core::id::{MessageId, SemanticProfileId};
use mxr_core::types::{
    AttachmentMeta, Envelope, MessageBody, SearchMode, SemanticChunkRecord,
    SemanticChunkSourceKind, SemanticEmbeddingRecord, SemanticEmbeddingStatus, SemanticProfile,
    SemanticProfileRecord, SemanticProfileStatus, SemanticStatusSnapshot,
};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::Store;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FASTEMBED_REVISION: &str = "fastembed-5.13.0";
#[derive(Debug, Clone)]
pub struct SemanticHit {
    pub message_id: MessageId,
    pub score: f32,
}

struct IndexedChunk {
    message_id: MessageId,
}

struct SemanticIndex {
    hnsw: Hnsw<'static, f32, DistCosine>,
    chunks_by_id: HashMap<usize, IndexedChunk>,
}

pub struct SemanticEngine {
    store: Arc<Store>,
    cache_dir: PathBuf,
    config: SemanticConfig,
    models: HashMap<SemanticProfile, TextEmbedding>,
    indexes: HashMap<SemanticProfile, SemanticIndex>,
}

impl SemanticEngine {
    pub fn new(store: Arc<Store>, data_dir: &Path, config: SemanticConfig) -> Self {
        Self {
            store,
            cache_dir: data_dir.join("models"),
            config,
            models: HashMap::new(),
            indexes: HashMap::new(),
        }
    }

    pub fn apply_config(&mut self, config: SemanticConfig) {
        self.config = config;
    }

    pub async fn status_snapshot(&self) -> Result<SemanticStatusSnapshot> {
        Ok(SemanticStatusSnapshot {
            enabled: self.config.enabled,
            active_profile: self.config.active_profile,
            profiles: self.store.list_semantic_profiles().await?,
        })
    }

    pub async fn install_profile(
        &mut self,
        profile: SemanticProfile,
    ) -> Result<SemanticProfileRecord> {
        let dimensions = {
            let model = self.ensure_model(profile, true)?;
            let embeddings = model
                .embed([prefixed_document(profile, "warmup document")], Some(1))
                .context("embed warmup document")?;
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
        self.install_profile(profile).await?;
        let mut record = self.reindex_all_for_profile(profile).await?;
        record.activated_at = Some(chrono::Utc::now());
        self.store.upsert_semantic_profile(&record).await?;
        Ok(record)
    }

    pub async fn reindex_active(&mut self) -> Result<SemanticProfileRecord> {
        self.reindex_all_for_profile(self.config.active_profile)
            .await
    }

    pub async fn reindex_messages(&mut self, message_ids: &[MessageId]) -> Result<()> {
        if !self.config.enabled || message_ids.is_empty() {
            return Ok(());
        }

        let profile = self.config.active_profile;
        let record = self.install_profile(profile).await?;
        let now = chrono::Utc::now();

        for message_id in message_ids {
            let Some(envelope) = self.store.get_envelope(message_id).await? else {
                continue;
            };
            let body = self.store.get_body(message_id).await?;
            let (chunks, embeddings) =
                self.build_message_records(&record, &envelope, body.as_ref(), now)?;
            self.store
                .replace_semantic_message_data(&envelope.id, &record.id, &chunks, &embeddings)
                .await?;
        }

        let mut ready_record = record;
        ready_record.status = SemanticProfileStatus::Ready;
        ready_record.last_indexed_at = Some(chrono::Utc::now());
        ready_record.last_error = None;
        self.store.upsert_semantic_profile(&ready_record).await?;
        self.rebuild_index(profile).await?;
        Ok(())
    }

    pub async fn search(&mut self, query: &str, limit: usize) -> Result<Vec<SemanticHit>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let profile = self.config.active_profile;
        self.install_profile(profile).await?;
        if !self.indexes.contains_key(&profile) {
            self.rebuild_index(profile).await?;
        }

        let query_text = prefixed_query(profile, query);
        let query_embedding = self
            .ensure_model(profile, self.config.auto_download_models)?
            .embed([query_text], Some(1))
            .context("embed query")?
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
        let mut best_by_message: HashMap<MessageId, f32> = HashMap::new();

        for neighbour in neighbours {
            let Some(chunk) = index.chunks_by_id.get(&neighbour.d_id) else {
                continue;
            };
            let similarity = 1.0 - neighbour.distance;
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
        Ok(hits)
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

        let accounts = self.store.list_accounts().await?;
        let mut envelopes = Vec::new();
        for account in accounts {
            envelopes.extend(
                self.store
                    .list_envelopes_by_account(&account.id, 10_000, 0)
                    .await?,
            );
        }

        record.progress_total = envelopes.len() as u32;
        self.store.upsert_semantic_profile(&record).await?;
        let now = chrono::Utc::now();

        for envelope in &envelopes {
            let body = self.store.get_body(&envelope.id).await?;
            let (chunks, embeddings) =
                self.build_message_records(&record, envelope, body.as_ref(), now)?;
            self.store
                .replace_semantic_message_data(&envelope.id, &record.id, &chunks, &embeddings)
                .await?;
            record.progress_completed += 1;
        }

        record.status = SemanticProfileStatus::Ready;
        record.last_indexed_at = Some(chrono::Utc::now());
        if record.activated_at.is_none() && self.config.active_profile == profile {
            record.activated_at = Some(chrono::Utc::now());
        }
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
                },
            );
        }
        hnsw.set_searching_mode(true);

        self.indexes
            .insert(profile, SemanticIndex { hnsw, chunks_by_id });
        Ok(())
    }

    fn build_message_records(
        &mut self,
        profile: &SemanticProfileRecord,
        envelope: &Envelope,
        body: Option<&MessageBody>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(Vec<SemanticChunkRecord>, Vec<SemanticEmbeddingRecord>)> {
        let chunks = build_chunks(envelope, body);
        if chunks.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }

        let texts = chunks
            .iter()
            .map(|chunk| prefixed_document(profile.profile, &chunk.1))
            .collect::<Vec<_>>();
        let embeddings = self
            .ensure_model(profile.profile, self.config.auto_download_models)?
            .embed(texts, Some(32))
            .context("embed message chunks")?;

        let mut chunk_records = Vec::with_capacity(chunks.len());
        let mut embedding_records = Vec::with_capacity(chunks.len());

        for (index, ((source_kind, normalized), embedding)) in
            chunks.into_iter().zip(embeddings.into_iter()).enumerate()
        {
            let chunk_id = semantic_chunk_id(&envelope.id.as_str(), &source_kind, index as u32);
            let chunk_record = SemanticChunkRecord {
                id: chunk_id.clone(),
                message_id: envelope.id.clone(),
                source_kind,
                ordinal: index as u32,
                normalized: normalized.clone(),
                content_hash: content_hash(&normalized),
                created_at: now,
                updated_at: now,
            };
            let embedding_record = SemanticEmbeddingRecord {
                chunk_id,
                profile_id: profile.id.clone(),
                dimensions: embedding.len() as u32,
                vector: f32s_to_blob(&embedding),
                status: SemanticEmbeddingStatus::Ready,
                created_at: now,
                updated_at: now,
            };
            chunk_records.push(chunk_record);
            embedding_records.push(embedding_record);
        }

        Ok((chunk_records, embedding_records))
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

pub fn should_use_semantic(mode: SearchMode) -> bool {
    matches!(mode, SearchMode::Hybrid | SearchMode::Semantic)
}

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

fn semantic_profile_id(profile: SemanticProfile) -> SemanticProfileId {
    SemanticProfileId::from_provider_id("semantic_profile", profile.as_str())
}

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

fn read_attachment_text(attachment: &AttachmentMeta) -> Option<String> {
    let path = attachment.local_path.as_ref()?;
    let mime = attachment.mime_type.to_ascii_lowercase();
    if !(mime.starts_with("text/")
        || mime == "application/json"
        || mime == "application/xml"
        || mime == "text/html")
    {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let normalized = if mime == "text/html" {
        clean(None, Some(&content), &ReaderConfig::default()).content
    } else {
        normalize_text(&content)
    };
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn embedding_model(profile: SemanticProfile) -> EmbeddingModel {
    match profile {
        SemanticProfile::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
        SemanticProfile::MultilingualE5Small => EmbeddingModel::MultilingualE5Small,
        SemanticProfile::BgeM3 => EmbeddingModel::BGEM3,
    }
}

fn prefixed_query(profile: SemanticProfile, text: &str) -> String {
    let normalized = normalize_text(text);
    match profile {
        SemanticProfile::MultilingualE5Small => format!("query: {normalized}"),
        _ => normalized,
    }
}

fn prefixed_document(profile: SemanticProfile, text: &str) -> String {
    let normalized = normalize_text(text);
    match profile {
        SemanticProfile::MultilingualE5Small => format!("passage: {normalized}"),
        _ => normalized,
    }
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

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

fn content_hash(normalized: &str) -> String {
    let digest = Sha256::digest(normalized.as_bytes());
    format!("{digest:x}")
}

fn f32s_to_blob(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn blob_to_f32s(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
