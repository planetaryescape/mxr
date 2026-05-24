#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use mxr_core::{
    AccountId, Address, Draft, Label, LabelChange, LabelId, LabelKind, MailSendProvider,
    MailSyncProvider, MutateCaps, MxrError, PushCaps, Role, SearchCaps, SendReceipt, SyncBatch,
    SyncCapabilities, SyncCaps, SyncCursor,
};
use tracing::{debug, warn};

use crate::client::{GmailApi, GmailClient, MessageFormat};
use crate::cursor::GmailCursor;
use crate::error::GmailError;
use crate::parse::{extract_message_body_for_account, gmail_message_to_envelope};
use crate::send;
use mxr_core::types::SyncedMessage;

#[cfg(test)]
#[derive(Clone)]
struct ParseObserver {
    current: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    max: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    delay: std::time::Duration,
}

#[cfg(test)]
impl ParseObserver {
    fn new(delay: std::time::Duration) -> Self {
        Self {
            current: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            delay,
        }
    }

    fn max_concurrency(&self) -> usize {
        self.max.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn enter(&self) -> ParseObserverGuard {
        let current = self
            .current
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;

        loop {
            let max = self.max.load(std::sync::atomic::Ordering::SeqCst);
            if current <= max {
                break;
            }
            if self
                .max
                .compare_exchange(
                    max,
                    current,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                )
                .is_ok()
            {
                break;
            }
        }

        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }

        ParseObserverGuard {
            observer: self.clone(),
        }
    }
}

#[cfg(test)]
struct ParseObserverGuard {
    observer: ParseObserver,
}

