//! Fake provider, canonical fixtures, and adapter conformance helpers.
//!
//! This crate serves three jobs:
//! - network-free integration testing
//! - reference provider implementation for adapter authors
//! - reusable conformance checks exported from [`conformance`]

pub mod conformance;
pub mod fixtures;

use async_trait::async_trait;
use mail_builder::MessageBuilder;
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::{
    IdleWatcher, MailSendProvider, MailSyncProvider, MxrError, SendReceipt, SyncCapabilities,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Notify;

/// Messages per `sync_messages` page.
///
/// Paging keeps the demo seed observable (the cursor and `sync_log` advance
/// while it runs) and keeps peak memory proportional to a page rather than
/// to the whole dataset. Every page also costs the daemon a post-sync
/// fan-out (rules, deliveries, analytics) that competes with the sync for
/// the single SQLite writer, so pages are large enough that a 50,000
/// message demo is a handful of them per account — and still far under the
/// daemon's 50-page cap on consecutive `has_more` re-polls.
const SYNC_PAGE_SIZE: usize = 10_000;

/// Cursor for "resume the initial sync at this offset".
const PAGE_CURSOR_PREFIX: &str = "fake-page:";
/// Cursor for "initial sync finished"; every later call is a no-op delta.
const COMPLETE_CURSOR: &[u8] = b"fake-synced";

/// Where a provider's messages come from.
///
/// The demo dataset is generated per page instead of materialised: 50,000
/// synthetic messages cost ~2.5 KB each and used to sit in memory for the
/// daemon's whole life.
enum Dataset {
    Materialised {
        messages: Vec<Envelope>,
        bodies: HashMap<String, MessageBody>,
    },
    Demo(fixtures::DemoFixtureStream),
}

impl Dataset {
    fn len(&self) -> usize {
        match self {
            Self::Materialised { messages, .. } => messages.len(),
            Self::Demo(stream) => stream.len(),
        }
    }

    fn page(&self, offset: usize, limit: usize) -> Vec<SyncedMessage> {
        match self {
            Self::Materialised { messages, bodies } => messages
                .iter()
                .skip(offset)
                .take(limit)
                .map(|envelope| SyncedMessage {
                    body: body_for(bodies, envelope),
                    envelope: envelope.clone(),
                })
                .collect(),
            Self::Demo(stream) => stream
                .page(offset, limit)
                .into_iter()
                .map(|(envelope, body)| SyncedMessage { envelope, body })
                .collect(),
        }
    }

    fn find(&self, provider_message_id: &str) -> Option<SyncedMessage> {
        match self {
            Self::Materialised { messages, bodies } => messages
                .iter()
                .find(|message| message.provider_id == provider_message_id)
                .map(|envelope| SyncedMessage {
                    body: body_for(bodies, envelope),
                    envelope: envelope.clone(),
                }),
            Self::Demo(stream) => stream
                .find(provider_message_id)
                .map(|(envelope, body)| SyncedMessage { envelope, body }),
        }
    }
}

/// Offset carried by a mid-backfill cursor, if this is one.
fn page_offset(cursor: &SyncCursor) -> Option<usize> {
    std::str::from_utf8(cursor.as_bytes())
        .ok()?
        .strip_prefix(PAGE_CURSOR_PREFIX)?
        .parse()
        .ok()
}

fn body_for(bodies: &HashMap<String, MessageBody>, envelope: &Envelope) -> MessageBody {
    bodies
        .get(&envelope.provider_id)
        .cloned()
        .unwrap_or_else(|| fixtures::empty_body(&envelope.id))
}

pub struct FakeProvider {
    account_id: AccountId,
    dataset: Dataset,
    labels: Mutex<Vec<Label>>,
    sent: Mutex<Vec<Draft>>,
    server_drafts: Mutex<HashMap<String, Draft>>,
    server_draft_revisions: Mutex<HashMap<String, u64>>,
    /// The resolved From `Address` handed to each `send`, in order — lets
    /// tests assert the daemon selected and forwarded the right sender
    /// (primary vs a per-message alias override).
    sent_from: Mutex<Vec<Address>>,
    mutations: Mutex<Vec<RecordedMutation>>,
    /// Phase 3.1: when set, `idle_watch` returns a watcher that emits a
    /// notification each time `idle_trigger.notify_one()` is called.
    /// Tests use this to simulate server-pushed EXISTS / EXPUNGE events.
    idle_trigger: Option<Arc<Notify>>,
    /// When set, `save_draft` and `update_draft` fail. Lets tests drive the
    /// daemon's provider-failure branches, where the promise is that the local
    /// draft is left exactly as it was.
    server_drafts_fail: AtomicBool,
    page_size: usize,
}

#[derive(Debug, Clone)]
pub enum RecordedMutation {
    LabelsModified {
        provider_id: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
    Trashed {
        provider_id: String,
    },
    ReadSet {
        provider_id: String,
        read: bool,
    },
    StarredSet {
        provider_id: String,
        starred: bool,
    },
    KeywordsSet {
        provider_id: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
}

impl FakeProvider {
    fn labels_guard(&self) -> std::sync::MutexGuard<'_, Vec<Label>> {
        self.labels
            .lock()
            .expect("fake provider labels mutex should not be poisoned")
    }

    fn sent_guard(&self) -> std::sync::MutexGuard<'_, Vec<Draft>> {
        self.sent
            .lock()
            .expect("fake provider sent mutex should not be poisoned")
    }

    fn mutations_guard(&self) -> std::sync::MutexGuard<'_, Vec<RecordedMutation>> {
        self.mutations
            .lock()
            .expect("fake provider mutations mutex should not be poisoned")
    }

    pub fn new(account_id: AccountId) -> Self {
        match crate::fixtures::demo_message_count_from_env() {
            Some(count) => Self::with_demo_dataset(account_id, count),
            None => {
                let (messages, bodies, labels) = crate::fixtures::generate_fixtures(&account_id);
                Self::with_dataset(
                    account_id,
                    Dataset::Materialised { messages, bodies },
                    labels,
                )
            }
        }
    }

    /// Build a provider backed by the generated demo dataset, without
    /// reading `MXR_FAKE_DATASET` / `MXR_FAKE_MESSAGE_COUNT`.
    pub fn with_demo_dataset(account_id: AccountId, message_count: usize) -> Self {
        let stream = fixtures::DemoFixtureStream::new(&account_id, message_count);
        let labels = stream.labels().to_vec();
        Self::with_dataset(account_id, Dataset::Demo(stream), labels)
    }

    /// Page `sync_messages` at `page_size` instead of the default. Lets a
    /// test walk several pages without a large dataset.
    pub fn with_page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size.max(1);
        self
    }

    fn with_dataset(account_id: AccountId, dataset: Dataset, labels: Vec<Label>) -> Self {
        Self {
            account_id,
            dataset,
            labels: Mutex::new(labels),
            sent: Mutex::new(Vec::new()),
            server_drafts: Mutex::new(HashMap::new()),
            server_draft_revisions: Mutex::new(HashMap::new()),
            sent_from: Mutex::new(Vec::new()),
            mutations: Mutex::new(Vec::new()),
            idle_trigger: None,
            server_drafts_fail: AtomicBool::new(false),
            page_size: SYNC_PAGE_SIZE,
        }
    }

    /// Make every subsequent server-draft write fail.
    pub fn fail_server_drafts(&self) {
        self.server_drafts_fail.store(true, Ordering::SeqCst);
    }

    /// Enable IDLE watching. Returns the Notify handle test code uses
    /// to simulate server-pushed events.
    pub fn enable_idle(&mut self) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        self.idle_trigger = Some(notify.clone());
        notify
    }

    pub fn sent_drafts(&self) -> Vec<Draft> {
        self.sent_guard().clone()
    }

    pub fn server_drafts(&self) -> HashMap<String, Draft> {
        self.server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned")
            .clone()
    }

    /// Test seam for a provider-side edit that should be pulled locally on the
    /// next draft reconciliation.
    pub fn replace_server_draft(&self, provider_draft_id: &str, draft: Draft) -> bool {
        let mut server_drafts = self
            .server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned");
        let Some(stored) = server_drafts.get_mut(provider_draft_id) else {
            return false;
        };
        *stored = draft;
        let mut revisions = self
            .server_draft_revisions
            .lock()
            .expect("fake provider server_draft_revisions mutex should not be poisoned");
        *revisions.entry(provider_draft_id.to_string()).or_insert(1) += 1;
        true
    }

    /// The resolved From `Address` passed to each `send`, in call order.
    pub fn sent_from_addresses(&self) -> Vec<Address> {
        self.sent_from
            .lock()
            .expect("fake provider sent_from mutex should not be poisoned")
            .clone()
    }

    pub fn mutations(&self) -> Vec<RecordedMutation> {
        self.mutations_guard().clone()
    }
}

