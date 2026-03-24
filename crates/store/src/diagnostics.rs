use crate::mxr_core::types::MessageFlags;
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
        })
    }
}

async fn count_rows(pool: &SqlitePool, sql: &str) -> Result<u32, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await?
        .max(0) as u32)
}

async fn count_bound_rows(pool: &SqlitePool, sql: &str, value: i64) -> Result<u32, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i64>(sql)
        .bind(value)
        .fetch_one(pool)
        .await?
        .max(0) as u32)
}

#[cfg(test)]
mod tests {
    use super::StoreRecordCounts;
    use crate::mxr_core::id::{
        AccountId, DraftId, MessageId, SavedSearchId, SemanticChunkId, SemanticProfileId, ThreadId,
    };
    use crate::mxr_core::types::{
        Address, BackendRef, Draft, MessageBody, MessageFlags, ProviderKind, SearchMode,
        SemanticChunkRecord, SemanticChunkSourceKind, SemanticEmbeddingRecord,
        SemanticEmbeddingStatus, SemanticProfile, SemanticProfileRecord, SemanticProfileStatus,
        Snoozed, SortOrder, UnsubscribeMethod,
    };

    #[tokio::test]
    async fn collect_record_counts_reports_core_tables() {
        let store = crate::mxr_store::Store::in_memory().await.unwrap();
        let account = crate::mxr_core::Account {
            id: AccountId::new(),
            name: "Test".into(),
            email: "test@example.com".into(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".into(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.unwrap();

        let label = crate::mxr_core::types::Label {
            id: crate::mxr_core::id::LabelId::new(),
            account_id: account.id.clone(),
            name: "Inbox".into(),
            kind: crate::mxr_core::types::LabelKind::System,
            color: None,
            provider_id: "INBOX".into(),
            unread_count: 0,
            total_count: 0,
        };
        store.upsert_label(&label).await.unwrap();

        let message_id = MessageId::new();
        let envelope = crate::mxr_core::types::Envelope {
            id: message_id.clone(),
            account_id: account.id.clone(),
            provider_id: "fake-1".into(),
            thread_id: ThreadId::new(),
            message_id_header: Some("<msg@example.com>".into()),
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            to: vec![Address {
                name: None,
                email: "bob@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Subject".into(),
            date: chrono::Utc::now(),
            flags: MessageFlags::STARRED,
            snippet: "snippet".into(),
            has_attachments: true,
            size_bytes: 10,
            unsubscribe: UnsubscribeMethod::None,
            label_provider_ids: vec![label.provider_id.clone()],
        };
        store.upsert_envelope(&envelope).await.unwrap();
        store
            .set_message_labels(&message_id, std::slice::from_ref(&label.id))
            .await
            .unwrap();

        let body = MessageBody {
            message_id: message_id.clone(),
            text_plain: Some("body".into()),
            text_html: None,
            attachments: vec![crate::mxr_core::types::AttachmentMeta {
                id: crate::mxr_core::id::AttachmentId::new(),
                message_id: message_id.clone(),
                filename: "notes.txt".into(),
                mime_type: "text/plain".into(),
                size_bytes: 4,
                local_path: None,
                provider_id: "att-1".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: Default::default(),
        };
        store.insert_body(&body).await.unwrap();

        let saved = crate::mxr_core::types::SavedSearch {
            id: SavedSearchId::new(),
            account_id: None,
            name: "Unread".into(),
            query: "is:unread".into(),
            search_mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            icon: None,
            position: 0,
            created_at: chrono::Utc::now(),
        };
        store.insert_saved_search(&saved).await.unwrap();

        let draft = Draft {
            id: DraftId::new(),
            account_id: account.id.clone(),
            reply_headers: None,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "draft".into(),
            body_markdown: "body".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store.insert_draft(&draft).await.unwrap();

        let snoozed = Snoozed {
            message_id: message_id.clone(),
            account_id: account.id.clone(),
            snoozed_at: chrono::Utc::now(),
            wake_at: chrono::Utc::now(),
            original_labels: vec![label.id.clone()],
        };
        store.insert_snooze(&snoozed).await.unwrap();

        store
            .upsert_rule(crate::mxr_store::RuleRecordInput {
                id: "rule-1",
                name: "Archive",
                enabled: true,
                priority: 10,
                conditions_json: r#"{"type":"all"}"#,
                actions_json: r#"[{"type":"archive"}]"#,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        let message_id_str = message_id.as_str();
        store
            .insert_rule_log(crate::mxr_store::RuleLogInput {
                rule_id: "rule-1",
                rule_name: "Archive",
                message_id: &message_id_str,
                actions_applied_json: r#"["archive"]"#,
                timestamp: chrono::Utc::now(),
                success: true,
                error: None,
            })
            .await
            .unwrap();
        store
            .insert_event("info", "sync", "Sync complete", None, None)
            .await
            .unwrap();

        let profile = SemanticProfileRecord {
            id: SemanticProfileId::new(),
            profile: SemanticProfile::BgeSmallEnV15,
            backend: "local".into(),
            model_revision: "test".into(),
            dimensions: 384,
            status: SemanticProfileStatus::Ready,
            installed_at: Some(chrono::Utc::now()),
            activated_at: Some(chrono::Utc::now()),
            last_indexed_at: Some(chrono::Utc::now()),
            progress_completed: 1,
            progress_total: 1,
            last_error: None,
        };
        store.upsert_semantic_profile(&profile).await.unwrap();
        let chunk = SemanticChunkRecord {
            id: SemanticChunkId::new(),
            message_id: message_id.clone(),
            source_kind: SemanticChunkSourceKind::Body,
            ordinal: 0,
            normalized: "body".into(),
            content_hash: "hash".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let embedding = SemanticEmbeddingRecord {
            chunk_id: chunk.id.clone(),
            profile_id: profile.id.clone(),
            dimensions: 384,
            vector: vec![0; 16],
            status: SemanticEmbeddingStatus::Ready,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        store
            .replace_semantic_message_data(
                &message_id,
                &profile.id,
                std::slice::from_ref(&chunk),
                std::slice::from_ref(&embedding),
            )
            .await
            .unwrap();

        let counts = store.collect_record_counts().await.unwrap();
        assert_eq!(
            counts,
            StoreRecordCounts {
                accounts: 1,
                labels: 1,
                messages: 1,
                unread_messages: 1,
                starred_messages: 1,
                messages_with_attachments: 1,
                message_labels: 1,
                bodies: 1,
                attachments: 1,
                drafts: 1,
                snoozed: 1,
                saved_searches: 1,
                rules: 1,
                rule_logs: 1,
                sync_log: 0,
                sync_runtime_statuses: 0,
                event_log: 1,
                semantic_profiles: 1,
                semantic_chunks: 1,
                semantic_embeddings: 1,
            }
        );
    }
}