#[cfg(test)]
impl Drop for ParseObserverGuard {
    fn drop(&mut self) {
        self.observer
            .current
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

const GMAIL_PARSE_FANOUT_THRESHOLD: usize = 64;
const GMAIL_PARSE_FANOUT_MAX_CONCURRENCY: usize = 8;

pub struct GmailProvider {
    account_id: AccountId,
    client: Box<dyn GmailApi>,
    #[cfg(test)]
    parse_observer: Option<ParseObserver>,
}

impl GmailProvider {
    pub fn new(account_id: AccountId, client: GmailClient) -> Self {
        Self {
            account_id,
            client: Box::new(client),
            #[cfg(test)]
            parse_observer: None,
        }
    }

    #[cfg(test)]
    fn with_api(account_id: AccountId, client: Box<dyn GmailApi>) -> Self {
        Self {
            account_id,
            client,
            parse_observer: None,
        }
    }

    #[cfg(test)]
    fn with_api_and_parse_observer(
        account_id: AccountId,
        client: Box<dyn GmailApi>,
        parse_observer: ParseObserver,
    ) -> Self {
        Self {
            account_id,
            client,
            parse_observer: Some(parse_observer),
        }
    }

    fn map_label(&self, gl: crate::types::GmailLabel) -> Label {
        let kind = match gl.label_type.as_deref() {
            Some("system") => LabelKind::System,
            _ => LabelKind::User,
        };

        // Gmail system label ids are stable strings (RFC-style). Map the
        // well-known ones to MSP roles so clients get typed semantics
        // without parsing names. Unknown system labels (e.g. CATEGORY_*)
        // stay role-less.
        let role = match gl.id.as_str() {
            "INBOX" => Some(Role::Inbox),
            "SENT" => Some(Role::Sent),
            "DRAFT" => Some(Role::Drafts),
            "TRASH" => Some(Role::Trash),
            "SPAM" => Some(Role::Spam),
            "IMPORTANT" => Some(Role::Important),
            "STARRED" => Some(Role::Starred),
            _ => None,
        };

        let color = gl.color.as_ref().and_then(|c| c.background_color.clone());

        Label {
            id: LabelId::from_scoped_provider_id(&self.account_id, "gmail", &gl.id),
            account_id: self.account_id.clone(),
            name: gl.name,
            kind,
            color,
            provider_id: gl.id,
            unread_count: gl.messages_unread.unwrap_or(0),
            total_count: gl.messages_total.unwrap_or(0),
            role,
        }
    }

    async fn parse_synced_messages(
        &self,
        messages: Vec<crate::types::GmailMessage>,
    ) -> Result<Vec<SyncedMessage>, MxrError> {
        if messages.len() < GMAIL_PARSE_FANOUT_THRESHOLD {
            return Ok(messages
                .into_iter()
                .filter_map(|message| {
                    parse_synced_message(
                        self.account_id.clone(),
                        message,
                        #[cfg(test)]
                        self.parse_observer.clone(),
                    )
                })
                .collect());
        }

        let concurrency = gmail_parse_concurrency_limit(messages.len());
        let account_id = self.account_id.clone();
        #[cfg(test)]
        let parse_observer = self.parse_observer.clone();

        let results = stream::iter(messages.into_iter().enumerate().map(|(index, message)| {
            let account_id = account_id.clone();
            #[cfg(test)]
            let parse_observer = parse_observer.clone();

            async move {
                tokio::task::spawn_blocking(move || {
                    parse_synced_message(
                        account_id,
                        message,
                        #[cfg(test)]
                        parse_observer,
                    )
                    .map(|synced| IndexedSyncedMessage { index, synced })
                })
                .await
                .map_err(|error| MxrError::Provider(format!("gmail parse task failed: {error}")))
            }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

        let mut parsed = Vec::new();
        for result in results {
            if let Some(message) = result? {
                parsed.push(message);
            }
        }
        parsed.sort_by_key(|message| message.index);

        Ok(parsed.into_iter().map(|message| message.synced).collect())
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
            }
            all_messages.extend(self.parse_synced_messages(messages).await?);

            match resp.next_page_token {
                Some(token) => page_token = Some(token),
                None => {
                    page_token = None;
                    break;
                }
            }
        }

        let has_more = page_token.is_some();
        let next_cursor = match (latest_history_id, &page_token) {
            (Some(hid), Some(token)) => {
                tracing::info!(
                    history_id = hid,
                    "Initial sync producing GmailBackfill cursor for background sync"
                );
                GmailCursor::backfill(hid, token.clone()).encode()
            }
            (Some(hid), None) => {
                tracing::info!(
                    history_id = hid,
                    total = all_messages.len(),
                    "Initial sync complete — all messages fetched, delta-ready"
                );
                GmailCursor::delta(hid).encode()
            }
            _ => SyncCursor::empty(),
        };

        Ok(SyncBatch {
            upserted: all_messages,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
            has_more,
            threads_changed: vec![],
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
                next_cursor: GmailCursor::delta(history_id).encode(),
                has_more: false,
                threads_changed: vec![],
            });
        }

        let ids: Vec<String> = refs.iter().map(|r| r.id.clone()).collect();
        debug!("Backfill: fetching {} messages (full)", ids.len());
        let messages = self
            .client
            .batch_get_messages(&ids, MessageFormat::Full)
            .await
            .map_err(MxrError::from)?;

        let synced = self.parse_synced_messages(messages).await?;

        let has_more = resp.next_page_token.is_some();
        let next_cursor = match resp.next_page_token {
            Some(token) => GmailCursor::backfill(history_id, token).encode(),
            None => GmailCursor::delta(history_id).encode(),
        };

        tracing::info!(fetched = synced.len(), has_more, "Backfill batch complete");

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
            has_more,
            threads_changed: vec![],
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
                        "Gmail history cursor stale; surfacing SyncCursorExpired for daemon-side reset"
                    );
                    return Err(MxrError::SyncCursorExpired {
                        reason: format!("Gmail history cursor {history_id} past retention: {body}"),
                    });
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

            synced = self.parse_synced_messages(messages).await?;
        }

        Ok(SyncBatch {
            upserted: synced,
            deleted_provider_ids: deleted_ids,
            label_changes,
            next_cursor: GmailCursor::delta(latest_history_id).encode(),
            has_more: false,
            threads_changed: vec![],
        })
    }

