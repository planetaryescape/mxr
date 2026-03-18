use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::{MailSyncProvider, MxrError};
use mxr_search::SearchIndex;
use mxr_store::Store;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SyncEngine {
    store: Arc<Store>,
    search: Arc<Mutex<SearchIndex>>,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>, search: Arc<Mutex<SearchIndex>>) -> Self {
        Self { store, search }
    }

    pub async fn sync_account(&self, provider: &dyn MailSyncProvider) -> Result<u32, MxrError> {
        let account_id = provider.account_id();
        let cursor = self
            .store
            .get_sync_cursor(account_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
            .unwrap_or(SyncCursor::Initial);

        // Sync labels
        let labels = provider.sync_labels().await?;
        for label in &labels {
            self.store
                .upsert_label(label)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
        }

        // Sync messages
        let batch = provider.sync_messages(&cursor).await?;
        let synced_count = batch.upserted.len() as u32;

        // Apply upserts
        {
            let mut search = self.search.lock().await;
            for envelope in &batch.upserted {
                self.store
                    .upsert_envelope(envelope)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                search.index_envelope(envelope)?;
            }

            if !batch.deleted_provider_ids.is_empty() {
                self.store
                    .delete_messages_by_provider_ids(account_id, &batch.deleted_provider_ids)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
            }

            search.commit()?;
        }

        // Update cursor
        self.store
            .set_sync_cursor(account_id, &batch.next_cursor)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        Ok(synced_count)
    }

    pub async fn fetch_body(
        &self,
        provider: &dyn MailSyncProvider,
        message_id: &MessageId,
    ) -> Result<MessageBody, MxrError> {
        // Check cache
        if let Some(body) = self
            .store
            .get_body(message_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
        {
            return Ok(body);
        }

        // Cache miss: fetch from provider
        let envelope = self
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
            .ok_or_else(|| MxrError::NotFound(format!("Message {}", message_id)))?;

        let body = provider.fetch_body(&envelope.provider_id).await?;

        // Cache in store
        self.store
            .insert_body(&body)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        // Update search index with body text
        {
            let mut search = self.search.lock().await;
            search.index_body(&envelope, &body)?;
            search.commit()?;
        }

        Ok(body)
    }

    pub async fn check_snoozes(&self) -> Result<Vec<MessageId>, MxrError> {
        let now = chrono::Utc::now();
        let due = self
            .store
            .get_due_snoozes(now)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        let mut woken = Vec::new();
        for snoozed in &due {
            self.store
                .remove_snooze(&snoozed.message_id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            woken.push(snoozed.message_id.clone());
        }

        Ok(woken)
    }
}
