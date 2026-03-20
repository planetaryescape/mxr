use async_trait::async_trait;
use mxr_core::{
    AccountId, Address, Draft, Label, LabelChange, LabelId, LabelKind, MailSendProvider,
    MailSyncProvider, MxrError, SendReceipt, SyncBatch, SyncCapabilities, SyncCursor,
};
use tracing::{debug, warn};

use crate::client::{GmailClient, MessageFormat};
use crate::parse::{extract_message_body, gmail_message_to_envelope};
use crate::send;
use mxr_core::types::SyncedMessage;

pub struct GmailProvider {
    account_id: AccountId,
    client: GmailClient,
}

impl GmailProvider {
    pub fn new(account_id: AccountId, client: GmailClient) -> Self {
        Self { account_id, client }
    }

    fn map_label(&self, gl: crate::types::GmailLabel) -> Label {
        let kind = match gl.label_type.as_deref() {
            Some("system") => LabelKind::System,
            _ => LabelKind::User,
        };

        let color = gl.color.as_ref().and_then(|c| c.background_color.clone());

        Label {
            id: LabelId::from_provider_id("gmail", &gl.id),
            account_id: self.account_id.clone(),
            name: gl.name,
            kind,
            color,
            provider_id: gl.id,
            unread_count: gl.messages_unread.unwrap_or(0),
            total_count: gl.messages_total.unwrap_or(0),
        }
    }

    async fn initial_sync(&self) -> Result<SyncBatch, MxrError> {
        debug!("Starting initial sync for account {}", self.account_id);

        let mut all_messages = Vec::new();
        let mut page_token: Option<String> = None;
        let mut latest_history_id: Option<u64> = None;
        // Fetch first 200 messages for fast time-to-first-content.
        // The daemon stores a GmailBackfill cursor with the page_token,
        // and the sync loop continues fetching remaining pages in the
        // background every 2s until all messages are synced.
        const MAX_INITIAL_MESSAGES: usize = 200;

        loop {
            let batch_size = (MAX_INITIAL_MESSAGES - all_messages.len()).min(100) as u32;
            if batch_size == 0 {
                tracing::info!(
                    "Initial sync: fetched {MAX_INITIAL_MESSAGES} messages, \
                     remaining pages will be backfilled in background"
                );
                break;
            }

            let resp = self
                .client
                .list_messages(None, page_token.as_deref(), batch_size)
                .await
                .map_err(MxrError::from)?;

            let refs = resp.messages.unwrap_or_default();
            if refs.is_empty() {
                break;
            }

            let ids: Vec<String> = refs.iter().map(|r| r.id.clone()).collect();
            let messages = self
                .client
                .batch_get_messages(&ids, MessageFormat::Full)
                .await
                .map_err(MxrError::from)?;

            for msg in &messages {
                if let Some(ref hid) = msg.history_id {
                    if let Ok(h) = hid.parse::<u64>() {
                        latest_history_id =
                            Some(latest_history_id.map_or(h, |cur: u64| cur.max(h)));
                    }
                }
                match gmail_message_to_envelope(msg, &self.account_id) {
                    Ok(env) => {
                        let body = extract_message_body(msg);
                        all_messages.push(SyncedMessage { envelope: env, body });
                    }
                    Err(e) => warn!(msg_id = %msg.id, error = %e, "Failed to parse message"),
                }
            }

            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        let next_cursor = match (latest_history_id, &page_token) {
            (Some(hid), Some(token)) => {
                tracing::info!(
                    history_id = hid,
                    "Initial sync producing GmailBackfill cursor for background sync"
                );
                SyncCursor::GmailBackfill {
                    history_id: hid,
                    page_token: token.clone(),
                }
            }
            (Some(hid), None) => {
                tracing::info!(
                    history_id = hid,
                    total = all_messages.len(),
                    "Initial sync complete — all messages fetched, delta-ready"
                );
                SyncCursor::Gmail { history_id: hid }
            }
            _ => SyncCursor::Initial,
        };

        Ok(SyncBatch {
            upserted: all_messages,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
        })
    }

    async fn backfill_sync(
        &self,
        history_id: u64,
        page_token: &str,
    ) -> Result<SyncBatch, MxrError> {
        tracing::info!(
            "Backfill sync: fetching next page for account {}",
            self.account_id,
        );

        const BACKFILL_BATCH: u32 = 100;
        let resp = self
            .client
            .list_messages(None, Some(page_token), BACKFILL_BATCH)
            .await
            .map_err(MxrError::from)?;

        let refs = resp.messages.unwrap_or_default();
        if refs.is_empty() {
            return Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Gmail { history_id },
            });
        }

