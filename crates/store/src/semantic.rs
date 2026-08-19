use crate::{decode_id, decode_json, decode_optional_timestamp, decode_timestamp, encode_json};
use mxr_core::id::*;
use mxr_core::types::*;
use sqlx::Row;

/// The minimum an ANN index needs per chunk: identity, the vector, and enough
/// text to render a search hit.
#[derive(Debug, Clone)]
pub struct SemanticIndexRow {
    pub chunk_id: SemanticChunkId,
    pub message_id: MessageId,
    pub source_kind: SemanticChunkSourceKind,
    pub snippet: String,
    pub vector: Vec<u8>,
}

impl super::Store {
    pub async fn list_semantic_profiles(&self) -> Result<Vec<SemanticProfileRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, profile_name, backend, model_revision, dimensions, status,
                      installed_at, activated_at, last_indexed_at,
                      progress_completed, progress_total, last_error
               FROM semantic_profiles
               ORDER BY profile_name ASC"#,
        )
        .fetch_all(self.reader())
        .await?;

        rows.into_iter().map(row_to_semantic_profile).collect()
    }

    pub async fn get_semantic_profile(
        &self,
        profile: SemanticProfile,
    ) -> Result<Option<SemanticProfileRecord>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, profile_name, backend, model_revision, dimensions, status,
                      installed_at, activated_at, last_indexed_at,
                      progress_completed, progress_total, last_error
               FROM semantic_profiles
               WHERE profile_name = ?"#,
        )
        .bind(profile.as_str())
        .fetch_optional(self.reader())
        .await?;

        row.map(row_to_semantic_profile).transpose()
    }

    pub async fn upsert_semantic_profile(
        &self,
        profile: &SemanticProfileRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO semantic_profiles
               (id, profile_name, backend, model_revision, dimensions, status,
                installed_at, activated_at, last_indexed_at,
                progress_completed, progress_total, last_error)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                   profile_name = excluded.profile_name,
                   backend = excluded.backend,
                   model_revision = excluded.model_revision,
                   dimensions = excluded.dimensions,
                   status = excluded.status,
                   installed_at = excluded.installed_at,
                   activated_at = excluded.activated_at,
                   last_indexed_at = excluded.last_indexed_at,
                   progress_completed = excluded.progress_completed,
                   progress_total = excluded.progress_total,
                   last_error = excluded.last_error"#,
        )
        .bind(profile.id.as_str())
        .bind(profile.profile.as_str())
        .bind(&profile.backend)
        .bind(&profile.model_revision)
        .bind(profile.dimensions as i64)
        .bind(encode_json(&profile.status)?)
        .bind(profile.installed_at.map(|v| v.timestamp()))
        .bind(profile.activated_at.map(|v| v.timestamp()))
        .bind(profile.last_indexed_at.map(|v| v.timestamp()))
        .bind(profile.progress_completed as i64)
        .bind(profile.progress_total as i64)
        .bind(&profile.last_error)
        .execute(self.writer())
        .await?;

        Ok(())
    }

    pub async fn replace_semantic_chunks(
        &self,
        message_id: &MessageId,
        chunks: &[SemanticChunkRecord],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let message_id_str = message_id.as_str();

        sqlx::query("DELETE FROM semantic_chunks WHERE message_id = ?")
            .bind(&message_id_str)
            .execute(&mut *tx)
            .await?;

        for chunk in chunks {
            sqlx::query(
                r#"INSERT INTO semantic_chunks
                   (id, message_id, source_kind, ordinal, normalized, content_hash, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(chunk.id.as_str())
            .bind(chunk.message_id.as_str())
            .bind(encode_json(&chunk.source_kind)?)
            .bind(chunk.ordinal as i64)
            .bind(&chunk.normalized)
            .bind(&chunk.content_hash)
            .bind(chunk.created_at.timestamp())
            .bind(chunk.updated_at.timestamp())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn replace_semantic_embeddings(
        &self,
        message_id: &MessageId,
        profile_id: &SemanticProfileId,
        embeddings: &[SemanticEmbeddingRecord],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let message_id_str = message_id.as_str();
        let profile_id_str = profile_id.as_str();

        sqlx::query(
            r#"DELETE FROM semantic_embeddings
               WHERE profile_id = ?
                 AND chunk_id IN (
                    SELECT id FROM semantic_chunks WHERE message_id = ?
               )"#,
        )
        .bind(profile_id_str)
        .bind(&message_id_str)
        .execute(&mut *tx)
        .await?;

        for embedding in embeddings {
            sqlx::query(
                r#"INSERT INTO semantic_embeddings
                   (chunk_id, profile_id, dimensions, vector_blob, status, created_at, updated_at)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(embedding.chunk_id.as_str())
            .bind(embedding.profile_id.as_str())
            .bind(embedding.dimensions as i64)
            .bind(&embedding.vector)
            .bind(encode_json(&embedding.status)?)
            .bind(embedding.created_at.timestamp())
            .bind(embedding.updated_at.timestamp())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_semantic_chunks(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<SemanticChunkRecord>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, message_id, source_kind, ordinal, normalized, content_hash, created_at, updated_at
               FROM semantic_chunks
               WHERE message_id = ?
               ORDER BY ordinal ASC"#,
        )
        .bind(message_id.as_str())
        .fetch_all(self.reader())
        .await?;

        rows.into_iter().map(row_to_semantic_chunk).collect()
    }

    /// One keyset page of the rows an ANN index needs, ordered by chunk id.
    ///
    /// Deliberately narrower than [`Self::list_semantic_embeddings`]: it skips
    /// the content hash and timestamps and truncates the chunk text to the
    /// snippet length, so building an index over a large mailbox never holds
    /// the full chunk corpus in memory.
    pub async fn list_semantic_index_rows_after(
        &self,
        profile_id: &SemanticProfileId,
        after_chunk_id: Option<&SemanticChunkId>,
        snippet_chars: u32,
        limit: u32,
    ) -> Result<Vec<SemanticIndexRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT c.id, c.message_id, c.source_kind,
                      substr(c.normalized, 1, ?3) AS snippet, e.vector_blob
               FROM semantic_embeddings e
               JOIN semantic_chunks c ON c.id = e.chunk_id
               WHERE e.profile_id = ?1
                 AND (?2 IS NULL OR c.id > ?2)
               ORDER BY c.id ASC
               LIMIT ?4"#,
        )
        .bind(profile_id.as_str())
        .bind(after_chunk_id.map(SemanticChunkId::as_str))
        .bind(i64::from(snippet_chars))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(SemanticIndexRow {
                    chunk_id: decode_id(&row.get::<String, _>("id"))?,
                    message_id: decode_id(&row.get::<String, _>("message_id"))?,
                    source_kind: decode_json(&row.get::<String, _>("source_kind"))?,
                    snippet: row.get::<String, _>("snippet"),
                    vector: row.get::<Vec<u8>, _>("vector_blob"),
                })
            })
            .collect()
    }

    /// One keyset page of message ids for a semantic index pass.
    ///
    /// Lives here rather than in `message.rs` because it exists purely for the
    /// semantic engine: it returns ids only (no envelope hydration) so a full
    /// pass over a large mailbox stays cheap, and it pages on the primary key
    /// so there is no per-account row cap.
    pub async fn list_message_ids_after(
        &self,
        after: Option<&MessageId>,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"SELECT id
               FROM messages
               WHERE ?1 IS NULL OR id > ?1
               ORDER BY id ASC
               LIMIT ?2"#,
        )
        .bind(after.map(MessageId::as_str))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|id| decode_id(&id)).collect()
    }

    /// Stored chunk ids and content hashes for a message, in ordinal order.
    ///
    /// Callers use this to tell whether freshly extracted chunks differ from
    /// what is stored, so an unchanged message can skip a chunk rewrite (which
    /// would cascade its embeddings away) and a re-embed.
    pub async fn list_semantic_chunk_fingerprints(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<(SemanticChunkId, String)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT id, content_hash
               FROM semantic_chunks
               WHERE message_id = ?
               ORDER BY ordinal ASC"#,
        )
        .bind(message_id.as_str())
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok((
                    decode_id(&row.get::<String, _>("id"))?,
                    row.get::<String, _>("content_hash"),
                ))
            })
            .collect()
    }

    /// Chunks of a message that have no embedding for `profile_id` at the
    /// profile's current `dimensions`. Zero means the message is already
    /// indexed for that profile and can be skipped.
    pub async fn count_semantic_chunks_missing_embeddings(
        &self,
        message_id: &MessageId,
        profile_id: &SemanticProfileId,
        dimensions: u32,
    ) -> Result<u32, sqlx::Error> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*)
               FROM semantic_chunks c
               WHERE c.message_id = ?
                 AND NOT EXISTS (
                     SELECT 1
                     FROM semantic_embeddings e
                     WHERE e.chunk_id = c.id
                       AND e.profile_id = ?
                       AND e.dimensions = ?
                 )"#,
        )
        .bind(message_id.as_str())
        .bind(profile_id.as_str())
        .bind(i64::from(dimensions))
        .fetch_one(self.reader())
        .await?
        .max(0) as u32)
    }

    pub async fn list_semantic_embeddings(
        &self,
        profile_id: &SemanticProfileId,
    ) -> Result<Vec<(SemanticChunkRecord, SemanticEmbeddingRecord)>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT
                   c.id as chunk_id,
                   c.message_id,
                   c.source_kind,
                   c.ordinal,
                   c.normalized,
                   c.content_hash,
                   c.created_at as chunk_created_at,
                   c.updated_at as chunk_updated_at,
                   e.profile_id,
                   e.dimensions,
                   e.vector_blob,
                   e.status,
                   e.created_at as embedding_created_at,
                   e.updated_at as embedding_updated_at
               FROM semantic_embeddings e
               JOIN semantic_chunks c ON c.id = e.chunk_id
               WHERE e.profile_id = ?
               ORDER BY c.message_id ASC, c.ordinal ASC"#,
        )
        .bind(profile_id.as_str())
        .fetch_all(self.reader())
        .await?;

        rows.into_iter()
            .map(|row| {
                let chunk = SemanticChunkRecord {
                    id: decode_id(&row.get::<String, _>("chunk_id"))?,
                    message_id: decode_id(&row.get::<String, _>("message_id"))?,
                    source_kind: decode_json(&row.get::<String, _>("source_kind"))?,
                    ordinal: row.get::<i64, _>("ordinal") as u32,
                    normalized: row.get::<String, _>("normalized"),
                    content_hash: row.get::<String, _>("content_hash"),
                    created_at: decode_timestamp(row.get::<i64, _>("chunk_created_at"))?,
                    updated_at: decode_timestamp(row.get::<i64, _>("chunk_updated_at"))?,
                };
                let embedding = SemanticEmbeddingRecord {
                    chunk_id: chunk.id.clone(),
                    profile_id: decode_id(&row.get::<String, _>("profile_id"))?,
                    dimensions: row.get::<i64, _>("dimensions") as u32,
                    vector: row.get::<Vec<u8>, _>("vector_blob"),
                    status: decode_json(&row.get::<String, _>("status"))?,
                    created_at: decode_timestamp(row.get::<i64, _>("embedding_created_at"))?,
                    updated_at: decode_timestamp(row.get::<i64, _>("embedding_updated_at"))?,
                };
                Ok((chunk, embedding))
            })
            .collect()
    }

    pub async fn count_messages_missing_semantic_chunks(&self) -> Result<u32, sqlx::Error> {
        count_semantic_gap(
            self.reader(),
            r#"SELECT COUNT(*)
               FROM messages m
               WHERE NOT EXISTS (
                   SELECT 1 FROM semantic_chunks c WHERE c.message_id = m.id
               )"#,
        )
        .await
    }

    pub async fn count_messages_missing_semantic_embeddings(
        &self,
        profile_id: &SemanticProfileId,
    ) -> Result<u32, sqlx::Error> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(DISTINCT c.message_id)
               FROM semantic_chunks c
               WHERE NOT EXISTS (
                   SELECT 1
                   FROM semantic_embeddings e
                   WHERE e.chunk_id = c.id AND e.profile_id = ?
               )"#,
        )
        .bind(profile_id.as_str())
        .fetch_one(self.reader())
        .await?
        .max(0) as u32)
    }

    pub async fn list_message_ids_missing_semantic_chunks(
        &self,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"SELECT m.id
               FROM messages m
               WHERE NOT EXISTS (
                   SELECT 1 FROM semantic_chunks c WHERE c.message_id = m.id
               )
               ORDER BY m.date DESC
               LIMIT ?"#,
        )
        .bind(limit as i64)
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|id| decode_id(&id)).collect()
    }

    pub async fn list_message_ids_missing_semantic_embeddings(
        &self,
        profile_id: &SemanticProfileId,
        limit: u32,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"SELECT DISTINCT c.message_id
               FROM semantic_chunks c
               WHERE NOT EXISTS (
                   SELECT 1
                   FROM semantic_embeddings e
                   WHERE e.chunk_id = c.id AND e.profile_id = ?
               )
               ORDER BY c.updated_at DESC
               LIMIT ?"#,
        )
        .bind(profile_id.as_str())
        .bind(limit as i64)
        .fetch_all(self.reader())
        .await?;
        rows.into_iter().map(|id| decode_id(&id)).collect()
    }
}