    async fn apply_modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> mxr_core::provider::Result<()> {
        let add_refs: Vec<&str> = add.iter().map(std::string::String::as_str).collect();
        let remove_refs: Vec<&str> = remove.iter().map(std::string::String::as_str).collect();
        self.client
            .modify_message(provider_message_id, &add_refs, &remove_refs)
            .await
            .map_err(MxrError::from)
    }

    async fn apply_trash(&self, provider_message_id: &str) -> mxr_core::provider::Result<()> {
        self.client
            .trash_message(provider_message_id)
            .await
            .map_err(MxrError::from)
    }

    async fn apply_set_read(
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

    async fn apply_set_starred(
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
}

struct IndexedSyncedMessage {
    index: usize,
    synced: SyncedMessage,
}

fn gmail_parse_concurrency_limit(message_count: usize) -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .min(GMAIL_PARSE_FANOUT_MAX_CONCURRENCY)
        .min(message_count.max(1))
}

fn parse_synced_message(
    account_id: AccountId,
    message: crate::types::GmailMessage,
    #[cfg(test)] parse_observer: Option<ParseObserver>,
) -> Option<SyncedMessage> {
    #[cfg(test)]
    let _parse_guard = parse_observer.as_ref().map(ParseObserver::enter);

    match gmail_message_to_envelope(&message, &account_id) {
        Ok(envelope) => {
            let body = extract_message_body_for_account(&message, &account_id);
            Some(SyncedMessage { envelope, body })
        }
        Err(error) => {
            warn!(
                msg_id = %message.id,
                error = %error,
                "Failed to parse message"
            );
            None
        }
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
            sync: SyncCaps {
                delta: true,
                native_threading: true,
            },
            mutate: MutateCaps {
                labels: true,
                batch_operations: true,
                // Gmail has no native keyword surface; the daemon
                // refuses Mutation::SetKeywords against Gmail accounts.
                custom_keywords: false,
            },
            search: SearchCaps { server_side: true },
            push: PushCaps { streaming: false }, // push via pub/sub not yet implemented
        }
    }

    fn describe_cursor(&self, cursor: &SyncCursor) -> String {
        match GmailCursor::decode(cursor) {
            Ok(None) => "initial".to_string(),
            Ok(Some(c)) => c.describe(),
            Err(_) => format!("expired len={}", cursor.as_bytes().len()),
        }
    }

    fn is_backfill_cursor(&self, cursor: &SyncCursor) -> bool {
        matches!(GmailCursor::decode(cursor), Ok(Some(c)) if c.is_backfill())
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
        match GmailCursor::decode(cursor)? {
            None => self.initial_sync().await,
            Some(decoded) => {
                let GmailCursor::V1(v) = decoded;
                match v.page_token {
                    Some(token) => self.backfill_sync(v.history_id, &token).await,
                    None => self.delta_sync(v.history_id).await,
                }
            }
        }
    }

