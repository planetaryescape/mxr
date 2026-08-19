use mail_threading::{thread_messages, Message as ThreadingMessage};
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::{MailSyncProvider, MxrError};
use mxr_search::{SearchIndexEntry, SearchServiceHandle, SearchUpdateBatch};
use mxr_store::{ScreenerDisposition, Store, SyncUpsert};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct SyncOutcome {
    pub synced_count: u32,
    pub upserted_message_ids: Vec<MessageId>,
    /// Provider truncated this batch and the next sync call will yield
    /// more data immediately. The daemon uses this to skip its normal
    /// sleep interval and re-poll right away.
    pub has_more: bool,
    /// Threads whose membership or metadata changed during this batch.
    /// Populated by the engine after envelope upsert and any rethread
    /// pass. Empty `Thread.message_ids` denotes a tombstoned thread
    /// (e.g. the loser side of a mail-threading merge).
    pub threads_changed: Vec<Thread>,
}

/// A milestone reached part-way through a sync pass.
///
/// The engine reports these so a caller can show movement during a long
/// backfill; it stays free of IPC and daemon types, and the daemon maps them
/// onto `AccountSyncStatus.progress` and `OperationProgress` events. Each
/// variant is a point the pass has actually reached, not an estimate, and
/// reporting one costs a function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncProgress {
    /// The provider handed back a page, before any of it is stored.
    PageFetched { messages: u32, has_more: bool },
    /// The page's envelopes and bodies are committed to SQLite.
    PageStored { messages: u32 },
    /// The page is committed to the lexical index; the pass is wrapping up.
    PageIndexed { messages: u32 },
    /// The pass is starting over from an empty cursor — the label junction
    /// table was found empty and the account has to be re-paged to rebuild it.
    /// Everything reported for this pass so far describes rows the restart
    /// will report again, so a caller counting messages must discard it.
    Restarted,
}

/// Where a caller receives [`SyncProgress`] milestones.
///
/// `Send + Sync` because the sync future is spawned onto the runtime and holds
/// the sink across await points.
pub type ProgressSink<'a> = &'a (dyn Fn(SyncProgress) + Send + Sync);

/// Discards every milestone. Used by callers that only want the outcome.
fn ignore_progress(_: SyncProgress) {}

