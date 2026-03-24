use crate::mxr_core::id::*;
use crate::mxr_core::types::*;
use sqlx::Row;

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

        Ok(rows.into_iter().map(row_to_semantic_profile).collect())
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

        Ok(row.map(row_to_semantic_profile))
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
        .bind(serde_json::to_string(&profile.status).unwrap())
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

    pub async fn replace_semantic_message_data(
        &self,
        message_id: &MessageId,
        profile_id: &SemanticProfileId,
        chunks: &[SemanticChunkRecord],
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
            .bind(serde_json::to_string(&chunk.source_kind).unwrap())
            .bind(chunk.ordinal as i64)
            .bind(&chunk.normalized)
            .bind(&chunk.content_hash)
            .bind(chunk.created_at.timestamp())
            .bind(chunk.updated_at.timestamp())
            .execute(&mut *tx)
            .await?;
        }

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
            .bind(serde_json::to_string(&embedding.status).unwrap())
            .bind(embedding.created_at.timestamp())
            .bind(embedding.updated_at.timestamp())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
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

        Ok(rows
            .into_iter()
            .map(|row| {
                let chunk = SemanticChunkRecord {
                    id: SemanticChunkId::from_uuid(
                        uuid::Uuid::parse_str(&row.get::<String, _>("chunk_id")).unwrap(),
                    ),
                    message_id: MessageId::from_uuid(
                        uuid::Uuid::parse_str(&row.get::<String, _>("message_id")).unwrap(),
                    ),
                    source_kind: serde_json::from_str(&row.get::<String, _>("source_kind"))
                        .unwrap(),
                    ordinal: row.get::<i64, _>("ordinal") as u32,
                    normalized: row.get::<String, _>("normalized"),
                    content_hash: row.get::<String, _>("content_hash"),
                    created_at: chrono::DateTime::from_timestamp(
                        row.get::<i64, _>("chunk_created_at"),
                        0,
                    )
                    .unwrap_or_default(),
                    updated_at: chrono::DateTime::from_timestamp(
                        row.get::<i64, _>("chunk_updated_at"),
                        0,
                    )
                    .unwrap_or_default(),
                };
                let embedding = SemanticEmbeddingRecord {
                    chunk_id: chunk.id.clone(),
                    profile_id: SemanticProfileId::from_uuid(
                        uuid::Uuid::parse_str(&row.get::<String, _>("profile_id")).unwrap(),
                    ),
                    dimensions: row.get::<i64, _>("dimensions") as u32,
                    vector: row.get::<Vec<u8>, _>("vector_blob"),
                    status: serde_json::from_str(&row.get::<String, _>("status")).unwrap(),
                    created_at: chrono::DateTime::from_timestamp(
                        row.get::<i64, _>("embedding_created_at"),
                        0,
                    )
                    .unwrap_or_default(),
                    updated_at: chrono::DateTime::from_timestamp(
                        row.get::<i64, _>("embedding_updated_at"),
                        0,
                    )
                    .unwrap_or_default(),
                };
                (chunk, embedding)
            })
            .collect())
    }
}

fn row_to_semantic_profile(row: sqlx::sqlite::SqliteRow) -> SemanticProfileRecord {
    SemanticProfileRecord {
        id: SemanticProfileId::from_uuid(
            uuid::Uuid::parse_str(&row.get::<String, _>("id")).unwrap(),
        ),
        profile: serde_json::from_str(&format!("\"{}\"", row.get::<String, _>("profile_name")))
            .unwrap(),
        backend: row.get::<String, _>("backend"),
        model_revision: row.get::<String, _>("model_revision"),
        dimensions: row.get::<i64, _>("dimensions") as u32,
        status: serde_json::from_str(&row.get::<String, _>("status")).unwrap_or_default(),
        installed_at: row
            .get::<Option<i64>, _>("installed_at")
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)),
        activated_at: row
            .get::<Option<i64>, _>("activated_at")
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)),
        last_indexed_at: row
            .get::<Option<i64>, _>("last_indexed_at")
            .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)),
        progress_completed: row.get::<i64, _>("progress_completed") as u32,
        progress_total: row.get::<i64, _>("progress_total") as u32,
        last_error: row.get::<Option<String>, _>("last_error"),
    }
}