async fn count_semantic_gap(pool: &sqlx::SqlitePool, sql: &str) -> Result<u32, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?
        .max(0) as u32)
}

fn row_to_semantic_chunk(row: sqlx::sqlite::SqliteRow) -> Result<SemanticChunkRecord, sqlx::Error> {
    Ok(SemanticChunkRecord {
        id: decode_id(&row.get::<String, _>("id"))?,
        message_id: decode_id(&row.get::<String, _>("message_id"))?,
        source_kind: decode_json(&row.get::<String, _>("source_kind"))?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        normalized: row.get::<String, _>("normalized"),
        content_hash: row.get::<String, _>("content_hash"),
        created_at: decode_timestamp(row.get::<i64, _>("created_at"))?,
        updated_at: decode_timestamp(row.get::<i64, _>("updated_at"))?,
    })
}

fn row_to_semantic_profile(
    row: sqlx::sqlite::SqliteRow,
) -> Result<SemanticProfileRecord, sqlx::Error> {
    Ok(SemanticProfileRecord {
        id: decode_id(&row.get::<String, _>("id"))?,
        profile: serde_json::from_value(serde_json::Value::String(
            row.get::<String, _>("profile_name"),
        ))
        .map_err(sqlx::Error::decode)?,
        backend: row.get::<String, _>("backend"),
        model_revision: row.get::<String, _>("model_revision"),
        dimensions: row.get::<i64, _>("dimensions") as u32,
        status: decode_json(&row.get::<String, _>("status"))?,
        installed_at: decode_optional_timestamp(row.get::<Option<i64>, _>("installed_at"))?,
        activated_at: decode_optional_timestamp(row.get::<Option<i64>, _>("activated_at"))?,
        last_indexed_at: decode_optional_timestamp(row.get::<Option<i64>, _>("last_indexed_at"))?,
        progress_completed: row.get::<i64, _>("progress_completed") as u32,
        progress_total: row.get::<i64, _>("progress_total") as u32,
        last_error: row.get::<Option<String>, _>("last_error"),
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};

    #[tokio::test]
    async fn replacing_chunks_cascades_existing_embeddings_for_all_profiles() {
        let store = super::super::Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();

        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();

        let now = chrono::Utc::now();
        let original_chunk = SemanticChunkRecord {
            id: SemanticChunkId::new(),
            message_id: envelope.id.clone(),
            source_kind: SemanticChunkSourceKind::Body,
            ordinal: 0,
            normalized: "old body".into(),
            content_hash: "old-hash".into(),
            created_at: now,
            updated_at: now,
        };
        store
            .replace_semantic_chunks(&envelope.id, std::slice::from_ref(&original_chunk))
            .await
            .unwrap();

        for profile in [SemanticProfile::BgeSmallEnV15, SemanticProfile::BgeM3] {
            let profile_record = SemanticProfileRecord {
                id: SemanticProfileId::new(),
                profile,
                backend: "test".into(),
                model_revision: "test".into(),
                dimensions: 2,
                status: SemanticProfileStatus::Ready,
                installed_at: Some(now),
                activated_at: Some(now),
                last_indexed_at: Some(now),
                progress_completed: 1,
                progress_total: 1,
                last_error: None,
            };
            store
                .upsert_semantic_profile(&profile_record)
                .await
                .unwrap();

            let embedding = SemanticEmbeddingRecord {
                chunk_id: original_chunk.id.clone(),
                profile_id: profile_record.id.clone(),
                dimensions: 2,
                vector: vec![1, 2, 3, 4, 5, 6, 7, 8],
                status: SemanticEmbeddingStatus::Ready,
                created_at: now,
                updated_at: now,
            };
            store
                .replace_semantic_embeddings(
                    &envelope.id,
                    &embedding.profile_id,
                    std::slice::from_ref(&embedding),
                )
                .await
                .unwrap();
        }

        let replacement_chunk = SemanticChunkRecord {
            id: SemanticChunkId::new(),
            message_id: envelope.id.clone(),
            source_kind: SemanticChunkSourceKind::Body,
            ordinal: 0,
            normalized: "new body".into(),
            content_hash: "new-hash".into(),
            created_at: now,
            updated_at: now,
        };
        store
            .replace_semantic_chunks(&envelope.id, std::slice::from_ref(&replacement_chunk))
            .await
            .unwrap();

        let counts = store.collect_record_counts().await.unwrap();
        assert_eq!(counts.semantic_chunks, 1);
        assert_eq!(counts.semantic_embeddings, 0);

        let chunks = store.list_semantic_chunks(&envelope.id).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].normalized, "new body");
    }
}