#[async_trait]
impl MailSyncProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            sync: SyncCaps {
                delta: false,
                native_threading: true,
            },
            mutate: MutateCaps {
                labels: true,
                batch_operations: false,
                custom_keywords: true,
            },
            search: SearchCaps { server_side: false },
            push: PushCaps { streaming: false },
        }
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(self.labels_guard().clone())
    }

    fn is_backfill_cursor(&self, cursor: &SyncCursor) -> bool {
        page_offset(cursor).is_some()
    }

    fn describe_cursor(&self, cursor: &SyncCursor) -> String {
        match page_offset(cursor) {
            Some(offset) => format!("backfill offset={offset}/{}", self.dataset.len()),
            None if cursor.is_empty() => "initial".to_string(),
            None => "synced".to_string(),
        }
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        let offset = if cursor.is_empty() {
            0
        } else {
            match page_offset(cursor) {
                Some(offset) => offset,
                // Any other non-empty cursor means the initial sync
                // already finished: steady state returns empty batches.
                None => {
                    return Ok(SyncBatch {
                        upserted: vec![],
                        deleted_provider_ids: vec![],
                        label_changes: vec![],
                        next_cursor: cursor.clone(),
                        has_more: false,
                        threads_changed: vec![],
                    })
                }
            }
        };

        let total = self.dataset.len();
        let upserted = self.dataset.page(offset, self.page_size);
        let next_offset = offset.saturating_add(upserted.len());
        let has_more = next_offset < total;
        let next_cursor = if has_more {
            SyncCursor::from_bytes(format!("{PAGE_CURSOR_PREFIX}{next_offset}").into_bytes())
        } else {
            SyncCursor::from_bytes(COMPLETE_CURSOR.to_vec())
        };

        Ok(SyncBatch {
            upserted,
            deleted_provider_ids: vec![],
            label_changes: vec![],
            next_cursor,
            has_more,
            threads_changed: vec![],
        })
    }

    async fn fetch_message(
        &self,
        provider_message_id: &str,
    ) -> Result<Option<SyncedMessage>, MxrError> {
        Ok(self.dataset.find(provider_message_id))
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, MxrError> {
        Ok(b"fake attachment content".to_vec())
    }

    async fn apply_mutation(
        &self,
        _mutation_id: &str,
        mutation: &Mutation,
    ) -> Result<(), MxrError> {
        let recorded = match mutation {
            Mutation::ModifyLabels {
                provider_message_id,
                add,
                remove,
            } => RecordedMutation::LabelsModified {
                provider_id: provider_message_id.clone(),
                added: add.clone(),
                removed: remove.clone(),
            },
            Mutation::Trash {
                provider_message_id,
            } => RecordedMutation::Trashed {
                provider_id: provider_message_id.clone(),
            },
            Mutation::SetRead {
                provider_message_id,
                read,
            } => RecordedMutation::ReadSet {
                provider_id: provider_message_id.clone(),
                read: *read,
            },
            Mutation::SetStarred {
                provider_message_id,
                starred,
            } => RecordedMutation::StarredSet {
                provider_id: provider_message_id.clone(),
                starred: *starred,
            },
            Mutation::SetKeywords {
                provider_message_id,
                add,
                remove,
            } => RecordedMutation::KeywordsSet {
                provider_id: provider_message_id.clone(),
                added: add.clone(),
                removed: remove.clone(),
            },
        };
        self.mutations_guard().push(recorded);
        Ok(())
    }

    async fn create_label(&self, name: &str, color: Option<&str>) -> Result<Label, MxrError> {
        let label = Label {
            id: LabelId::from_scoped_provider_id(&self.account_id, "fake", name),
            account_id: self.account_id.clone(),
            name: name.to_string(),
            kind: LabelKind::User,
            color: color.map(str::to_string),
            provider_id: name.to_string(),
            unread_count: 0,
            total_count: 0,
            role: None,
        };
        self.labels_guard().push(label.clone());
        Ok(label)
    }

    async fn rename_label(
        &self,
        provider_label_id: &str,
        new_name: &str,
    ) -> Result<Label, MxrError> {
        let mut labels = self.labels_guard();
        let label = labels
            .iter_mut()
            .find(|label| label.provider_id == provider_label_id)
            .ok_or_else(|| MxrError::NotFound(format!("label {provider_label_id}")))?;
        label.id = LabelId::from_scoped_provider_id(&self.account_id, "fake", new_name);
        label.name = new_name.to_string();
        label.provider_id = new_name.to_string();
        Ok(label.clone())
    }

    async fn delete_label(&self, provider_label_id: &str) -> Result<(), MxrError> {
        let mut labels = self.labels_guard();
        let before = labels.len();
        labels.retain(|label| label.provider_id != provider_label_id);
        if labels.len() == before {
            return Err(MxrError::NotFound(format!("label {provider_label_id}")));
        }
        Ok(())
    }

    async fn idle_watch(&self) -> Result<Option<Box<dyn IdleWatcher>>, MxrError> {
        let Some(trigger) = self.idle_trigger.clone() else {
            return Ok(None);
        };
        Ok(Some(Box::new(FakeIdleWatcher { trigger })))
    }
}

