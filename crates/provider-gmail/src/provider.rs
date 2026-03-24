use crate::mxr_core::{
    AccountId, Address, Draft, Label, LabelChange, LabelId, LabelKind, MailSendProvider,
    MailSyncProvider, MxrError, SendReceipt, SyncBatch, SyncCapabilities, SyncCursor,
};
use async_trait::async_trait;
use tracing::{debug, warn};

use crate::mxr_core::types::SyncedMessage;
use crate::mxr_provider_gmail::client::{GmailApi, GmailClient, MessageFormat};
use crate::mxr_provider_gmail::error::GmailError;
use crate::mxr_provider_gmail::parse::{extract_message_body, gmail_message_to_envelope};
use crate::mxr_provider_gmail::send;

pub struct GmailProvider {
    account_id: AccountId,
    client: Box<dyn GmailApi>,
}

impl GmailProvider {
    pub fn new(account_id: AccountId, client: GmailClient) -> Self {
        Self {
            account_id,
            client: Box::new(client),
        }
    }

    #[cfg(test)]
    fn with_api(account_id: AccountId, client: Box<dyn GmailApi>) -> Self {
        Self { account_id, client }
    }

    fn map_label(&self, gl: crate::mxr_provider_gmail::types::GmailLabel) -> Label {
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
                        all_messages.push(SyncedMessage {
                            envelope: env,
                            body,
                        });
                    }
                    Err(e) => warn!(msg_id = %msg.id, error = %e, "Failed to parse message"),
                }
            }

            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => {
                    page_token = None;
                    break;
                }
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
                    synced.push(SyncedMessage {
                        envelope: env,
                        body,
                    });
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

        tracing::info!(fetched = synced.len(), has_more, "Backfill batch complete");

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
            let resp = match self
                .client
                .list_history(history_id, page_token.as_deref())
                .await
            {
                Ok(resp) => resp,
                Err(GmailError::NotFound(body)) => {
                    warn!(
                        history_id,
                        account = %self.account_id,
                        error = %body,
                        "Gmail history cursor stale, falling back to initial sync"
                    );
                    return self.initial_sync().await;
                }
                Err(error) => return Err(MxrError::from(error)),
            };

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
                        synced.push(SyncedMessage {
                            envelope: env,
                            body,
                        });
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

    async fn authenticate(&mut self) -> crate::mxr_core::provider::Result<()> {
        // Auth is managed by GmailAuth externally before constructing the provider
        Ok(())
    }

    async fn refresh_auth(&mut self) -> crate::mxr_core::provider::Result<()> {
        // Token refresh is handled automatically by yup-oauth2
        Ok(())
    }

    async fn sync_labels(&self) -> crate::mxr_core::provider::Result<Vec<Label>> {
        let resp = self.client.list_labels().await.map_err(MxrError::from)?;

        let gmail_labels = resp.labels.unwrap_or_default();
        let mut labels = Vec::with_capacity(gmail_labels.len());

        for gl in gmail_labels {
            labels.push(self.map_label(gl));
        }

        Ok(labels)
    }

    async fn sync_messages(
        &self,
        cursor: &SyncCursor,
    ) -> crate::mxr_core::provider::Result<SyncBatch> {
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
    ) -> crate::mxr_core::provider::Result<Vec<u8>> {
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
    ) -> crate::mxr_core::provider::Result<()> {
        let add_refs: Vec<&str> = add.iter().map(|s| s.as_str()).collect();
        let remove_refs: Vec<&str> = remove.iter().map(|s| s.as_str()).collect();
        self.client
            .modify_message(provider_message_id, &add_refs, &remove_refs)
            .await
            .map_err(MxrError::from)
    }

    async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> crate::mxr_core::provider::Result<Label> {
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
    ) -> crate::mxr_core::provider::Result<Label> {
        let label = self
            .client
            .rename_label(provider_label_id, new_name)
            .await
            .map_err(MxrError::from)?;
        Ok(self.map_label(label))
    }

    async fn delete_label(&self, provider_label_id: &str) -> crate::mxr_core::provider::Result<()> {
        self.client
            .delete_label(provider_label_id)
            .await
            .map_err(MxrError::from)
    }

    async fn trash(&self, provider_message_id: &str) -> crate::mxr_core::provider::Result<()> {
        self.client
            .trash_message(provider_message_id)
            .await
            .map_err(MxrError::from)
    }

    async fn set_read(
        &self,
        provider_message_id: &str,
        read: bool,
    ) -> crate::mxr_core::provider::Result<()> {
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
    ) -> crate::mxr_core::provider::Result<()> {
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

    async fn search_remote(&self, query: &str) -> crate::mxr_core::provider::Result<Vec<String>> {
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

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
    ) -> crate::mxr_core::provider::Result<SendReceipt> {
        let rfc2822 =
            send::build_rfc2822(draft, from).map_err(|e| MxrError::Provider(e.to_string()))?;
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
    ) -> crate::mxr_core::provider::Result<Option<String>> {
        let rfc2822 =
            send::build_rfc2822(draft, from).map_err(|e| MxrError::Provider(e.to_string()))?;
        let encoded = send::encode_for_gmail(&rfc2822);

        let draft_id = self
            .client
            .create_draft(&encoded)
            .await
            .map_err(MxrError::from)?;

        Ok(Some(draft_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_provider_gmail::error::GmailError;
    use crate::mxr_provider_gmail::types::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;
    struct MockGmailApi {
        messages: HashMap<String, GmailMessage>,
        labels: Vec<GmailLabel>,
        modified: Mutex<Vec<String>>,
        stale_history: bool,
    }

    #[async_trait]
    impl GmailApi for MockGmailApi {
        async fn list_messages(
            &self,
            _query: Option<&str>,
            page_token: Option<&str>,
            _max_results: u32,
        ) -> Result<GmailListResponse, GmailError> {
            Ok(match page_token {
                Some("page-2") => GmailListResponse {
                    messages: Some(vec![GmailMessageRef {
                        id: "msg-backfill".into(),
                        thread_id: "thread-backfill".into(),
                    }]),
                    next_page_token: None,
                    result_size_estimate: Some(3),
                },
                _ => GmailListResponse {
                    messages: Some(vec![
                        GmailMessageRef {
                            id: "msg-1".into(),
                            thread_id: "thread-1".into(),
                        },
                        GmailMessageRef {
                            id: "msg-attach".into(),
                            thread_id: "thread-attach".into(),
                        },
                    ]),
                    next_page_token: Some("page-2".into()),
                    result_size_estimate: Some(3),
                },
            })
        }

        async fn batch_get_messages(
            &self,
            message_ids: &[String],
            _format: MessageFormat,
        ) -> Result<Vec<GmailMessage>, GmailError> {
            Ok(message_ids
                .iter()
                .filter_map(|id| self.messages.get(id).cloned())
                .collect())
        }

        async fn list_history(
            &self,
            _start_history_id: u64,
            _page_token: Option<&str>,
        ) -> Result<GmailHistoryResponse, GmailError> {
            if self.stale_history {
                return Err(GmailError::NotFound(
                    json!({
                        "error": {
                            "code": 404,
                            "message": "Requested entity was not found.",
                            "errors": [
                                {
                                    "message": "Requested entity was not found.",
                                    "domain": "global",
                                    "reason": "notFound"
                                }
                            ],
                            "status": "NOT_FOUND"
                        }
                    })
                    .to_string(),
                ));
            }

            Ok(GmailHistoryResponse {
                history: Some(vec![GmailHistoryRecord {
                    id: "23".into(),
                    messages: None,
                    messages_added: Some(vec![GmailHistoryMessageAdded {
                        message: GmailMessageRef {
                            id: "msg-3".into(),
                            thread_id: "thread-3".into(),
                        },
                    }]),
                    messages_deleted: Some(vec![GmailHistoryMessageDeleted {
                        message: GmailMessageRef {
                            id: "msg-1".into(),
                            thread_id: "thread-1".into(),
                        },
                    }]),
                    labels_added: Some(vec![GmailHistoryLabelAdded {
                        message: GmailMessageRef {
                            id: "msg-attach".into(),
                            thread_id: "thread-attach".into(),
                        },
                        label_ids: Some(vec!["STARRED".into()]),
                    }]),
                    labels_removed: None,
                }]),
                next_page_token: None,
                history_id: Some("23".into()),
            })
        }

        async fn modify_message(
            &self,
            message_id: &str,
            _add_labels: &[&str],
            _remove_labels: &[&str],
        ) -> Result<(), GmailError> {
            self.modified.lock().unwrap().push(message_id.to_string());
            Ok(())
        }

        async fn trash_message(&self, message_id: &str) -> Result<(), GmailError> {
            self.modified
                .lock()
                .unwrap()
                .push(format!("trash:{message_id}"));
            Ok(())
        }

        async fn send_message(
            &self,
            _raw_base64url: &str,
        ) -> Result<serde_json::Value, GmailError> {
            Ok(json!({"id": "sent-1"}))
        }

        async fn get_attachment(
            &self,
            _message_id: &str,
            _attachment_id: &str,
        ) -> Result<Vec<u8>, GmailError> {
            Ok(b"Hello".to_vec())
        }

        async fn create_draft(&self, _raw_base64url: &str) -> Result<String, GmailError> {
            Ok("draft-1".into())
        }

        async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
            Ok(GmailLabelsResponse {
                labels: Some(self.labels.clone()),
            })
        }

        async fn create_label(
            &self,
            name: &str,
            color: Option<&str>,
        ) -> Result<GmailLabel, GmailError> {
            Ok(GmailLabel {
                id: "Label_2".into(),
                name: name.into(),
                label_type: Some("user".into()),
                messages_total: Some(0),
                messages_unread: Some(0),
                color: color.map(|color| GmailLabelColor {
                    text_color: Some("#000000".into()),
                    background_color: Some(color.into()),
                }),
            })
        }

        async fn rename_label(
            &self,
            label_id: &str,
            new_name: &str,
        ) -> Result<GmailLabel, GmailError> {
            Ok(GmailLabel {
                id: label_id.into(),
                name: new_name.into(),
                label_type: Some("user".into()),
                messages_total: Some(0),
                messages_unread: Some(0),
                color: None,
            })
        }

        async fn delete_label(&self, _label_id: &str) -> Result<(), GmailError> {
            Ok(())
        }
    }

    fn gmail_provider() -> GmailProvider {
        gmail_provider_with_stale_history(false)
    }

    fn gmail_provider_with_stale_history(stale_history: bool) -> GmailProvider {
        let mut messages = HashMap::new();
        for message in [
            serde_json::from_value::<GmailMessage>(gmail_message("msg-1", "thread-1", "Welcome"))
                .unwrap(),
            serde_json::from_value::<GmailMessage>(gmail_attachment_message()).unwrap(),
            serde_json::from_value::<GmailMessage>(gmail_message(
                "msg-3",
                "thread-3",
                "Delta message",
            ))
            .unwrap(),
            serde_json::from_value::<GmailMessage>(gmail_message(
                "msg-backfill",
                "thread-backfill",
                "Backfill message",
            ))
            .unwrap(),
        ] {
            messages.insert(message.id.clone(), message);
        }

        GmailProvider::with_api(
            AccountId::new(),
            Box::new(MockGmailApi {
                messages,
                labels: vec![
                    GmailLabel {
                        id: "INBOX".into(),
                        name: "INBOX".into(),
                        label_type: Some("system".into()),
                        messages_total: Some(2),
                        messages_unread: Some(1),
                        color: None,
                    },
                    GmailLabel {
                        id: "Label_1".into(),
                        name: "Projects".into(),
                        label_type: Some("user".into()),
                        messages_total: Some(1),
                        messages_unread: Some(0),
                        color: None,
                    },
                ],
                modified: Mutex::new(Vec::new()),
                stale_history,
            }),
        )
    }

    fn gmail_message(id: &str, thread_id: &str, subject: &str) -> serde_json::Value {
        json!({
            "id": id,
            "threadId": thread_id,
            "labelIds": ["INBOX"],
            "snippet": format!("Snippet for {subject}"),
            "historyId": "22",
            "internalDate": "1710495000000",
            "sizeEstimate": 1024,
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "From", "value": "Alice Example <alice@example.com>"},
                    {"name": "To", "value": "Bob Example <bob@example.com>"},
                    {"name": "Subject", "value": subject},
                    {"name": "Date", "value": "Fri, 15 Mar 2024 09:30:00 +0000"},
                    {"name": "Message-ID", "value": format!("<{id}@example.com>")}
                ],
                "parts": [
                    {
                        "mimeType": "text/plain",
                        "body": {"size": 12, "data": "SGVsbG8gd29ybGQ"}
                    },
                    {
                        "mimeType": "text/html",
                        "body": {"size": 33, "data": "PHA-SGVsbG8gd29ybGQ8L3A-"}
                    }
                ]
            }
        })
    }

    fn gmail_attachment_message() -> serde_json::Value {
        json!({
            "id": "msg-attach",
            "threadId": "thread-attach",
            "labelIds": ["INBOX", "UNREAD"],
            "snippet": "Attachment snippet",
            "historyId": "21",
            "internalDate": "1710495000000",
            "sizeEstimate": 2048,
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "From", "value": "Calendar Bot <calendar@example.com>"},
                    {"name": "To", "value": "Bob Example <bob@example.com>"},
                    {"name": "Subject", "value": "Calendar invite"},
                    {"name": "Date", "value": "Fri, 15 Mar 2024 09:30:00 +0000"},
                    {"name": "Message-ID", "value": "<msg-attach@example.com>"},
                    {"name": "List-Unsubscribe", "value": "<https://example.com/unsubscribe>"},
                    {"name": "Authentication-Results", "value": "mx.example.net; dkim=pass"},
                    {"name": "Content-Language", "value": "en"}
                ],
                "parts": [
                    {
                        "mimeType": "text/plain",
                        "body": {"size": 16, "data": "QXR0YWNobWVudCBib2R5"}
                    },
                    {
                        "mimeType": "application/pdf",
                        "filename": "report.pdf",
                        "body": {"attachmentId": "att-1", "size": 5}
                    }
                ]
            }
        })
    }

    #[tokio::test]
    async fn gmail_provider_passes_sync_and_send_conformance() {
        let provider = gmail_provider();
        crate::mxr_provider_fake::conformance::run_sync_conformance(&provider).await;
        crate::mxr_provider_fake::conformance::run_send_conformance(&provider).await;
    }

    #[tokio::test]
    async fn gmail_delta_sync_tracks_history_changes() {
        let provider = gmail_provider();
        let batch = provider
            .sync_messages(&SyncCursor::Gmail { history_id: 22 })
            .await
            .unwrap();

        assert_eq!(batch.deleted_provider_ids, vec!["msg-1"]);
        assert_eq!(batch.label_changes.len(), 1);
        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].envelope.provider_id, "msg-3");
        assert!(matches!(
            batch.next_cursor,
            SyncCursor::Gmail { history_id: 23 }
        ));
    }

    #[tokio::test]
    async fn gmail_delta_sync_recovers_from_stale_history_cursor() {
        let provider = gmail_provider_with_stale_history(true);
        let batch = provider
            .sync_messages(&SyncCursor::Gmail {
                history_id: 27_672_073,
            })
            .await
            .unwrap();

        assert_eq!(batch.upserted.len(), 3);
        assert!(batch.deleted_provider_ids.is_empty());
        assert!(batch.label_changes.is_empty());
        assert!(matches!(
            batch.next_cursor,
            SyncCursor::Gmail { history_id: 22 }
        ));
    }
}