        let ids: Vec<String> = refs.iter().map(|r| r.id.clone()).collect();
        debug!("Backfill: fetching {} messages (full)", ids.len());
        let messages = self
            .client
            .batch_get_messages(&ids, MessageFormat::Full)
            .await
            .map_err(MxrError::from)?;

        let mut synced = Vec::new();
        for msg in &messages {
            match gmail_message_to_envelope(msg, &self.account_id) {
                Ok(env) => {
                    let body = extract_message_body(msg);
                    synced.push(SyncedMessage { envelope: env, body });
                }
                Err(e) => {
                    warn!(msg_id = %msg.id, error = %e, "Failed to parse message in backfill")
                }
            }
        }

        let has_more = resp.next_page_token.is_some();
        let next_cursor = match resp.next_page_token {
            Some(token) => SyncCursor::GmailBackfill {
                history_id,
                page_token: token,
            },
            None => SyncCursor::Gmail { history_id },
        };

        tracing::info!(
            fetched = synced.len(),
            has_more,
            "Backfill batch complete"
        );

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
        })
    }

    async fn delta_sync(&self, history_id: u64) -> Result<SyncBatch, MxrError> {
        debug!(
            history_id,
            "Starting delta sync for account {}", self.account_id
        );

        let mut upserted_ids = std::collections::HashSet::new();
        let mut deleted_ids = Vec::new();
        let mut label_changes = Vec::new();
        let mut latest_history_id = history_id;
        let mut page_token: Option<String> = None;

        loop {
            let resp = self
                .client
                .list_history(history_id, page_token.as_deref())
                .await
                .map_err(MxrError::from)?;

            if let Some(ref hid) = resp.history_id {
                if let Ok(h) = hid.parse::<u64>() {
                    latest_history_id = latest_history_id.max(h);
                }
            }

            let records = resp.history.unwrap_or_default();
            for record in records {
                // Messages added
                if let Some(added) = record.messages_added {
                    for a in added {
                        upserted_ids.insert(a.message.id);
                    }
                }

                // Messages deleted
                if let Some(deleted) = record.messages_deleted {
                    for d in deleted {
                        deleted_ids.push(d.message.id);
                    }
                }

                // Label additions
                if let Some(label_added) = record.labels_added {
                    for la in label_added {
                        label_changes.push(LabelChange {
                            provider_message_id: la.message.id,
                            added_labels: la.label_ids.unwrap_or_default(),
                            removed_labels: vec![],
                        });
                    }
                }

                // Label removals
                if let Some(label_removed) = record.labels_removed {
                    for lr in label_removed {
                        label_changes.push(LabelChange {
                            provider_message_id: lr.message.id,
                            added_labels: vec![],
                            removed_labels: lr.label_ids.unwrap_or_default(),
                        });
                    }
                }
            }

            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        // Fetch full messages for new/changed messages
        let ids_to_fetch: Vec<String> = upserted_ids.into_iter().collect();
        let mut synced = Vec::new();

        if !ids_to_fetch.is_empty() {
            let messages = self
                .client
                .batch_get_messages(&ids_to_fetch, MessageFormat::Full)
                .await
                .map_err(MxrError::from)?;

            for msg in &messages {
                match gmail_message_to_envelope(msg, &self.account_id) {
                    Ok(env) => {
                        let body = extract_message_body(msg);
                        synced.push(SyncedMessage { envelope: env, body });
                    }
                    Err(e) => warn!(msg_id = %msg.id, error = %e, "Failed to parse message"),
                }
            }
        }

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: deleted_ids,
            label_changes,
            next_cursor: SyncCursor::Gmail {
                history_id: latest_history_id,
            },
        })
    }
}

