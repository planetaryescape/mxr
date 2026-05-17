use mail_threading::{thread_messages, Message as ThreadingMessage};
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::{MailSyncProvider, MxrError};
use mxr_search::{SearchIndexEntry, SearchServiceHandle, SearchUpdateBatch};
use mxr_store::{ScreenerDisposition, Store};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SyncOutcome {
    pub synced_count: u32,
    pub upserted_message_ids: Vec<MessageId>,
}

/// No-op lookup used when the engine is constructed without an explicit
/// address source. Reports `is_loaded=false` so direction stays `Unknown`
/// rather than being misclassified as inbound.
struct NoopAddressLookup;
impl AccountAddressLookup for NoopAddressLookup {
    fn is_account_address(&self, _email: &str) -> bool {
        false
    }
    fn is_loaded(&self) -> bool {
        false
    }
}

pub struct SyncEngine {
    store: Arc<Store>,
    search: SearchServiceHandle,
    address_lookup: Arc<dyn AccountAddressLookup>,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>, search: SearchServiceHandle) -> Self {
        Self::with_address_lookup(store, search, Arc::new(NoopAddressLookup))
    }

    /// Construct a sync engine that classifies direction using the provided
    /// address lookup. Daemon code should use this constructor and supply
    /// an `InMemoryAccountAddressLookup` populated from the store.
    pub fn with_address_lookup(
        store: Arc<Store>,
        search: SearchServiceHandle,
        address_lookup: Arc<dyn AccountAddressLookup>,
    ) -> Self {
        Self {
            store,
            search,
            address_lookup,
        }
    }

    /// Returns the direction for a sender email based on the configured
    /// address lookup. Falls back to `Unknown` when the lookup hasn't been
    /// loaded yet — the doctor `--rebuild-analytics` command reclassifies.
    fn classify_direction(&self, from_email: &str) -> MessageDirection {
        if !self.address_lookup.is_loaded() {
            return MessageDirection::Unknown;
        }
        if self.address_lookup.is_account_address(from_email) {
            MessageDirection::Outbound
        } else {
            MessageDirection::Inbound
        }
    }

    async fn apply_screener_decision(
        &self,
        envelope: &Envelope,
        direction: MessageDirection,
    ) -> Result<Envelope, MxrError> {
        if direction != MessageDirection::Inbound {
            return Ok(envelope.clone());
        }

        let Some(decision) = self
            .store
            .get_screener_decision(&envelope.account_id, &envelope.from.email)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
        else {
            return Ok(envelope.clone());
        };

        let mut routed = envelope.clone();
        match decision.disposition {
            ScreenerDisposition::Allow => {
                add_route_label(
                    &mut routed.label_provider_ids,
                    decision.route_label.as_deref(),
                );
            }
            ScreenerDisposition::Deny => {
                routed.flags.insert(MessageFlags::READ);
                routed
                    .label_provider_ids
                    .retain(|label| !label.eq_ignore_ascii_case(system_labels::INBOX));
                add_unique_label(&mut routed.label_provider_ids, system_labels::TRASH);
                add_route_label(
                    &mut routed.label_provider_ids,
                    decision.route_label.as_deref(),
                );
            }
            ScreenerDisposition::Feed | ScreenerDisposition::PaperTrail => {
                routed
                    .label_provider_ids
                    .retain(|label| !label.eq_ignore_ascii_case(system_labels::INBOX));
                add_route_label(
                    &mut routed.label_provider_ids,
                    decision.route_label.as_deref(),
                );
            }
            ScreenerDisposition::Unknown => {}
        }
        Ok(routed)
    }

    pub async fn persist_synced_message(&self, synced: &SyncedMessage) -> Result<(), MxrError> {
        let mut body = synced.body.clone();
        body.ensure_best_effort_readable();

        let direction = self.classify_direction(&synced.envelope.from.email);
        let envelope = self
            .apply_screener_decision(&synced.envelope, direction)
            .await?;

        self.store
            .upsert_envelope_with_direction(&envelope, direction)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        // Slice 9: forward-populate `reply_pairs`. If the parent isn't in the
        // store yet (out-of-order delivery), park the reply for the
        // reconciler to pick up later.
        if synced.envelope.in_reply_to.is_some() {
            let resolved = self
                .store
                .try_create_reply_pair(&envelope, direction)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            if !resolved {
                self.store
                    .enqueue_reply_pair_pending(&envelope)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
            }
        }

        self.store
            .insert_body(&body)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        let label_ids = if envelope.label_provider_ids.is_empty() {
            Vec::new()
        } else {
            self.store
                .find_labels_by_provider_ids(&envelope.account_id, &envelope.label_provider_ids)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?
        };
        self.store
            .set_message_labels(&envelope.id, &label_ids, EventSource::Sync)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        self.search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: envelope.clone(),
                    body: Some(body),
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await?;

        Ok(())
    }

    pub async fn repair_body(
        &self,
        envelope: &Envelope,
        body: &MessageBody,
    ) -> Result<MessageBody, MxrError> {
        let mut normalized = body.clone();
        normalized.ensure_best_effort_readable();

        self.store
            .insert_body(&normalized)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        self.search
            .apply_batch(SearchUpdateBatch {
                entries: vec![SearchIndexEntry {
                    envelope: envelope.clone(),
                    body: Some(normalized.clone()),
                    reply_later: false,
                }],
                removed_message_ids: Vec::new(),
            })
            .await?;

        Ok(normalized)
    }

    pub async fn sync_account(&self, provider: &dyn MailSyncProvider) -> Result<u32, MxrError> {
        Ok(self.sync_account_with_outcome(provider).await?.synced_count)
    }

    pub async fn sync_account_with_outcome(
        &self,
        provider: &dyn MailSyncProvider,
    ) -> Result<SyncOutcome, MxrError> {
        let account_id = provider.account_id();
        let mut recovered_expired_cursor = false;
        tracing::debug!(account = %account_id, "sync_account_with_outcome: starting");

        loop {
            let cursor = self
                .store
                .get_sync_cursor(account_id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?
                .unwrap_or_default();

            // Sync labels — skip during backfill to avoid slowing down pagination
            if !provider.is_backfill_cursor(&cursor) {
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
            let batch = match provider.sync_messages(&cursor).await {
                Ok(batch) => batch,
                Err(MxrError::SyncCursorExpired { reason })
                    if !recovered_expired_cursor =>
                {
                    tracing::warn!(
                        account = %account_id,
                        cursor = ?cursor,
                        reason = %reason,
                        "provider sync cursor expired; resetting to initial sync"
                    );
                    self.store
                        .set_sync_cursor(account_id, &SyncCursor::empty())
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                    recovered_expired_cursor = true;
                    continue;
                }
                Err(error) => {
                    tracing::error!(
                        account = %account_id,
                        cursor = ?cursor,
                        error = %error,
                        "provider sync failed"
                    );
                    return Err(error);
                }
            };
            let synced_count = batch.upserted.len() as u32;
            let upserted_message_ids = batch
                .upserted
                .iter()
                .map(|synced| synced.envelope.id.clone())
                .collect::<Vec<_>>();
            let mut lexical_batch = SearchUpdateBatch::default();

            // Core sync guarantee: after this loop, SQLite has the envelope/body
            // pair and Tantivy has the same message's lexical corpus.
            // Semantic chunk prep is intentionally deferred to the daemon's
            // post-sync platform step so mail sync/read/search correctness does
            // not depend on semantic enablement.
            // Apply upserts — store envelope + body, index with body text
            for synced in &batch.upserted {
                let mut normalized_body = synced.body.clone();
                normalized_body.ensure_best_effort_readable();

                let direction = self.classify_direction(&synced.envelope.from.email);
                let mut envelope = self
                    .apply_screener_decision(&synced.envelope, direction)
                    .await?;

                // Derive link-density inputs from the normalized body so the
                // tri-state link indicator and `has:link*` search filters work.
                let link_metrics = crate::links::body_link_metrics(&normalized_body);
                envelope.link_count = link_metrics.link_count;
                envelope.body_word_count = link_metrics.body_word_count;

                self.store
                    .upsert_envelope_with_direction(&envelope, direction)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                if synced.envelope.in_reply_to.is_some() {
                    let resolved = self
                        .store
                        .try_create_reply_pair(&envelope, direction)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                    if !resolved {
                        self.store
                            .enqueue_reply_pair_pending(&envelope)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                    }
                }
                self.store
                    .insert_body(&normalized_body)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                let label_ids = if envelope.label_provider_ids.is_empty() {
                    Vec::new()
                } else {
                    self.store
                        .find_labels_by_provider_ids(account_id, &envelope.label_provider_ids)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?
                };
                self.store
                    .set_message_labels(&envelope.id, &label_ids, EventSource::Sync)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                let reply_later = self
                    .store
                    .is_reply_later(&envelope.id)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                lexical_batch.entries.push(SearchIndexEntry {
                    envelope,
                    body: Some(normalized_body),
                    reply_later,
                });
            }

            for provider_message_id in &batch.deleted_provider_ids {
                if let Some(message_id) = self
                    .store
                    .get_message_id_by_provider_id(account_id, provider_message_id)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?
                {
                    lexical_batch.removed_message_ids.push(message_id);
                }
            }

            if !batch.deleted_provider_ids.is_empty() {
                self.store
                    .delete_messages_by_provider_ids(account_id, &batch.deleted_provider_ids)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
            }

            // Apply label changes from delta sync (previously dead code)
            for change in &batch.label_changes {
                if let Some(message_id) = self
                    .store
                    .get_message_id_by_provider_id(account_id, &change.provider_message_id)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?
                {
                    if !change.added_labels.is_empty() {
                        let add_ids = self
                            .store
                            .find_labels_by_provider_ids(account_id, &change.added_labels)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                        for lid in &add_ids {
                            self.store
                                .add_message_label(&message_id, lid, EventSource::Sync)
                                .await
                                .map_err(|e| MxrError::Store(e.to_string()))?;
                        }
                    }
                    if !change.removed_labels.is_empty() {
                        let rm_ids = self
                            .store
                            .find_labels_by_provider_ids(account_id, &change.removed_labels)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                        for lid in &rm_ids {
                            self.store
                                .remove_message_label(&message_id, lid, EventSource::Sync)
                                .await
                                .map_err(|e| MxrError::Store(e.to_string()))?;
                        }
                    }

                    self.apply_system_label_flag_change(&message_id, change)
                        .await?;

                    if let Some(envelope) = self
                        .store
                        .get_envelope(&message_id)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?
                    {
                        let body = self
                            .store
                            .get_body(&message_id)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                        let reply_later = self
                            .store
                            .is_reply_later(&message_id)
                            .await
                            .map_err(|e| MxrError::Store(e.to_string()))?;
                        lexical_batch.entries.push(SearchIndexEntry {
                            envelope,
                            body,
                            reply_later,
                        });
                    }
                }
            }

            // Commit lexical search for this batch before counts/threading/cursor
            // maintenance. Startup repair can rebuild this index from SQLite if
            // it ever drifts.
            if !lexical_batch.entries.is_empty() || !lexical_batch.removed_message_ids.is_empty() {
                let lexical_queue_started = std::time::Instant::now();
                self.search.apply_batch(lexical_batch).await?;
                tracing::trace!(
                    account = %account_id,
                    elapsed_ms = lexical_queue_started.elapsed().as_secs_f64() * 1000.0,
                    "lexical batch applied"
                );
            }

            // Recalculate label counts every batch (including during backfill)
            self.store
                .recalculate_label_counts(account_id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;

            if !provider.capabilities().sync.native_threading {
                self.rethread_account(account_id).await?;
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
            if provider.capabilities().mutate.labels && junction_count == 0 && message_count > 0 {
                tracing::warn!(
                    message_count,
                    "Junction table empty — resetting sync cursor for full re-sync"
                );
                self.store
                    .set_sync_cursor(account_id, &SyncCursor::empty())
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                continue;
            }

            return Ok(SyncOutcome {
                synced_count,
                upserted_message_ids,
            });
        }
    }

    async fn apply_system_label_flag_change(
        &self,
        message_id: &MessageId,
        change: &LabelChange,
    ) -> Result<(), MxrError> {
        for label in &change.added_labels {
            match label.as_str() {
                "UNREAD" => self
                    .store
                    .set_read(message_id, false, EventSource::Sync)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?,
                "STARRED" => self
                    .store
                    .set_starred(message_id, true, EventSource::Sync)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?,
                _ => {}
            }
        }

        for label in &change.removed_labels {
            match label.as_str() {
                "UNREAD" => self
                    .store
                    .set_read(message_id, true, EventSource::Sync)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?,
                "STARRED" => self
                    .store
                    .set_starred(message_id, false, EventSource::Sync)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?,
                _ => {}
            }
        }

        Ok(())
    }

    async fn rethread_account(&self, account_id: &AccountId) -> Result<(), MxrError> {
        tracing::debug!(account = %account_id, "rethreading account");
        let envelopes = self
            .store
            .list_envelopes_by_account(account_id, 10_000, 0)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        let by_threading_id: HashMap<String, usize> = envelopes
            .iter()
            .enumerate()
            .map(|(index, envelope)| (envelope.id.to_string(), index))
            .collect();

        let threading_input: Vec<ThreadingMessage> = envelopes
            .iter()
            .map(|envelope| ThreadingMessage {
                id: envelope.id.to_string(),
                message_id: envelope.message_id_header.clone(),
                in_reply_to: envelope.in_reply_to.clone(),
                references: envelope.references.clone(),
                date: envelope.date,
                subject: envelope.subject.clone(),
            })
            .collect();

        for thread in thread_messages(&threading_input) {
            let member_indices: Vec<usize> = thread
                .messages
                .iter()
                .filter_map(|message_id| by_threading_id.get(message_id).copied())
                .collect();

            let Some(first_member_index) = member_indices.first() else {
                continue;
            };

            let canonical_thread_index = by_threading_id
                .get(&thread.root_message_id)
                .copied()
                .unwrap_or(*first_member_index);
            let canonical_thread_id = envelopes[canonical_thread_index].thread_id.clone();

            for member_index in member_indices {
                let member = &envelopes[member_index];
                if member.thread_id != canonical_thread_id {
                    let message_id = member.id.clone();
                    self.store
                        .update_message_thread_id(&message_id, &canonical_thread_id)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                }
            }
        }

        Ok(())
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

fn add_route_label(labels: &mut Vec<String>, route_label: Option<&str>) {
    if let Some(route_label) = route_label.filter(|label| !label.trim().is_empty()) {
        add_unique_label(labels, route_label);
    }
}

fn add_unique_label(labels: &mut Vec<String>, label: &str) {
    if !labels
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(label))
    {
        labels.push(label.to_string());
    }
}
