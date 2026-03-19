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

        // Sync labels — skip during backfill to avoid slowing down pagination
        if !matches!(cursor, SyncCursor::GmailBackfill { .. }) {
            let labels = provider.sync_labels().await?;
            tracing::debug!(count = labels.len(), "synced labels from provider");
            for label in &labels {
                self.store
                    .upsert_label(label)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
            }
        }

        // Sync messages
        tracing::info!(cursor = ?cursor, "sync_account: dispatching with cursor");
        let batch = provider.sync_messages(&cursor).await?;
        let synced_count = batch.upserted.len() as u32;

        // Apply upserts — store envelope + body, index with body text
        for synced in &batch.upserted {
            // Store envelope
            self.store
                .upsert_envelope(&synced.envelope)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;

            // Store body (eagerly fetched during sync)
            self.store
                .insert_body(&synced.body)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;

            // Populate message_labels junction table
            if !synced.envelope.label_provider_ids.is_empty() {
                let label_ids = self
                    .store
                    .find_labels_by_provider_ids(account_id, &synced.envelope.label_provider_ids)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                if !label_ids.is_empty() {
                    self.store
                        .set_message_labels(&synced.envelope.id, &label_ids)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                }
            }

            // Search index — index with body text for immediate full-text search
            {
                let mut search = self.search.lock().await;
                search.index_body(&synced.envelope, &synced.body)?;
            }
        }

        // Deletions (store-only, no search lock)
        if !batch.deleted_provider_ids.is_empty() {
            self.store
                .delete_messages_by_provider_ids(account_id, &batch.deleted_provider_ids)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
        }

        // Apply label changes from delta sync (previously dead code)
        for change in &batch.label_changes {
            if let Ok(Some(message_id)) = self
                .store
                .get_message_id_by_provider_id(account_id, &change.provider_message_id)
                .await
            {
                if !change.added_labels.is_empty() {
                    if let Ok(add_ids) = self
                        .store
                        .find_labels_by_provider_ids(account_id, &change.added_labels)
                        .await
                    {
                        for lid in &add_ids {
                            let _ = self.store.add_message_label(&message_id, lid).await;
                        }
                    }
                }
                if !change.removed_labels.is_empty() {
                    if let Ok(rm_ids) = self
                        .store
                        .find_labels_by_provider_ids(account_id, &change.removed_labels)
                        .await
                    {
                        for lid in &rm_ids {
                            let _ = self.store.remove_message_label(&message_id, lid).await;
                        }
                    }
                }
            }
        }

        // Commit search index
        {
            let mut search = self.search.lock().await;
            search.commit()?;
        }

        // Recalculate label counts every batch (including during backfill)
        self.store
            .recalculate_label_counts(account_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        // Update cursor
        tracing::info!(next_cursor = ?batch.next_cursor, "sync_account: saving cursor");
        self.store
            .set_sync_cursor(account_id, &batch.next_cursor)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        // Backfill: if junction table is empty but messages exist, reset cursor
        // and re-sync to rebuild label associations (handles DBs corrupted by
        // the old INSERT OR REPLACE cascade bug).
        let junction_count = self
            .store
            .count_message_labels()
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;
        let message_count = self
            .store
            .count_messages_by_account(account_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;
        if junction_count == 0 && message_count > 0 {
            tracing::warn!(
                message_count,
                "Junction table empty — resetting sync cursor for full re-sync"
            );
            self.store
                .set_sync_cursor(account_id, &SyncCursor::Initial)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            return Box::pin(self.sync_account(provider)).await;
        }

        Ok(synced_count)
    }

    /// Read body from store. Bodies are always available after sync.
    pub async fn get_body(&self, message_id: &MessageId) -> Result<MessageBody, MxrError> {
        self.store
            .get_body(message_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
            .ok_or_else(|| MxrError::NotFound(format!("Body for message {}", message_id)))
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