#[async_trait]
impl MailSyncProvider for GmailProvider {
    fn name(&self) -> &str {
        "gmail"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: true,
            server_search: true,
            delta_sync: true,
            push: false, // push via pub/sub not yet implemented
            batch_operations: true,
            native_thread_ids: true,
        }
    }

    async fn authenticate(&mut self) -> mxr_core::provider::Result<()> {
        // Auth is managed by GmailAuth externally before constructing the provider
        Ok(())
    }

    async fn refresh_auth(&mut self) -> mxr_core::provider::Result<()> {
        // Token refresh is handled automatically by yup-oauth2
        Ok(())
    }

    async fn sync_labels(&self) -> mxr_core::provider::Result<Vec<Label>> {
        let resp = self.client.list_labels().await.map_err(MxrError::from)?;

        let gmail_labels = resp.labels.unwrap_or_default();
        let mut labels = Vec::with_capacity(gmail_labels.len());

        for gl in gmail_labels {
            labels.push(self.map_label(gl));
        }

        Ok(labels)
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> mxr_core::provider::Result<SyncBatch> {
        match cursor {
            SyncCursor::Initial => self.initial_sync().await,
            SyncCursor::Gmail { history_id } => self.delta_sync(*history_id).await,
            SyncCursor::GmailBackfill {
                history_id,
                page_token,
            } => self.backfill_sync(*history_id, page_token).await,
            other => Err(MxrError::Provider(format!(
                "Gmail provider received incompatible cursor: {other:?}"
            ))),
        }
    }

    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> mxr_core::provider::Result<Vec<u8>> {
        self.client
            .get_attachment(provider_message_id, provider_attachment_id)
            .await
            .map_err(MxrError::from)
    }

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> mxr_core::provider::Result<()> {
        let add_refs: Vec<&str> = add.iter().map(|s| s.as_str()).collect();
        let remove_refs: Vec<&str> = remove.iter().map(|s| s.as_str()).collect();
        self.client
            .modify_message(provider_message_id, &add_refs, &remove_refs)
            .await
            .map_err(MxrError::from)
    }

    async fn create_label(&self, name: &str, color: Option<&str>) -> mxr_core::provider::Result<Label> {
        let label = self
            .client
            .create_label(name, color)
            .await
            .map_err(MxrError::from)?;
        Ok(self.map_label(label))
    }

    async fn rename_label(
        &self,
        provider_label_id: &str,
        new_name: &str,
    ) -> mxr_core::provider::Result<Label> {
        let label = self
            .client
            .rename_label(provider_label_id, new_name)
            .await
            .map_err(MxrError::from)?;
        Ok(self.map_label(label))
    }

    async fn delete_label(&self, provider_label_id: &str) -> mxr_core::provider::Result<()> {
        self.client
            .delete_label(provider_label_id)
            .await
            .map_err(MxrError::from)
    }

    async fn trash(&self, provider_message_id: &str) -> mxr_core::provider::Result<()> {
        self.client
            .trash_message(provider_message_id)
            .await
            .map_err(MxrError::from)
    }

    async fn set_read(
        &self,
        provider_message_id: &str,
        read: bool,
    ) -> mxr_core::provider::Result<()> {
        if read {
            self.client
                .modify_message(provider_message_id, &[], &["UNREAD"])
                .await
                .map_err(MxrError::from)
        } else {
            self.client
                .modify_message(provider_message_id, &["UNREAD"], &[])
                .await
                .map_err(MxrError::from)
        }
    }

    async fn set_starred(
        &self,
        provider_message_id: &str,
        starred: bool,
    ) -> mxr_core::provider::Result<()> {
        if starred {
            self.client
                .modify_message(provider_message_id, &["STARRED"], &[])
                .await
                .map_err(MxrError::from)
        } else {
            self.client
                .modify_message(provider_message_id, &[], &["STARRED"])
                .await
                .map_err(MxrError::from)
        }
    }

    async fn search_remote(&self, query: &str) -> mxr_core::provider::Result<Vec<String>> {
        let resp = self
            .client
            .list_messages(Some(query), None, 100)
            .await
            .map_err(MxrError::from)?;

        let ids = resp
            .messages
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.id)
            .collect();

        Ok(ids)
    }
}

#[async_trait]
impl MailSendProvider for GmailProvider {
    fn name(&self) -> &str {
        "gmail"
    }

    async fn send(&self, draft: &Draft, from: &Address) -> mxr_core::provider::Result<SendReceipt> {
        let rfc2822 = send::build_rfc2822(draft, &from.email)
            .map_err(|e| MxrError::Provider(e.to_string()))?;
        let encoded = send::encode_for_gmail(&rfc2822);

        let result = self
            .client
            .send_message(&encoded)
            .await
            .map_err(MxrError::from)?;

        let message_id = result["id"].as_str().map(|s| s.to_string());

        Ok(SendReceipt {
            provider_message_id: message_id,
            sent_at: chrono::Utc::now(),
        })
    }

    async fn save_draft(
        &self,
        draft: &Draft,
        from: &Address,
    ) -> mxr_core::provider::Result<Option<String>> {
        let rfc2822 = send::build_rfc2822(draft, &from.email)
            .map_err(|e| MxrError::Provider(e.to_string()))?;
        let encoded = send::encode_for_gmail(&rfc2822);

        let draft_id = self
            .client
            .create_draft(&encoded)
            .await
            .map_err(MxrError::from)?;

        Ok(Some(draft_id))
    }
}
