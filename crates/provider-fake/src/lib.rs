//! Fake provider, canonical fixtures, and adapter conformance helpers.
//!
//! This crate serves three jobs:
//! - network-free integration testing
//! - reference provider implementation for adapter authors
//! - reusable conformance checks exported from [`conformance`]

pub mod conformance;
pub mod fixtures;

use async_trait::async_trait;
use mxr_core::id::*;
use mxr_core::types::*;
use mxr_core::{
    IdleWatcher, MailSendProvider, MailSyncProvider, MxrError, SendReceipt, SyncCapabilities,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Notify;

pub struct FakeProvider {
    account_id: AccountId,
    messages: Vec<Envelope>,
    bodies: HashMap<String, MessageBody>,
    labels: Mutex<Vec<Label>>,
    sent: Mutex<Vec<Draft>>,
    mutations: Mutex<Vec<RecordedMutation>>,
    /// Phase 3.1: when set, `idle_watch` returns a watcher that emits a
    /// notification each time `idle_trigger.notify_one()` is called.
    /// Tests use this to simulate server-pushed EXISTS / EXPUNGE events.
    idle_trigger: Option<Arc<Notify>>,
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
        let (messages, bodies, labels) =
            crate::fixtures::generate_env_selected_fixtures(&account_id);
        Self {
            account_id,
            messages,
            bodies,
            labels: Mutex::new(labels),
            sent: Mutex::new(Vec::new()),
            mutations: Mutex::new(Vec::new()),
            idle_trigger: None,
        }
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

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        if cursor.is_empty() {
            let synced = self
                .messages
                .iter()
                .map(|env| {
                    let body =
                        self.bodies
                            .get(&env.provider_id)
                            .cloned()
                            .unwrap_or_else(|| MessageBody {
                                message_id: env.id.clone(),
                                text_plain: None,
                                text_html: None,
                                attachments: vec![],
                                fetched_at: chrono::Utc::now(),
                                metadata: Default::default(),
                            });
                    SyncedMessage {
                        envelope: env.clone(),
                        body,
                    }
                })
                .collect();
            // Any non-empty cursor signals "initial sync complete";
            // subsequent calls take the steady-state branch below and
            // return empty batches.
            Ok(SyncBatch {
                upserted: synced,
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::from_bytes(b"fake-synced".to_vec()),
                has_more: false,
            })
        } else {
            Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: cursor.clone(),
                has_more: false,
            })
        }
    }

    async fn fetch_message(
        &self,
        provider_message_id: &str,
    ) -> Result<Option<SyncedMessage>, MxrError> {
        let Some(envelope) = self
            .messages
            .iter()
            .find(|message| message.provider_id == provider_message_id)
            .cloned()
        else {
            return Ok(None);
        };

        let body = self
            .bodies
            .get(provider_message_id)
            .cloned()
            .unwrap_or_else(|| MessageBody {
                message_id: envelope.id.clone(),
                text_plain: None,
                text_html: None,
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            });

        Ok(Some(SyncedMessage { envelope, body }))
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

#[async_trait]
impl MailSendProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    async fn send(
        &self,
        draft: &Draft,
        _from: &Address,
        rfc2822_message_id: &str,
    ) -> Result<SendReceipt, MxrError> {
        self.sent_guard().push(draft.clone());
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }

    async fn save_draft(
        &self,
        _draft: &Draft,
        _from: &Address,
    ) -> Result<Option<String>, MxrError> {
        Ok(Some(format!("fake-draft-{}", uuid::Uuid::now_v7())))
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
            reply_headers: None,
            intent: mxr_core::DraftIntent::Reply,
            to: vec![reply.to.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: reply.subject.clone(),
            body_markdown: format!("{}\n\n{}", reply.body_text, reply.ics),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
        let _ = from;
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-calendar-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
            rfc2822_message_id: rfc2822_message_id.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

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
        // Bodies are eagerly fetched during sync
        assert!(batch.upserted[0].body.text_plain.is_some());
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
        assert!(matches!(
            mutations[0],
            RecordedMutation::Trashed { .. }
        ));
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
            intent: DraftIntent::New,
            reply_headers: None,
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test".to_string(),
            body_markdown: "Hello".to_string(),
            attachments: vec![],
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