/// No-op lookup used when the engine is constructed without an explicit
/// address source. Reports `is_loaded=false` so direction stays `Unknown`
/// rather than being misclassified as inbound.
struct NoopAddressLookup;
impl AccountAddressLookup for NoopAddressLookup {
    fn is_account_address(&self, _account_id: &AccountId, _email: &str) -> bool {
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
    /// address lookup. The sender is only outbound when it is an owned address
    /// of the *receiving* `account_id` — matching against another account's
    /// addresses would misclassify a genuine inbound message. Falls back to
    /// `Unknown` when the lookup hasn't been loaded yet — the doctor
    /// `--rebuild-analytics` command reclassifies.
    fn classify_direction(&self, account_id: &AccountId, from_email: &str) -> MessageDirection {
        if !self.address_lookup.is_loaded() {
            return MessageDirection::Unknown;
        }
        if self
            .address_lookup
            .is_account_address(account_id, from_email)
        {
            MessageDirection::Outbound
        } else {
            MessageDirection::Inbound
        }
    }

    /// Account labels keyed by provider id, read once per batch instead of
    /// once per message.
    async fn label_ids_by_provider_id(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<String, LabelId>, MxrError> {
        let labels = self
            .store
            .list_labels_by_account(account_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?;
        Ok(labels
            .into_iter()
            .map(|label| (label.provider_id, label.id))
            .collect())
    }

    /// Takes the envelope by value: sync routes a whole page at a time and
    /// a clone per message doubles the page's peak memory.
    async fn apply_screener_decision(
        &self,
        envelope: Envelope,
        direction: MessageDirection,
    ) -> Result<Envelope, MxrError> {
        if direction != MessageDirection::Inbound {
            return Ok(envelope);
        }

        let Some(decision) = self
            .store
            .get_screener_decision(&envelope.account_id, &envelope.from.email)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
        else {
            return Ok(envelope);
        };

        let mut routed = envelope;
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

        let direction =
            self.classify_direction(&synced.envelope.account_id, &synced.envelope.from.email);
        let envelope = self
            .apply_screener_decision(synced.envelope.clone(), direction)
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
        self.sync_account_reporting(provider, &ignore_progress)
            .await
    }

    /// Run one sync pass, reporting [`SyncProgress`] milestones to `progress`
    /// as it goes. A pass covers a single provider page; a backfill is many
    /// passes, and the caller accumulates across them.
    pub async fn sync_account_reporting(
        &self,
        provider: &dyn MailSyncProvider,
        progress: ProgressSink<'_>,
    ) -> Result<SyncOutcome, MxrError> {
        let account_id = provider.account_id();
        let mut recovered_expired_cursor = false;
        // Phase F: accumulate thread_ids touched this sync run. Populated
        // from each upserted envelope plus any (old, canonical) pair
        // returned by `rethread_account`. Drained into
        // `outcome.threads_changed` just before returning.
        let mut touched_threads: HashSet<ThreadId> = HashSet::new();
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
            let mut batch = match provider.sync_messages(&cursor).await {
                Ok(batch) => batch,
                Err(MxrError::SyncCursorExpired { reason }) if !recovered_expired_cursor => {
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
            let has_more = batch.has_more;
            progress(SyncProgress::PageFetched {
                messages: synced_count,
                has_more,
            });
            let upserted_message_ids = batch
                .upserted
                .iter()
                .map(|synced| synced.envelope.id.clone())
                .collect::<Vec<_>>();
            let mut lexical_batch = SearchUpdateBatch::default();

            // Core sync guarantee: after this batch, SQLite has the
            // envelope/body pair and Tantivy has the same message's lexical
            // corpus. Semantic chunk prep is intentionally deferred to the
            // daemon's post-sync platform step so mail sync/read/search
            // correctness does not depend on semantic enablement.
            //
            // The writes go through the store's batched path: one
            // transaction per chunk of messages rather than ~8 commits per
            // message through the single writer connection.
            let label_ids_by_provider_id = if batch.upserted.is_empty() {
                HashMap::new()
            } else {
                self.label_ids_by_provider_id(account_id).await?
            };
            // Consumed, not borrowed: holding the provider's page alongside
            // the upserts would keep two copies of every body alive.
            let mut upserts = Vec::with_capacity(batch.upserted.len());
            for synced in std::mem::take(&mut batch.upserted) {
                let mut body = synced.body;
                body.ensure_best_effort_readable();

                let direction = self
                    .classify_direction(&synced.envelope.account_id, &synced.envelope.from.email);
                let mut envelope = self
                    .apply_screener_decision(synced.envelope, direction)
                    .await?;

                // Derive link-density inputs from the normalized body so the
                // tri-state link indicator and `has:link*` search filters work.
                let link_metrics = crate::links::body_link_metrics(&body);
                envelope.link_count = link_metrics.link_count;
                envelope.body_word_count = link_metrics.body_word_count;

                // Deduplicated: `message_labels` has a composite primary key,
                // and a provider is free to repeat a label id.
                let mut label_ids: Vec<LabelId> = Vec::new();
                for provider_id in &envelope.label_provider_ids {
                    if let Some(label_id) = label_ids_by_provider_id.get(provider_id) {
                        if !label_ids.contains(label_id) {
                            label_ids.push(label_id.clone());
                        }
                    }
                }

                // Phase F: record the touched thread so the outcome's
                // `threads_changed` list reflects this upsert.
                touched_threads.insert(envelope.thread_id.clone());
                upserts.push(SyncUpsert {
                    envelope,
                    direction,
                    body,
                    label_ids,
                });
            }

            self.store
                .apply_sync_upserts(&upserts)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            progress(SyncProgress::PageStored {
                messages: synced_count,
            });

            // Reply pairs resolve against committed rows, so they run after
            // the batch: a reply whose parent arrived in the same batch now
            // pairs up directly instead of going through the pending queue.
            for upsert in &upserts {
                if upsert.envelope.in_reply_to.is_none() {
                    continue;
                }
                let resolved = self
                    .store
                    .try_create_reply_pair(&upsert.envelope, upsert.direction)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                if !resolved {
                    self.store
                        .enqueue_reply_pair_pending(&upsert.envelope)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                }
            }

            let reply_later_ids = self
                .store
                .reply_later_message_ids(&upserted_message_ids)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            for upsert in upserts {
                lexical_batch.entries.push(SearchIndexEntry {
                    reply_later: reply_later_ids.contains(&upsert.envelope.id),
                    envelope: upsert.envelope,
                    body: Some(upsert.body),
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
            progress(SyncProgress::PageIndexed {
                messages: synced_count,
            });

            // Recalculate label counts every batch (including during backfill)
            self.store
                .recalculate_label_counts(account_id)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;

            if !provider.capabilities().sync.native_threading {
                let rethread_touched = self.rethread_account(account_id).await?;
                touched_threads.extend(rethread_touched);
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
                progress(SyncProgress::Restarted);
                continue;
            }

            // Phase F: hydrate `threads_changed` from the touched set.
            // Live threads (i.e. those with at least one member row) come
            // back from `get_threads_batch`; ids present in the touched
            // set but absent from the result are merged-away thread
            // tombstones — we synthesise an empty-`message_ids` Thread
            // so clients can drop their cached metadata.
            let touched_vec: Vec<ThreadId> = touched_threads.iter().cloned().collect();
            let live_threads = self
                .store
                .get_threads_batch(&touched_vec)
                .await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            let live_ids: HashSet<ThreadId> = live_threads.iter().map(|t| t.id.clone()).collect();
            let mut threads_changed = live_threads;
            for id in &touched_vec {
                if !live_ids.contains(id) {
                    threads_changed.push(Thread {
                        id: id.clone(),
                        account_id: account_id.clone(),
                        subject: String::new(),
                        participants: Vec::new(),
                        message_count: 0,
                        unread_count: 0,
                        latest_date: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
                        snippet: String::new(),
                        message_ids: Vec::new(),
                    });
                }
            }

            return Ok(SyncOutcome {
                synced_count,
                upserted_message_ids,
                has_more,
                threads_changed,
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

    async fn rethread_account(
        &self,
        account_id: &AccountId,
    ) -> Result<HashSet<ThreadId>, MxrError> {
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

        // Phase F: track every (old, canonical) thread_id pair that a
        // mail-threading merge touches so the sync outcome can emit a
        // `threads_changed` slice. The loser's id stays in the set
        // even after every member is reassigned — `get_threads_batch`
        // returns nothing for it, and the caller synthesises an
        // empty-`message_ids` tombstone Thread.
        let mut touched: HashSet<ThreadId> = HashSet::new();

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
                    touched.insert(member.thread_id.clone());
                    touched.insert(canonical_thread_id.clone());
                    let message_id = member.id.clone();
                    self.store
                        .update_message_thread_id(&message_id, &canonical_thread_id)
                        .await
                        .map_err(|e| MxrError::Store(e.to_string()))?;
                }
            }
        }

        Ok(touched)
    }

    /// Read body from store. Bodies are always available after sync.
    pub async fn get_body(&self, message_id: &MessageId) -> Result<MessageBody, MxrError> {
        self.store
            .get_body(message_id)
            .await
            .map_err(|e| MxrError::Store(e.to_string()))?
            .ok_or_else(|| MxrError::NotFound(format!("Body for message {message_id}")))
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
