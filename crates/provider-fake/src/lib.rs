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
use mxr_core::{MailSendProvider, MailSyncProvider, MxrError, SendReceipt, SyncCapabilities};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct FakeProvider {
    account_id: AccountId,
    messages: Vec<Envelope>,
    bodies: HashMap<String, MessageBody>,
    labels: Mutex<Vec<Label>>,
    sent: Mutex<Vec<Draft>>,
    mutations: Mutex<Vec<Mutation>>,
}

#[derive(Debug, Clone)]
pub enum Mutation {
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
}

impl FakeProvider {
    pub fn new(account_id: AccountId) -> Self {
        let (messages, bodies, labels) = crate::fixtures::generate_fixtures(&account_id);
        Self {
            account_id,
            messages,
            bodies,
            labels: Mutex::new(labels),
            sent: Mutex::new(Vec::new()),
            mutations: Mutex::new(Vec::new()),
        }
    }

    pub fn sent_drafts(&self) -> Vec<Draft> {
        self.sent.lock().unwrap().clone()
    }

    pub fn mutations(&self) -> Vec<Mutation> {
        self.mutations.lock().unwrap().clone()
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
            labels: true,
            server_search: false,
            delta_sync: false,
            push: false,
            batch_operations: false,
            native_thread_ids: true,
        }
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(self.labels.lock().unwrap().clone())
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        match cursor {
            SyncCursor::Initial => {
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
                Ok(SyncBatch {
                    upserted: synced,
                    deleted_provider_ids: vec![],
                    label_changes: vec![],
                    next_cursor: SyncCursor::Gmail { history_id: 1 },
                })
            }
            _ => Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: cursor.clone(),
            }),
        }
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, MxrError> {
        Ok(b"fake attachment content".to_vec())
    }

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<(), MxrError> {
        self.mutations
            .lock()
            .unwrap()
            .push(Mutation::LabelsModified {
                provider_id: provider_message_id.to_string(),
                added: add.to_vec(),
                removed: remove.to_vec(),
            });
        Ok(())
    }

    async fn create_label(&self, name: &str, color: Option<&str>) -> Result<Label, MxrError> {
        let label = Label {
            id: LabelId::from_provider_id("fake", name),
            account_id: self.account_id.clone(),
            name: name.to_string(),
            kind: LabelKind::User,
            color: color.map(str::to_string),
            provider_id: name.to_string(),
            unread_count: 0,
            total_count: 0,
        };
        self.labels.lock().unwrap().push(label.clone());
        Ok(label)
    }

    async fn rename_label(
        &self,
        provider_label_id: &str,
        new_name: &str,
    ) -> Result<Label, MxrError> {
        let mut labels = self.labels.lock().unwrap();
        let label = labels
            .iter_mut()
            .find(|label| label.provider_id == provider_label_id)
            .ok_or_else(|| MxrError::NotFound(format!("label {provider_label_id}")))?;
        label.id = LabelId::from_provider_id("fake", new_name);
        label.name = new_name.to_string();
        label.provider_id = new_name.to_string();
        Ok(label.clone())
    }

    async fn delete_label(&self, provider_label_id: &str) -> Result<(), MxrError> {
        let mut labels = self.labels.lock().unwrap();
        let before = labels.len();
        labels.retain(|label| label.provider_id != provider_label_id);
        if labels.len() == before {
            return Err(MxrError::NotFound(format!("label {provider_label_id}")));
        }
        Ok(())
    }

    async fn trash(&self, provider_message_id: &str) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::Trashed {
            provider_id: provider_message_id.to_string(),
        });
        Ok(())
    }

    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::ReadSet {
            provider_id: provider_message_id.to_string(),
            read,
        });
        Ok(())
    }

    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::StarredSet {
            provider_id: provider_message_id.to_string(),
            starred,
        });
        Ok(())
    }
}

#[async_trait]
impl MailSendProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    async fn send(&self, draft: &Draft, _from: &Address) -> Result<SendReceipt, MxrError> {
        self.sent.lock().unwrap().push(draft.clone());
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
        })
    }

    async fn save_draft(
        &self,
        _draft: &Draft,
        _from: &Address,
    ) -> Result<Option<String>, MxrError> {
        Ok(Some(format!("fake-draft-{}", uuid::Uuid::now_v7())))
    }
}

#[cfg(test)]
mod tests {
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

    #[tokio::test]
    async fn sync_initial_returns_all_with_bodies() {
        let provider = FakeProvider::new(AccountId::new());
        let batch = provider.sync_messages(&SyncCursor::Initial).await.unwrap();
        assert_eq!(batch.upserted.len(), 55);
        // Bodies are eagerly fetched during sync
        assert!(batch.upserted[0].body.text_plain.is_some());
    }

    #[tokio::test]
    async fn sync_delta_returns_empty() {
        let provider = FakeProvider::new(AccountId::new());
        let batch = provider
            .sync_messages(&SyncCursor::Gmail { history_id: 1 })
            .await
            .unwrap();
        assert_eq!(batch.upserted.len(), 0);
    }

    #[tokio::test]
    async fn mutations_recorded() {
        let provider = FakeProvider::new(AccountId::new());
        provider.trash("fake-msg-1").await.unwrap();
        provider.set_read("fake-msg-2", true).await.unwrap();
        provider
            .modify_labels("fake-msg-3", &["work".to_string()], &[])
            .await
            .unwrap();

        let mutations = provider.mutations();
        assert_eq!(mutations.len(), 3);
    }

    #[tokio::test]
    async fn send_recorded() {
        let provider = FakeProvider::new(AccountId::new());
        let draft = Draft {
            id: DraftId::new(),
            account_id: provider.account_id().clone(),
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let from = Address {
            name: Some("User".to_string()),
            email: "user@example.com".to_string(),
        };
        provider.send(&draft, &from).await.unwrap();
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
