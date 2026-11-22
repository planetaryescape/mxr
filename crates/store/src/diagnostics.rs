use mxr_core::types::MessageFlags;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreRecordCounts {
    pub accounts: u32,
    pub labels: u32,
    pub messages: u32,
    pub unread_messages: u32,
    pub starred_messages: u32,
    pub messages_with_attachments: u32,
    pub message_labels: u32,
    pub bodies: u32,
    pub attachments: u32,
    pub drafts: u32,
    pub snoozed: u32,
    pub saved_searches: u32,
    pub rules: u32,
    pub rule_logs: u32,
    pub sync_log: u32,
    pub sync_runtime_statuses: u32,
    pub event_log: u32,
    pub semantic_profiles: u32,
    pub semantic_chunks: u32,
    pub semantic_embeddings: u32,
    pub messages_missing_semantic_chunks: u32,
    pub semantic_chunks_missing_embeddings: u32,
    pub relationship_drifts: u32,
}

impl super::Store {
    pub async fn collect_record_counts(&self) -> Result<StoreRecordCounts, sqlx::Error> {
        let read_flag = MessageFlags::READ.bits() as i64;
        let starred_flag = MessageFlags::STARRED.bits() as i64;
        let pool = self.reader();

        Ok(StoreRecordCounts {
            accounts: count_rows(pool, "SELECT COUNT(*) FROM accounts").await?,
            labels: count_rows(pool, "SELECT COUNT(*) FROM labels").await?,
            messages: count_rows(pool, "SELECT COUNT(*) FROM messages").await?,
            unread_messages: count_bound_rows(
                pool,
                "SELECT COUNT(*) FROM messages WHERE (flags & ?) = 0",
                read_flag,
            )
            .await?,
            starred_messages: count_bound_rows(
                pool,
                "SELECT COUNT(*) FROM messages WHERE (flags & ?) != 0",
                starred_flag,
            )
            .await?,
            messages_with_attachments: count_rows(
                pool,
                "SELECT COUNT(*) FROM messages WHERE has_attachments = 1",
            )
            .await?,
            message_labels: count_rows(pool, "SELECT COUNT(*) FROM message_labels").await?,
            bodies: count_rows(pool, "SELECT COUNT(*) FROM bodies").await?,
            attachments: count_rows(pool, "SELECT COUNT(*) FROM attachments").await?,
            drafts: count_rows(pool, "SELECT COUNT(*) FROM drafts").await?,
            snoozed: count_rows(pool, "SELECT COUNT(*) FROM snoozed").await?,
            saved_searches: count_rows(pool, "SELECT COUNT(*) FROM saved_searches").await?,
            rules: count_rows(pool, "SELECT COUNT(*) FROM rules").await?,
            rule_logs: count_rows(pool, "SELECT COUNT(*) FROM rule_execution_log").await?,
            sync_log: count_rows(pool, "SELECT COUNT(*) FROM sync_log").await?,
            sync_runtime_statuses: count_rows(pool, "SELECT COUNT(*) FROM sync_runtime_status")
                .await?,
            event_log: count_rows(pool, "SELECT COUNT(*) FROM event_log").await?,
            semantic_profiles: count_rows(pool, "SELECT COUNT(*) FROM semantic_profiles").await?,
            semantic_chunks: count_rows(pool, "SELECT COUNT(*) FROM semantic_chunks").await?,
            semantic_embeddings: count_rows(pool, "SELECT COUNT(*) FROM semantic_embeddings")
                .await?,
            messages_missing_semantic_chunks: count_rows(
                pool,
                "SELECT COUNT(*) FROM messages m WHERE NOT EXISTS (SELECT 1 FROM semantic_chunks c WHERE c.message_id = m.id)",
            )
            .await?,
            semantic_chunks_missing_embeddings: count_rows(
                pool,
                "SELECT COUNT(*) FROM semantic_chunks c WHERE NOT EXISTS (SELECT 1 FROM semantic_embeddings e WHERE e.chunk_id = c.id)",
            )
            .await?,
            relationship_drifts: count_rows(
                pool,
                "SELECT COUNT(*) FROM contact_style WHERE drift_detected = 1",
            )
            .await?,
        })
    }
}

async fn count_rows(pool: &SqlitePool, sql: &str) -> Result<u32, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await?
        .max(0) as u32)
}

async fn count_bound_rows(pool: &SqlitePool, sql: &str, value: i64) -> Result<u32, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
        .bind(value)
        .fetch_one(pool)
        .await?
        .max(0) as u32)
}