    async fn fetch_message(
        &self,
        provider_message_id: &str,
    ) -> mxr_core::provider::Result<Option<SyncedMessage>> {
        let message = self
            .client
            .get_message(provider_message_id, MessageFormat::Full)
            .await
            .map_err(MxrError::from)?;
        Ok(parse_synced_message(
            self.account_id.clone(),
            message,
            #[cfg(test)]
            self.parse_observer.clone(),
        ))
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

    async fn apply_mutation(
        &self,
        _mutation_id: &str,
        mutation: &mxr_core::Mutation,
    ) -> mxr_core::provider::Result<()> {
        match mutation {
            mxr_core::Mutation::ModifyLabels {
                provider_message_id,
                add,
                remove,
            } => {
                self.apply_modify_labels(provider_message_id, add, remove)
                    .await
            }
            mxr_core::Mutation::Trash {
                provider_message_id,
            } => self.apply_trash(provider_message_id).await,
            mxr_core::Mutation::SetRead {
                provider_message_id,
                read,
            } => self.apply_set_read(provider_message_id, *read).await,
            mxr_core::Mutation::SetStarred {
                provider_message_id,
                starred,
            } => self.apply_set_starred(provider_message_id, *starred).await,
            mxr_core::Mutation::SetKeywords { .. } => Err(MxrError::Provider(
                "Custom IMAP keywords are not supported by the Gmail adapter; \
                 check capabilities().mutate.custom_keywords before issuing \
                 Mutation::SetKeywords"
                    .to_string(),
            )),
        }
    }

    async fn create_label(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> mxr_core::provider::Result<Label> {
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

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> mxr_core::provider::Result<SendReceipt> {
        let rfc2822 = send::build_rfc2822_async_with_id(draft, from, rfc2822_message_id)
            .await
            .map_err(|e| MxrError::Provider(e.to_string()))?;
        let encoded = send::encode_for_gmail(&rfc2822);

        let result = self
            .client
            .send_message(&encoded)
            .await
            .map_err(MxrError::from)?;

        let message_id = result["id"].as_str().map(std::string::ToString::to_string);

        Ok(SendReceipt {
            provider_message_id: message_id,
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }

    async fn send_calendar_reply(
        &self,
        reply: &mxr_core::CalendarReplyMessage,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> mxr_core::provider::Result<SendReceipt> {
        let message = mxr_outbound::email::build_calendar_reply_message_with_id(
            reply,
            from,
            rfc2822_message_id,
        )
        .map_err(|e| MxrError::Provider(e.to_string()))?;
        let rfc2822 = mxr_outbound::email::format_message_for_gmail(&message);
        let encoded = send::encode_for_gmail(&rfc2822);

        let result = self
            .client
            .send_message(&encoded)
            .await
            .map_err(MxrError::from)?;

        Ok(SendReceipt {
            provider_message_id: result["id"].as_str().map(std::string::ToString::to_string),
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }

    async fn save_draft(
        &self,
        draft: &Draft,
        from: &Address,
    ) -> mxr_core::provider::Result<Option<String>> {
        let rfc2822 = send::build_draft_rfc2822_async(draft, from)
            .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GmailError;
    use crate::types::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;
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

        async fn get_message(
            &self,
            message_id: &str,
            _format: MessageFormat,
        ) -> Result<GmailMessage, GmailError> {
            self.messages
                .get(message_id)
                .cloned()
                .ok_or_else(|| GmailError::NotFound(message_id.to_string()))
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

    struct BulkSyncGmailApi {
        messages: Vec<GmailMessage>,
    }

    #[async_trait]
    impl GmailApi for BulkSyncGmailApi {
        async fn list_messages(
            &self,
            _query: Option<&str>,
            _page_token: Option<&str>,
            _max_results: u32,
        ) -> Result<GmailListResponse, GmailError> {
            Ok(GmailListResponse {
                messages: Some(
                    self.messages
                        .iter()
                        .map(|message| GmailMessageRef {
                            id: message.id.clone(),
                            thread_id: message.thread_id.clone(),
                        })
                        .collect(),
                ),
                next_page_token: None,
                result_size_estimate: Some(self.messages.len() as u64),
            })
        }

        async fn get_message(
            &self,
            message_id: &str,
            _format: MessageFormat,
        ) -> Result<GmailMessage, GmailError> {
            self.messages
                .iter()
                .find(|message| message.id == message_id)
                .cloned()
                .ok_or_else(|| GmailError::NotFound(message_id.to_string()))
        }

        async fn batch_get_messages(
            &self,
            message_ids: &[String],
            _format: MessageFormat,
        ) -> Result<Vec<GmailMessage>, GmailError> {
            Ok(self
                .messages
                .iter()
                .filter(|message| message_ids.iter().any(|id| id == &message.id))
                .cloned()
                .collect())
        }

        async fn list_history(
            &self,
            _start_history_id: u64,
            _page_token: Option<&str>,
        ) -> Result<GmailHistoryResponse, GmailError> {
            unreachable!("history is not used in initial sync fan-out test")
        }

        async fn modify_message(
            &self,
            _message_id: &str,
            _add_labels: &[&str],
            _remove_labels: &[&str],
        ) -> Result<(), GmailError> {
            unreachable!("modify is not used in initial sync fan-out test")
        }

        async fn trash_message(&self, _message_id: &str) -> Result<(), GmailError> {
            unreachable!("trash is not used in initial sync fan-out test")
        }

        async fn send_message(
            &self,
            _raw_base64url: &str,
        ) -> Result<serde_json::Value, GmailError> {
            unreachable!("send is not used in initial sync fan-out test")
        }

        async fn get_attachment(
            &self,
            _message_id: &str,
            _attachment_id: &str,
        ) -> Result<Vec<u8>, GmailError> {
            unreachable!("attachments are not used in initial sync fan-out test")
        }

        async fn create_draft(&self, _raw_base64url: &str) -> Result<String, GmailError> {
            unreachable!("drafts are not used in initial sync fan-out test")
        }

        async fn list_labels(&self) -> Result<GmailLabelsResponse, GmailError> {
            unreachable!("labels are not used in initial sync fan-out test")
        }

        async fn create_label(
            &self,
            _name: &str,
            _color: Option<&str>,
        ) -> Result<GmailLabel, GmailError> {
            unreachable!("labels are not used in initial sync fan-out test")
        }

        async fn rename_label(
            &self,
            _label_id: &str,
            _new_name: &str,
        ) -> Result<GmailLabel, GmailError> {
            unreachable!("labels are not used in initial sync fan-out test")
        }

        async fn delete_label(&self, _label_id: &str) -> Result<(), GmailError> {
            unreachable!("labels are not used in initial sync fan-out test")
        }
    }

    fn bulk_gmail_provider(message_count: usize, parse_observer: ParseObserver) -> GmailProvider {
        let messages = (0..message_count)
            .map(|index| {
                serde_json::from_value::<GmailMessage>(gmail_message(
                    &format!("bulk-msg-{index}"),
                    &format!("bulk-thread-{index}"),
                    &format!("Bulk subject {index}"),
                ))
                .unwrap()
            })
            .collect();

        GmailProvider::with_api_and_parse_observer(
            AccountId::new(),
            Box::new(BulkSyncGmailApi { messages }),
            parse_observer,
        )
    }

    #[tokio::test]
    async fn gmail_provider_passes_sync_and_send_conformance() {
        let provider = gmail_provider();
        mxr_provider_fake::conformance::run_sync_conformance(&provider).await;
        mxr_provider_fake::conformance::run_send_conformance(&provider).await;
    }

    #[tokio::test]
    async fn gmail_delta_sync_tracks_history_changes() {
        let provider = gmail_provider();
        let batch = provider
            .sync_messages(&GmailCursor::delta(22).encode())
            .await
            .unwrap();

        assert_eq!(batch.deleted_provider_ids, vec!["msg-1"]);
        assert_eq!(batch.label_changes.len(), 1);
        assert_eq!(batch.upserted.len(), 1);
        assert_eq!(batch.upserted[0].envelope.provider_id, "msg-3");
        let next = GmailCursor::decode(&batch.next_cursor).unwrap().unwrap();
        let GmailCursor::V1(v) = next;
        assert_eq!(v.history_id, 23);
        assert!(v.page_token.is_none());
    }

    #[tokio::test]
    async fn gmail_delta_sync_surfaces_sync_cursor_expired_when_history_is_stale() {
        // Adapters no longer self-recover from stale cursors; they surface
        // SyncCursorExpired so the daemon's reset-to-Initial path runs
        // uniformly across providers (MSP alignment, see docs/msp/spec.md §5).
        let provider = gmail_provider_with_stale_history(true);
        let err = provider
            .sync_messages(&GmailCursor::delta(27_672_073).encode())
            .await
            .expect_err("stale history cursor should surface SyncCursorExpired");

        let reason = match err {
            MxrError::SyncCursorExpired { reason } => reason,
            other => panic!("expected SyncCursorExpired, got {other:?}"),
        };
        assert!(
            reason.contains("27672073"),
            "reason should carry the expired history id: {reason}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gmail_initial_sync_fans_out_parsing_for_large_batches() {
        let parse_observer = ParseObserver::new(Duration::from_millis(10));
        let provider = bulk_gmail_provider(64, parse_observer.clone());

        let batch = provider.sync_messages(&SyncCursor::empty()).await.unwrap();

        assert_eq!(batch.upserted.len(), 64);
        assert!(
            parse_observer.max_concurrency() > 1,
            "expected parsing overlap for large batches, observed {}",
            parse_observer.max_concurrency()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gmail_initial_sync_keeps_small_batches_inline() {
        let parse_observer = ParseObserver::new(Duration::from_millis(10));
        let provider = bulk_gmail_provider(63, parse_observer.clone());

        let batch = provider.sync_messages(&SyncCursor::empty()).await.unwrap();

        assert_eq!(batch.upserted.len(), 63);
        assert_eq!(parse_observer.max_concurrency(), 1);
    }

    #[tokio::test]
    async fn gmail_backfill_with_more_pages_surfaces_has_more() {
        let provider = gmail_provider();
        let cursor = GmailCursor::backfill(22, "page-1".into()).encode();

        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert!(
            batch.has_more,
            "backfill with a non-terminal page should surface has_more = true"
        );
        let next = GmailCursor::decode(&batch.next_cursor).unwrap().unwrap();
        let GmailCursor::V1(v) = next;
        assert_eq!(v.page_token.as_deref(), Some("page-2"));
    }

    #[tokio::test]
    async fn gmail_backfill_final_page_surfaces_has_more_false() {
        let provider = gmail_provider();
        let cursor = GmailCursor::backfill(22, "page-2".into()).encode();

        let batch = provider.sync_messages(&cursor).await.unwrap();

        assert!(
            !batch.has_more,
            "the terminal backfill page must surface has_more = false so the daemon stops re-polling"
        );
        let next = GmailCursor::decode(&batch.next_cursor).unwrap().unwrap();
        let GmailCursor::V1(v) = next;
        assert!(
            v.page_token.is_none(),
            "next_cursor should be delta-ready (no page_token)"
        );
    }

    /// Phase E: Gmail has no native keyword surface. The adapter advertises
    /// `mutate.custom_keywords = false`, and any SetKeywords mutation
    /// returned by an outdated client must surface as a typed provider error
    /// rather than silently dropping.
    #[tokio::test]
    async fn gmail_rejects_set_keywords_mutation() {
        let provider = gmail_provider();

        assert!(
            !provider.capabilities().mutate.custom_keywords,
            "Gmail adapter must advertise no keyword support"
        );

        let err = provider
            .apply_mutation(
                "mut-1",
                &mxr_core::Mutation::SetKeywords {
                    provider_message_id: "msg-1".to_string(),
                    add: vec!["$Forwarded".to_string()],
                    remove: vec![],
                },
            )
            .await
            .expect_err("SetKeywords must error on Gmail");

        match err {
            MxrError::Provider(msg) => {
                assert!(
                    msg.contains("keyword"),
                    "error must mention keywords: {msg}"
                );
            }
            other => panic!("expected Provider error, got {other:?}"),
        }
    }
}