struct FakeIdleWatcher {
    trigger: Arc<Notify>,
}

#[async_trait]
impl IdleWatcher for FakeIdleWatcher {
    async fn next_event(&mut self) -> Result<(), MxrError> {
        self.trigger.notified().await;
        Ok(())
    }
}

impl FakeProvider {
    fn fail_if_requested(&self) -> Result<(), MxrError> {
        if self.server_drafts_fail.load(Ordering::SeqCst) {
            return Err(MxrError::Provider("fake: server draft write failed".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl MailSendProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn supports_server_drafts(&self) -> bool {
        true
    }

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        self.sent_guard().push(draft.clone());
        self.sent_from
            .lock()
            .expect("fake provider sent_from mutex should not be poisoned")
            .push(from.clone());
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }

    /// Mirror Gmail: a reply draft is filed on its parent's thread, so the
    /// daemon's cache-back of the provider thread id is exercised in tests.
    async fn resolve_reply_thread_id(&self, draft: &Draft) -> Result<Option<String>, MxrError> {
        Ok(draft.reply_headers.as_ref().map(|headers| {
            headers
                .thread_id
                .clone()
                .unwrap_or_else(|| format!("fake-thread-{}", headers.in_reply_to))
        }))
    }

    async fn save_draft(&self, draft: &Draft, _from: &Address) -> Result<Option<String>, MxrError> {
        self.fail_if_requested()?;
        let provider_draft_id = format!("fake-draft-{}", uuid::Uuid::now_v7());
        self.server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned")
            .insert(provider_draft_id.clone(), draft.clone());
        self.server_draft_revisions
            .lock()
            .expect("fake provider server_draft_revisions mutex should not be poisoned")
            .insert(provider_draft_id.clone(), 1);
        Ok(Some(provider_draft_id))
    }

    async fn update_draft(
        &self,
        provider_draft_id: &str,
        draft: &Draft,
        _from: &Address,
    ) -> Result<(), MxrError> {
        self.fail_if_requested()?;
        let mut server_drafts = self
            .server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned");
        let stored = server_drafts
            .get_mut(provider_draft_id)
            .ok_or_else(|| MxrError::NotFound(format!("provider draft {provider_draft_id}")))?;
        *stored = draft.clone();
        let mut revisions = self
            .server_draft_revisions
            .lock()
            .expect("fake provider server_draft_revisions mutex should not be poisoned");
        *revisions.entry(provider_draft_id.to_string()).or_insert(1) += 1;
        Ok(())
    }

    async fn fetch_draft(
        &self,
        provider_draft_id: &str,
    ) -> Result<Option<mxr_core::ServerDraftSnapshot>, MxrError> {
        let draft = self
            .server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned")
            .get(provider_draft_id)
            .cloned();
        let Some(draft) = draft else {
            return Ok(None);
        };
        let revision = self
            .server_draft_revisions
            .lock()
            .expect("fake provider server_draft_revisions mutex should not be poisoned")
            .get(provider_draft_id)
            .copied()
            .unwrap_or(1);
        Ok(Some(mxr_core::ServerDraftSnapshot {
            revision: revision.to_string(),
            raw_rfc822: build_fake_draft_message(&draft)?,
        }))
    }

    async fn delete_draft(&self, provider_draft_id: &str) -> Result<(), MxrError> {
        let removed = self
            .server_drafts
            .lock()
            .expect("fake provider server_drafts mutex should not be poisoned")
            .remove(provider_draft_id);
        self.server_draft_revisions
            .lock()
            .expect("fake provider server_draft_revisions mutex should not be poisoned")
            .remove(provider_draft_id);
        if removed.is_none() {
            return Err(MxrError::NotFound(format!(
                "provider draft {provider_draft_id}"
            )));
        }
        Ok(())
    }

    async fn send_calendar_reply(
        &self,
        reply: &mxr_core::CalendarReplyMessage,
        from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        self.sent_guard().push(Draft {
            id: mxr_core::DraftId::new(),
            account_id: self.account_id.clone(),
            from: None,
            reply_headers: None,
            intent: mxr_core::DraftIntent::Reply,
            to: vec![reply.to.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: reply.subject.clone(),
            content: DraftContent::markdown(format!("{}\n\n{}", reply.body_text, reply.ics)),
            attachments: Vec::new(),
            inline_assets: Vec::new(),
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
        self.sent_from
            .lock()
            .expect("fake provider sent_from mutex should not be poisoned")
            .push(from.clone());
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-calendar-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }
}

fn build_fake_draft_message(draft: &Draft) -> Result<Vec<u8>, MxrError> {
    let from = draft
        .from
        .as_ref()
        .map_or("fake@example.com", |from| from.email.as_str());
    let mut builder = MessageBuilder::new()
        .from(from)
        .subject(draft.subject.clone());
    for address in &draft.to {
        builder = builder.to(address.email.as_str());
    }
    for address in &draft.cc {
        builder = builder.cc(address.email.as_str());
    }
    for address in &draft.bcc {
        builder = builder.bcc(address.email.as_str());
    }
    builder = match &draft.content {
        DraftContent::Markdown { source } => builder.text_body(source.clone()),
        DraftContent::Html { html, text } => builder
            .text_body(text.clone().unwrap_or_default())
            .html_body(html.clone()),
    };
    for path in &draft.attachments {
        let bytes = std::fs::read(path).map_err(|error| MxrError::Provider(error.to_string()))?;
        let filename = path
            .file_name()
            .and_then(|filename| filename.to_str())
            .unwrap_or("attachment")
            .to_string();
        builder = builder.attachment("application/octet-stream", filename, bytes);
    }
    for asset in &draft.inline_assets {
        let bytes =
            std::fs::read(&asset.path).map_err(|error| MxrError::Provider(error.to_string()))?;
        builder = builder.inline("application/octet-stream", asset.cid.clone(), bytes);
    }
    builder
        .write_to_vec()
        .map_err(|error| MxrError::Provider(error.to_string()))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn fixture_counts() {
        let account_id = AccountId::new();
        let (envelopes, _, labels) = fixtures::generate_fixtures(&account_id);
        assert_eq!(envelopes.len(), 55);
        assert_eq!(labels.len(), 8);
    }

    #[test]
    fn fixture_threads() {
        let account_id = AccountId::new();
        let (envelopes, _, _) = fixtures::generate_fixtures(&account_id);
        let thread_ids: HashSet<String> = envelopes.iter().map(|e| e.thread_id.as_str()).collect();
        assert!(thread_ids.len() >= 12);
    }

    #[test]
    fn fixture_unsubscribe_methods() {
        let account_id = AccountId::new();
        let (envelopes, _, _) = fixtures::generate_fixtures(&account_id);
        let methods: HashSet<String> = envelopes
            .iter()
            .map(|e| format!("{:?}", std::mem::discriminant(&e.unsubscribe)))
            .collect();
        // Should have at least None, OneClick, HttpLink, Mailto
        assert!(methods.len() >= 3);
    }

    #[test]
    fn fixture_attachments() {
        let account_id = AccountId::new();
        let (envelopes, _, _) = fixtures::generate_fixtures(&account_id);
        let with_attachments = envelopes.iter().filter(|e| e.has_attachments).count();
        assert!(with_attachments >= 3);
    }

    #[test]
    fn demo_fixtures_split_known_demo_accounts() {
        let personal = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let work = AccountId::from_provider_id("fake", "alex@work.demo.mxr.local");
        let (personal_env, _, _) = fixtures::generate_demo_fixtures(&personal, 100);
        let (work_env, _, _) = fixtures::generate_demo_fixtures(&work, 100);

        assert_eq!(personal_env.len(), 55);
        assert_eq!(work_env.len(), 45);
        assert!(personal_env.iter().all(|env| env.account_id == personal));
        assert!(work_env.iter().all(|env| env.account_id == work));
    }

    #[test]
    fn demo_fixtures_exercise_links_html_attachments_and_colors() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let (envelopes, bodies, labels) = fixtures::generate_demo_fixtures(&account_id, 120);
        let html_with_links = bodies
            .values()
            .filter_map(|body| body.text_html.as_deref())
            .filter(|html| html.contains("href=\"https://"))
            .count();
        let image_bodies = bodies
            .values()
            .filter_map(|body| body.text_html.as_deref())
            .filter(|html| html.contains("<img "))
            .count();
        let attachment_messages = envelopes.iter().filter(|env| env.has_attachments).count();
        let colored_labels = labels.iter().filter(|label| label.color.is_some()).count();

        assert!(html_with_links > 20);
        assert!(image_bodies > 0);
        assert!(attachment_messages > 0);
        assert!(colored_labels >= 6);
    }

    #[test]
    fn demo_fixtures_include_spam_promotions_and_suspicious_inbox_mail() {
        let account_id = AccountId::from_provider_id("fake", "alex@demo.mxr.local");
        let (envelopes, _, labels) = fixtures::generate_demo_fixtures(&account_id, 160);
        let spam = envelopes
            .iter()
            .filter(|env| {
                env.flags.contains(MessageFlags::SPAM)
                    && env.label_provider_ids.iter().any(|label| label == "SPAM")
            })
            .count();
        let promotions = envelopes
            .iter()
            .filter(|env| {
                env.label_provider_ids
                    .iter()
                    .any(|label| label == "promotions")
            })
            .count();
        let potential_spam_inbox = envelopes
            .iter()
            .filter(|env| {
                !env.flags.contains(MessageFlags::SPAM)
                    && env.label_provider_ids.iter().any(|label| label == "INBOX")
                    && env
                        .label_provider_ids
                        .iter()
                        .any(|label| label == "potential_spam")
            })
            .count();

        assert!(spam > 0);
        assert!(promotions > 0);
        assert!(potential_spam_inbox > 0);
        assert!(labels
            .iter()
            .any(|label| label.provider_id == "promotions" && label.color.is_some()));
        assert!(labels
            .iter()
            .any(|label| label.provider_id == "potential_spam" && label.color.is_some()));
    }

    #[tokio::test]
    async fn sync_initial_returns_all_with_bodies() {
        let provider = FakeProvider::new(AccountId::new());
        let batch = provider.sync_messages(&SyncCursor::empty()).await.unwrap();
        assert_eq!(batch.upserted.len(), 55);
        assert!(!batch.has_more);
        // Bodies are eagerly fetched during sync
        assert!(batch.upserted[0].body.text_plain.is_some());
    }

    /// Walking the pages must yield every message exactly once, in order,
    /// and only the last page may report `has_more == false`.
    #[tokio::test]
    async fn demo_dataset_pages_until_complete() {
        // A non-demo account id keeps the whole target on one provider
        // (the two known demo accounts split it 45/55).
        const PAGE: usize = 200;
        let total = PAGE * 2 + 37;
        let provider =
            FakeProvider::with_demo_dataset(AccountId::new(), total).with_page_size(PAGE);

        let mut cursor = SyncCursor::empty();
        let mut provider_ids = Vec::new();
        let mut pages = 0;
        loop {
            let batch = provider.sync_messages(&cursor).await.unwrap();
            pages += 1;
            assert!(pages <= 4, "paging should terminate");
            provider_ids.extend(
                batch
                    .upserted
                    .iter()
                    .map(|synced| synced.envelope.provider_id.clone()),
            );
            assert_eq!(batch.has_more, provider_ids.len() < total);
            assert_eq!(
                provider.is_backfill_cursor(&batch.next_cursor),
                batch.has_more
            );
            cursor = batch.next_cursor;
            if !batch.has_more {
                break;
            }
        }

        assert_eq!(pages, 3);
        assert_eq!(provider_ids.len(), total);
        let unique: HashSet<&String> = provider_ids.iter().collect();
        assert_eq!(unique.len(), total, "pages must not overlap");
        assert_eq!(
            provider_ids,
            (1..=total)
                .map(|num| format!("demo-msg-{num}"))
                .collect::<Vec<_>>()
        );

        // Steady state: the completed cursor yields empty batches.
        let delta = provider.sync_messages(&cursor).await.unwrap();
        assert!(delta.upserted.is_empty());
        assert!(!delta.has_more);
    }

    /// A demo profile seeded before paging existed carries the old
    /// one-shot cursor. The provider must read that as "initial sync
    /// finished" rather than replaying the whole dataset over rows that
    /// already exist.
    #[tokio::test]
    async fn legacy_cursors_are_treated_as_caught_up() {
        let provider = FakeProvider::with_demo_dataset(AccountId::new(), 500).with_page_size(100);

        for legacy in [
            b"fake-synced".to_vec(),
            b"some-older-opaque-cursor".to_vec(),
        ] {
            let cursor = SyncCursor::from_bytes(legacy.clone());
            let batch = provider.sync_messages(&cursor).await.unwrap();
            assert!(
                batch.upserted.is_empty(),
                "legacy cursor {legacy:?} must not replay the dataset"
            );
            assert!(!batch.has_more);
            assert_eq!(
                batch.next_cursor.as_bytes(),
                cursor.as_bytes(),
                "the cursor must be preserved"
            );
            assert!(!provider.is_backfill_cursor(&cursor));
        }
    }

    /// A page resumed from a cursor must equal the same slice generated in
    /// one pass — that is what makes a restarted seed idempotent.
    #[tokio::test]
    async fn demo_pages_are_stable_across_resume() {
        const PAGE: usize = 200;
        let provider =
            FakeProvider::with_demo_dataset(AccountId::new(), PAGE * 2).with_page_size(PAGE);

        let first = provider.sync_messages(&SyncCursor::empty()).await.unwrap();
        let second = provider.sync_messages(&first.next_cursor).await.unwrap();
        let resumed = provider.sync_messages(&first.next_cursor).await.unwrap();

        let ids = |batch: &SyncBatch| {
            batch
                .upserted
                .iter()
                .map(|synced| (synced.envelope.id.clone(), synced.envelope.subject.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&second), ids(&resumed));

        // And a single message fetched by provider id matches its page copy.
        let sample = &second.upserted[7];
        let fetched = provider
            .fetch_message(&sample.envelope.provider_id)
            .await
            .unwrap()
            .expect("message should be fetchable");
        assert_eq!(fetched.envelope.id, sample.envelope.id);
        assert_eq!(fetched.envelope.thread_id, sample.envelope.thread_id);
        assert_eq!(fetched.body.text_plain, sample.body.text_plain);
    }

    #[tokio::test]
    async fn sync_delta_returns_empty() {
        let provider = FakeProvider::new(AccountId::new());
        let batch = provider
            .sync_messages(&SyncCursor::from_bytes(b"any-non-empty".to_vec()))
            .await
            .unwrap();
        assert_eq!(batch.upserted.len(), 0);
    }

    #[tokio::test]
    async fn mutations_recorded() {
        let provider = FakeProvider::new(AccountId::new());
        provider
            .apply_mutation(
                "mut-1",
                &Mutation::Trash {
                    provider_message_id: "fake-msg-1".to_string(),
                },
            )
            .await
            .unwrap();
        provider
            .apply_mutation(
                "mut-2",
                &Mutation::SetRead {
                    provider_message_id: "fake-msg-2".to_string(),
                    read: true,
                },
            )
            .await
            .unwrap();
        provider
            .apply_mutation(
                "mut-3",
                &Mutation::ModifyLabels {
                    provider_message_id: "fake-msg-3".to_string(),
                    add: vec!["work".to_string()],
                    remove: vec![],
                },
            )
            .await
            .unwrap();

        let mutations = provider.mutations();
        assert_eq!(mutations.len(), 3);
        assert!(matches!(mutations[0], RecordedMutation::Trashed { .. }));
        assert!(matches!(
            mutations[1],
            RecordedMutation::ReadSet { read: true, .. }
        ));
        assert!(matches!(
            mutations[2],
            RecordedMutation::LabelsModified { .. }
        ));
    }

    #[tokio::test]
    async fn send_recorded() {
        let provider = FakeProvider::new(AccountId::new());
        let draft = Draft {
            id: DraftId::new(),
            account_id: provider.account_id().clone(),
            from: None,
            intent: DraftIntent::New,
            reply_headers: None,
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test".to_string(),
            content: DraftContent::markdown("Hello"),
            attachments: vec![],
            inline_assets: Vec::new(),
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let from = Address {
            name: Some("User".to_string()),
            email: "user@example.com".to_string(),
        };
        provider
            .send(&draft, &from, "<test-message@example.com>")
            .await
            .unwrap();
        assert_eq!(provider.sent_drafts().len(), 1);
    }

    #[tokio::test]
    async fn fake_provider_passes_sync_conformance() {
        let provider = FakeProvider::new(AccountId::new());
        crate::conformance::run_sync_conformance(&provider).await;
    }

    #[tokio::test]
    async fn fake_provider_passes_send_conformance() {
        let provider = FakeProvider::new(AccountId::new());
        crate::conformance::run_send_conformance(&provider).await;
    }
}
