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

        // Apply upserts
        {
            let mut search = self.search.lock().await;
            for envelope in &batch.upserted {
                self.store
                    .upsert_envelope(envelope)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                search.index_envelope(envelope)?;

                // Populate message_labels junction table
                if !envelope.label_provider_ids.is_empty() {
                    let label_ids = self
                        .store
                        .find_labels_by_provider_ids(account_id, &envelope.label_provider_ids)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                    tracing::debug!(
                        message_id = %envelope.id,
                        provider_label_count = envelope.label_provider_ids.len(),
                        resolved_label_count = label_ids.len(),
                        "resolved provider label IDs to internal label IDs"
                    );
                    if !label_ids.is_empty() {
                        self.store
                            .set_message_labels(&envelope.id, &label_ids)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                    }
                }
            }

            if !batch.deleted_provider_ids.is_empty() {
                self.store
                    .delete_messages_by_provider_ids(account_id, &batch.deleted_provider_ids)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
            }

            search.commit()?;
        }

        // Recalculate label counts — skip during backfill (will run after backfill completes)
        if !matches!(batch.next_cursor, SyncCursor::GmailBackfill { .. }) {
            self.store
                .recalculate_label_counts(account_id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
        }

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

    /// Pre-fetch bodies for messages that don't have cached bodies yet.
    /// Fetches up to `batch_size` bodies, newest first, with a small delay
    /// between fetches to avoid rate limiting. Returns count of bodies fetched.
    /// Stops early on rate limit errors.
    pub async fn prefetch_bodies(
        &self,
        provider: &dyn MailSyncProvider,
        account_id: &AccountId,
        batch_size: u32,
    ) -> Result<u32, MxrError> {
        let envelopes = self
            .store
            .list_envelopes_without_bodies(account_id, batch_size)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        if envelopes.is_empty() {
            return Ok(0);
        }

        tracing::debug!(
            unfetched = envelopes.len(),
            "starting body prefetch batch"
        );

        let mut fetched = 0u32;
        for env in &envelopes {
            match self.fetch_body(provider, &env.id).await {
                Ok(_) => {
                    fetched += 1;
                    if fetched % 10 == 0 {
                        tracing::info!("Pre-fetched {fetched} bodies");
                    }
                }
                Err(e) => {
                    if e.to_string().contains("Rate limited") {
                        tracing::info!(
                            "Body prefetch paused (rate limited), {fetched} fetched this batch"
                        );
                        return Ok(fetched);
                    }
                    tracing::debug!("Body prefetch failed for {}: {e}", env.id);
                }
            }

            // Small delay between fetches to avoid rate limits
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }

        if fetched > 0 {
            tracing::info!("Body prefetch batch complete: {fetched} bodies cached");
        }

        Ok(fetched)
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
